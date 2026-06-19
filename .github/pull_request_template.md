<!-- See CONTRIBUTING.md § "Filing a pull request" for the full bar. -->

## What & why

<!-- One paragraph. Link the issue/discussion/ADR this implements. -->

## CI gates (all must pass)

- [ ] `cargo build --workspace --release`
- [ ] `cargo test --workspace --release`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --exclude psl-rust-runner --all-targets -- -D warnings`
- [ ] `ruff check .` (if Python touched)
- [ ] `tools/ci/check_legacy_isolation.sh`

## Repo invariants

- [ ] No new `unwrap()`/`expect()` on production paths outside the audited categories (`docs/UNWRAP_AUDIT.md` updated if added)
- [ ] No floating point on the verifier path (fp in tooling/diagnostics is flagged explicitly below)
- [ ] No new Lean `sorry`s in load-bearing theorems
- [ ] `docs/INDEX.md` updated in this PR if any Markdown doc was added, moved, or removed
- [ ] Frozen code untouched (`legacy/` per ADR-0001, trace-hash v1 per ADR-0008) — or the PR explains why

## Tests

<!-- Tests are the spec. What new assertions pin this change,
     including adversarial cases? -->
