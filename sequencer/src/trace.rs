//! Transformer-trace executor.
//!
//! This is the bridge from the sequencer (Rust) to the specialized
//! transformer (Python on day one, Rust at Phase 1.5). The `TraceExecutor`
//! trait abstracts over implementations:
//!
//!   - `NativeTraceExecutor`: computes the state delta directly in Rust by
//!     mirroring the C primitive logic. Used for unit tests and the v1
//!     sovereign pilot when transformer-VM isn't available. Trace_hash is a
//!     fixed marker that does NOT match a real transformer trace; this
//!     mode is for development only.
//!
//!   - `SubprocessTraceExecutor`: invokes Transformer-VM's `wasm-run`
//!     binary per primitive, captures predicted token stream, computes
//!     real `trace_hash`. Production path until the Rust runner lands.
//!
//!   - `RustTraceExecutor` (Phase 1.5): pure-Rust port of the runner.
//!     Bit-exact-identical output to the Python path; ≥10× throughput.

use anyhow::{anyhow, Result};
use psl_crypto::{hash_trace_owned, Account, Hash};
use std::path::PathBuf;
use std::process::Command;

use crate::tx::{SignedTx, TxKind};

/// Result of running a single primitive: the deltas to apply, plus the
/// trace_hash that the block header will commit.
#[derive(Clone, Debug)]
pub struct TraceResult {
    pub updated_accounts: Vec<Account>,
    pub trace_hash: Hash,
    pub success: bool,
}

pub trait TraceExecutor: Send + Sync {
    fn execute(&self, tx: &SignedTx, witness: &Witness) -> Result<TraceResult>;
}

/// Witness: pre-validated input account state for one tx.
#[derive(Clone, Debug)]
pub struct Witness {
    pub epoch: u32,
    /// 1–4 input accounts, in the order each primitive expects.
    pub accounts: Vec<Account>,
    /// u128 little-endian amount.
    pub amount: [u8; 16],
    pub flag: u8,
}

// ── Native executor (development / unit tests) ──────────────────────────────

pub struct NativeTraceExecutor;

impl TraceExecutor for NativeTraceExecutor {
    fn execute(&self, tx: &SignedTx, w: &Witness) -> Result<TraceResult> {
        match tx.kind {
            TxKind::Freeze => self.freeze(w),
            TxKind::Transfer => self.transfer(w),
            TxKind::Mint => self.mint(w),
            TxKind::Burn => self.burn(w),
            TxKind::MultiAsset => self.multi_asset(w),
        }
    }
}

/// Per-tx trace-hash counts under the per-byte composition design.
/// (Used by sequencer + followers to verify block format consistency.)
pub fn expected_trace_hash_count(kind: TxKind, multi_n: usize) -> usize {
    match kind {
        TxKind::Freeze => 2,                 // freeze_setup + freeze_apply
        TxKind::Transfer => 34,              // 1 check + 16 sub + 16 add + 1 finalize
        TxKind::Mint => 16,                  // 16 byte_add
        TxKind::Burn => 17,                  // 1 check + 16 byte_sub
        TxKind::MultiAsset => multi_n * 34,  // N inner transfers
    }
}

impl NativeTraceExecutor {
    fn freeze(&self, w: &Witness) -> Result<TraceResult> {
        if w.accounts.len() != 1 {
            return Err(anyhow!("freeze needs 1 account"));
        }
        let mut a = w.accounts[0];
        a.set_frozen(w.flag != 0);
        Ok(TraceResult {
            updated_accounts: vec![a],
            trace_hash: NATIVE_TRACE_MARKER,
            success: true,
        })
    }

    fn transfer(&self, w: &Witness) -> Result<TraceResult> {
        if w.accounts.len() != 2 {
            return Err(anyhow!("transfer needs 2 accounts"));
        }
        let mut from = w.accounts[0];
        let mut to = w.accounts[1];
        let amount = u128::from_le_bytes(w.amount);
        if from.is_frozen() || from.balance() < amount {
            return Ok(TraceResult {
                updated_accounts: vec![Account::default(), Account::default()],
                trace_hash: NATIVE_TRACE_MARKER,
                success: false,
            });
        }
        from.set_balance(from.balance() - amount);
        to.set_balance(to.balance().wrapping_add(amount));
        from.set_nonce(from.nonce() + 1);
        from.set_last_active(w.epoch as u64);
        to.set_last_active(w.epoch as u64);
        Ok(TraceResult {
            updated_accounts: vec![from, to],
            trace_hash: NATIVE_TRACE_MARKER,
            success: true,
        })
    }

