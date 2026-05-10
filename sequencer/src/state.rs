//! In-memory state: account SMT + issuer registry SMT.
//!
//! Persistence is via the sled blockstore (block → state-snapshot pairs);
//! the live state is held in memory for fast trace assembly.

use psl_crypto::{Account, Hash, MerkleProof, SparseMerkleTree};

pub struct State {
    pub accounts: SparseMerkleTree,
    pub registry: SparseMerkleTree,
}

impl State {
    pub fn new() -> Self {
        Self {
            accounts: SparseMerkleTree::new(),
            registry: SparseMerkleTree::new(),
        }
    }

    pub fn account(&self, pubkey: &Hash) -> Account {
        match self.accounts.get(pubkey) {
            Some(bytes) if bytes.len() == 64 => {
                let mut buf = [0u8; 64];
                buf.copy_from_slice(bytes);
                Account { bytes: buf }
            }
            _ => {
                let mut a = Account::default();
                a.bytes[..32].copy_from_slice(pubkey);
                a
            }
        }
    }

    pub fn put_account(&mut self, account: Account) -> Hash {
        let pk = account.pubkey();
        self.accounts.put(pk, account.bytes.to_vec())
    }

    pub fn account_proof(&self, pubkey: &Hash) -> MerkleProof {
        self.accounts.proof(pubkey)
    }

    pub fn accounts_root(&self) -> Hash {
        self.accounts.root()
    }

    pub fn registry_root(&self) -> Hash {
        self.registry.root()
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
