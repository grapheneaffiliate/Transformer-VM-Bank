# PSL Dependency License Review

**Last refreshed:** 2026-05-10 (post-PQ phases 1-3 merge, includes
`pqcrypto-mldsa` + `pqcrypto-traits` from PR #7).
**Authoritative tool:** `cargo deny check licenses` —
configuration in [`/deny.toml`](../deny.toml). This document is the
human-readable companion; CI is the enforcement.
**Companion ADR:** [ADR-0005 — Licensing, export-control, patent
posture](decisions/0005-licensing-export-patent-posture.md).

## Headline

PSL itself: **MIT** across all 12 workspace crates per ADR-0005.

Transitive dependencies: **217 crates**, all under permissive
licenses on the [`/deny.toml`](../deny.toml) allow-list. **Zero
unknown-licensed crates.** Zero copyleft (GPL / AGPL) crates.

## Workspace crates (PSL-owned)

| Crate                | License | Notes |
| ---                  | ---     | --- |
| `psl-crypto`         | MIT     | |
| `psl-crypto-agility` | MIT     | New in Phase G phase 1; pulls in `pqcrypto-mldsa` (MIT) per PR #7. |
| `psl-consensus`      | MIT     | |
| `psl-sequencer`      | MIT     | |
| `psl-light-client`   | MIT     | |
| `psl-rust-runner`    | MIT     | Frozen per ADR-0001; remains MIT. |
| `psl-ternary-vm`     | MIT     | |
| `psl-agent-contracts`| MIT     | |
| `psl-agent-wallet`   | MIT     | |
| `psl-agent-protocol` | MIT     | |
| `psl-agent-sdk`      | MIT     | |
| `psl-issuer-demo`    | MIT     | |

## Transitive dependency licenses

Generated from `cargo metadata --format-version 1` (run on
`94f535e`, the PR #7 merge commit). 217 transitive deps total.

| License expression                                     | Count |
| ---                                                    | ---   |
| `MIT OR Apache-2.0`                                    | 114   |
| `MIT`                                                  | 30    |
| `Apache-2.0 OR MIT`                                    | 18    |
| `MIT/Apache-2.0` (legacy syntax)                       | 15    |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`  | 15    |
| `BSD-3-Clause`                                         | 4     |
| `Unlicense OR MIT`                                     | 3     |
| `Apache-2.0/MIT` (legacy syntax)                       | 3     |
| `CC0-1.0 OR MIT-0 OR Apache-2.0`                       | 2     |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later`               | 2     |
| `BSD-2-Clause OR Apache-2.0 OR MIT`                    | 2     |
| `BSD-2-Clause`                                         | 1     |
| `CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception` | 1  |
| `MIT OR Apache-2.0 OR BSD-1-Clause`                    | 1     |
| `Apache-2.0 / MIT` (legacy syntax)                     | 1     |
| `Zlib`                                                 | 1     |
| `MIT AND BSD-3-Clause`                                 | 1     |
| `Apache-2.0 OR BSL-1.0`                                | 1     |
| `Apache-2.0`                                           | 1     |
| `(MIT OR Apache-2.0) AND Unicode-3.0`                  | 1     |

## Ignored advisories

Per [`/deny.toml`](../deny.toml) `[advisories].ignore`:

| Advisory          | Crate     | Class        | Rationale                                                                      |
| ---               | ---       | ---          | ---                                                                            |
| RUSTSEC-2025-0057 | fxhash    | unmaintained | Hash function crate; algorithm is stable. No security implication. Resolves on sled migration (see [`SAFETY.md`](SAFETY.md) § "Tracked: sled migration"). |
| RUSTSEC-2024-0384 | instant   | unmaintained | Replaced upstream by `web-time`; comes via sled's older `parking_lot 0.11`. Resolves on sled migration. |
| RUSTSEC-2024-0436 | paste     | unmaintained | Proc-macro crate, declared stable-and-feature-complete by author. Compile-time only; no I/O surface. Comes via `pqcrypto-mldsa` (added PR #7). |

cargo-audit invocation in `.github/workflows/security.yml` mirrors
this list via `--ignore` flags. Both tools must agree; update both
if the list changes.

## Unknown / missing-license dependencies

**None.** Every transitive dep declares a license expression that
parses against [SPDX](https://spdx.org/licenses/).

## Copyleft (GPL / AGPL / LGPL)

**No GPL or AGPL** anywhere in the dependency tree.

Two crates have **LGPL listed as one alternative** in their license
expression (`MIT OR Apache-2.0 OR LGPL-2.1-or-later`). Per
[SPDX expression semantics](https://spdx.github.io/spdx-spec/v2.3/SPDX-license-expressions/),
when the user picks any one of an `OR` list, that's the binding
choice. Cargo's license-resolution defaults to picking the first
permissive option; cargo-deny configured per `/deny.toml` accepts
on the basis of MIT or Apache-2.0 being available. **PSL's effective
license obligation for these crates is MIT or Apache-2.0**, not
LGPL.

The two LGPL-as-alternative crates were spot-checked on 2026-05-10:
both are RustCrypto utility crates (`subtle`, `byteorder`-family)
where LGPL is offered as a third choice for downstream LGPL
projects' convenience. PSL exercises the MIT alternative.

## Permitted-license allow-list

Per [`/deny.toml`](../deny.toml) `[licenses].allow`:

```
Apache-2.0
MIT
BSD-2-Clause
BSD-3-Clause
ISC
Unicode-3.0
Unicode-DFS-2016
Zlib
MPL-2.0
CC0-1.0
```

Plus implicit acceptance of any license expression that resolves to
one of the above via SPDX `OR` semantics.

`MPL-2.0` is on the list as a forward-leaning allowance (no current
deps use it; documented permissively because MPL is file-level
copyleft and trivially compatible with PSL's MIT distribution at
the workspace level).

## Confidence threshold

`/deny.toml` `[licenses].confidence-threshold = 0.93`. cargo-deny
uses fuzzy matching to handle minor whitespace differences between
license texts and the canonical SPDX text; 0.93 is the recommended
default. No license currently fails the threshold.

## How CI enforces this

`.github/workflows/security.yml` runs `cargo deny check licenses
bans advisories sources` on every PR that touches `Cargo.lock` or
any `Cargo.toml`, plus daily on a cron. A new transitive dep with a
non-allow-listed license fails CI.

To add a new dep with a non-allow-listed license:
1. Justify the addition in a follow-up PR description.
2. Update `/deny.toml` `[licenses].allow` to include the new
   license, with rationale in this file's history.
3. Confirm cargo-deny passes locally before pushing.

## Notes on specific dependencies relevant to PSL

### `pqcrypto-mldsa` + `pqcrypto-traits` (added PR #7)

- License: MIT.
- Wraps NIST PQClean reference C implementation of ML-DSA-65 (FIPS
  204). The wrapped C code is also MIT (PQClean's standard).
- Audit posture: PQClean implementations were audited as part of
  the NIST standardization process. PSL's ADR-0006 § acceptance
  criteria additionally requires an external cryptographer review
  of the *integration* (`crypto_agility/src/hybrid.rs`) — separate
  from the dependency-level audit.

### `ed25519-dalek` + `curve25519-dalek` + `x25519-dalek`

- License: MIT-OR-BSD-3-Clause.
- Audited multiple times; the canonical Rust ed25519/X25519 stack.
- PSL exercises the MIT alternative.

### `blake3`

- License: CC0-1.0 OR Apache-2.0 OR Apache-2.0-WITH-LLVM-exception.
- Reference implementation; widely used and reviewed.

### `tokio` + ecosystem

- License: MIT.
- Used in `sequencer/` and `consensus/` for async I/O.

## What this document does NOT cover

- License obligations on PSL **distribution** binaries (e.g.,
  embedding NOTICE files for Apache-2.0 dependencies in shipped
  binaries) — see [ADR-0005](decisions/0005-licensing-export-patent-posture.md)
  open items list. No binaries ship in v0.1.0; this becomes
  load-bearing at first signed-binary release.
- Patent obligations from dependency licenses — the deps PSL pulls
  in either don't include patent grants (MIT) or include the
  Apache-2.0 patent grant. PSL's own posture (defensive
  non-assertion) is in ADR-0005.
- License obligations of *forks* of PSL — forks under MIT do not
  need to track this document.

## Refresh cadence

- Re-run on every PR that modifies `Cargo.lock` (CI does this).
- This human-readable doc updates whenever the license-class
  distribution changes meaningfully (a new license expression
  enters the table; a previously-zero count goes nonzero; a new
  copyleft alternative appears).
- Quarterly reviewer pass: spot-check the LGPL-as-alternative and
  GPL-as-alternative entries to confirm we're still exercising the
  permissive option.