    fn mint(&self, w: &Witness) -> Result<TraceResult> {
        if w.accounts.len() != 1 {
            return Err(anyhow!("mint needs 1 account"));
        }
        let mut to = w.accounts[0];
        let amount = u128::from_le_bytes(w.amount);
        let new_balance = to.balance().checked_add(amount);
        match new_balance {
            Some(b) => {
                to.set_balance(b);
                to.set_last_active(w.epoch as u64);
                Ok(TraceResult {
                    updated_accounts: vec![to],
                    trace_hash: NATIVE_TRACE_MARKER,
                    success: true,
                })
            }
            None => Ok(TraceResult {
                updated_accounts: vec![Account::default()],
                trace_hash: NATIVE_TRACE_MARKER,
                success: false,
            }),
        }
    }

    fn multi_asset(&self, w: &Witness) -> Result<TraceResult> {
        // multi_asset is N chained transfers. The witness's accounts vec carries
        // all 2N participating accounts in (from_0, to_0, from_1, to_1, ...) order.
        // For NativeTraceExecutor we apply each transfer in turn with the same
        // amount/epoch, accumulating updates.
        if w.accounts.len() % 2 != 0 || w.accounts.is_empty() {
            return Err(anyhow!("multi_asset needs an even, nonzero number of accounts"));
        }
        let amount = u128::from_le_bytes(w.amount);
        let mut updated = Vec::with_capacity(w.accounts.len());
        let n_pairs = w.accounts.len() / 2;
        let mut all_success = true;
        for i in 0..n_pairs {
            let mut from = w.accounts[2 * i];
            let mut to = w.accounts[2 * i + 1];
            if from.is_frozen() || from.balance() < amount {
                all_success = false;
                updated.push(Account::default());
                updated.push(Account::default());
                continue;
            }
            from.set_balance(from.balance() - amount);
            to.set_balance(to.balance().wrapping_add(amount));
            from.set_nonce(from.nonce() + 1);
            from.set_last_active(w.epoch as u64);
            to.set_last_active(w.epoch as u64);
            updated.push(from);
            updated.push(to);
        }
        Ok(TraceResult { updated_accounts: updated, trace_hash: NATIVE_TRACE_MARKER, success: all_success })
    }

    fn burn(&self, w: &Witness) -> Result<TraceResult> {
        if w.accounts.len() != 1 {
            return Err(anyhow!("burn needs 1 account"));
        }
        let mut from = w.accounts[0];
        let amount = u128::from_le_bytes(w.amount);
        if from.balance() < amount {
            return Ok(TraceResult {
                updated_accounts: vec![Account::default()],
                trace_hash: NATIVE_TRACE_MARKER,
                success: false,
            });
        }
        from.set_balance(from.balance() - amount);
        from.set_last_active(w.epoch as u64);
        Ok(TraceResult {
            updated_accounts: vec![from],
            trace_hash: NATIVE_TRACE_MARKER,
            success: true,
        })
    }
}

const NATIVE_TRACE_MARKER: Hash = [0xCA; 32];

// ── Subprocess executor (calls Transformer-VM's wasm-run) ───────────────────

pub struct SubprocessTraceExecutor {
    pub transformer_vm_path: PathBuf,
    pub weights_dir: PathBuf,
}

impl TraceExecutor for SubprocessTraceExecutor {
    fn execute(&self, tx: &SignedTx, w: &Witness) -> Result<TraceResult> {
        let primitive_name = match tx.kind {
            TxKind::Freeze => "ledger_freeze",
            TxKind::Transfer => "ledger_transfer",
            TxKind::Mint => "ledger_mint",
            TxKind::Burn => "ledger_burn",
            TxKind::MultiAsset => "ledger_multi_asset",
        };

        // Build input token sequence: "start" + hex-encoded witness bytes.
        let mut tokens: Vec<String> = vec!["start".to_string()];
        encode_witness(tx, w, &mut tokens);

        // Write tempfile, invoke wasm-run, capture predicted tokens.
        let tmp = tempfile::NamedTempFile::new()?;
        std::fs::write(tmp.path(), tokens.join(" "))?;

        let weights = self.weights_dir.join(format!("{primitive_name}.bin"));
        if !weights.exists() {
            return Err(anyhow!("weights missing: {}", weights.display()));
        }

        let output = Command::new("uv")
            .arg("run")
            .arg("wasm-run")
            .arg("--python")
            .arg("--model")
            .arg(&weights)
            .arg("-v")
            .arg(tmp.path())
            .current_dir(&self.transformer_vm_path)
            .output()?;

        if !output.status.success() {
            return Err(anyhow!(
                "wasm-run failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let predicted_tokens = extract_tokens_from_verbose(&stdout)?;
        let trace_hash = hash_trace_owned(&predicted_tokens);
        let updated_accounts = decode_output_accounts(&predicted_tokens, &tx.kind)?;
        let success = !updated_accounts.is_empty();
        Ok(TraceResult { updated_accounts, trace_hash, success })
    }
}

fn encode_witness(tx: &SignedTx, w: &Witness, out: &mut Vec<String>) {
    out.push(format!("{:08x}", w.epoch));
    if matches!(tx.kind, TxKind::Freeze) {
        out.push(format!("{:02x}", w.flag));
    }
    for acc in &w.accounts {
        for byte in acc.bytes.iter() {
            out.push(format!("{byte:02x}"));
        }
    }
    if !matches!(tx.kind, TxKind::Freeze) {
        for byte in w.amount.iter() {
            out.push(format!("{byte:02x}"));
        }
    }
}

fn extract_tokens_from_verbose(stdout: &str) -> Result<Vec<String>> {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("  Tokens: ") {
            return Ok(rest.split_whitespace().map(|s| s.to_string()).collect());
        }
        if let Some(rest) = line.strip_prefix("Tokens: ") {
            return Ok(rest.split_whitespace().map(|s| s.to_string()).collect());
        }
    }
    Err(anyhow!("could not extract token sequence from wasm-run output"))
}

fn decode_output_accounts(tokens: &[String], kind: &TxKind) -> Result<Vec<Account>> {
    // Tokens after the input prefix that are 2-char hex are output bytes.
    // We collect contiguous hex bytes and parse them in 64-byte chunks.
    let mut bytes = Vec::new();
    let mut in_output = false;
    for t in tokens {
        if t == "OUT" {
            in_output = true;
            continue;
        }
        if t == "halt" {
            break;
        }
        if !in_output {
            continue;
        }
        if t.len() == 2 && t.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(b) = u8::from_str_radix(t, 16) {
                bytes.push(b);
            }
        }
    }

    let n_accounts = match kind {
        TxKind::Freeze | TxKind::Mint | TxKind::Burn => 1,
        TxKind::Transfer => 2,
        TxKind::MultiAsset => bytes.len() / 64,
    };
    if bytes.len() < n_accounts * 64 {
        return Err(anyhow!(
            "expected {} bytes of output, got {}",
            n_accounts * 64,
            bytes.len()
        ));
    }

    let mut out = Vec::with_capacity(n_accounts);
    for i in 0..n_accounts {
        let mut a = Account::default();
        a.bytes.copy_from_slice(&bytes[i * 64..(i + 1) * 64]);
        out.push(a);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keypair_acc(seed: u8) -> Account {
        Account::new([seed; 32])
    }

    #[test]
    fn native_freeze_sets_flag() {
        let exec = NativeTraceExecutor;
        let tx = SignedTx {
            kind: TxKind::Freeze,
            asset_id: 1,
            nonce: 0,
            signer: [1u8; 32],
            recipient: None,
            amount: [0u8; 16],
            flag: 1,
            court_order_hash: Some([0xab; 32]),
            multi_payload: None,
            originator_metadata: None,
            signature: [0u8; 64],
        };
        let w = Witness {
            epoch: 1,
            accounts: vec![keypair_acc(2)],
            amount: [0u8; 16],
            flag: 1,
        };
        let r = exec.execute(&tx, &w).unwrap();
        assert!(r.success);
        assert!(r.updated_accounts[0].is_frozen());
    }

    #[test]
    fn native_transfer_debits_credits() {
        let exec = NativeTraceExecutor;
        let mut from = keypair_acc(2);
        from.set_balance(1_000_000);
        let to = keypair_acc(3);
        let tx = SignedTx {
            kind: TxKind::Transfer,
            asset_id: 1,
            nonce: 1,
            signer: from.pubkey(),
            recipient: Some(to.pubkey()),
            amount: 100u128.to_le_bytes(),
            flag: 0,
            court_order_hash: None,
            multi_payload: None,
            originator_metadata: None,
            signature: [0u8; 64],
        };
        let w = Witness {
            epoch: 5,
            accounts: vec![from, to],
            amount: 100u128.to_le_bytes(),
            flag: 0,
        };
        let r = exec.execute(&tx, &w).unwrap();
        assert!(r.success);
        assert_eq!(r.updated_accounts[0].balance(), 999_900);
        assert_eq!(r.updated_accounts[1].balance(), 100);
        assert_eq!(r.updated_accounts[0].nonce(), 1);
    }

    #[test]
    fn native_transfer_rejects_overdraft() {
        let exec = NativeTraceExecutor;
        let mut from = keypair_acc(2);
        from.set_balance(50);
        let to = keypair_acc(3);
        let tx = SignedTx {
            kind: TxKind::Transfer,
            asset_id: 1,
            nonce: 1,
            signer: from.pubkey(),
            recipient: Some(to.pubkey()),
            amount: 100u128.to_le_bytes(),
            flag: 0,
            court_order_hash: None,
            multi_payload: None,
            originator_metadata: None,
            signature: [0u8; 64],
        };
        let w = Witness {
            epoch: 5,
            accounts: vec![from, to],
            amount: 100u128.to_le_bytes(),
            flag: 0,
        };
        let r = exec.execute(&tx, &w).unwrap();
        assert!(!r.success);
    }
}
