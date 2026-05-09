# Contributing to PSL

Thanks for your interest. PSL is settlement infrastructure — the bar
for changes is high, but the path is well-defined.

## Before you start

1. Read the README. PSL has unusual properties (no fp64 on verifier
   path, no `unwrap()` in production paths, ternary integer
   contracts). Changes that violate these get rejected on review,
   no matter how well-implemented.
2. Read `docs/ARCHITECTURE.md` — especially § 0 (trust boundary,
   trace-hash contract).
3. Read the operating principles section of the README.
4. Skim recent ADRs in `docs/decisions/` so you understand which
   doors have already been closed and why.

## Filing an issue

- For bugs: include the exact command, the expected behavior, and
  the observed behavior. If you can produce a failing test, that's
  the gold standard.
- For feature requests: open a discussion first, not a PR. Many
  features have been considered and explicitly deferred (see ADRs);
  it saves both of us time.
- For security issues: **do not file a public GitHub issue.** See
  `SECURITY.md`.

## Filing a pull request

The CI gates (`.github/workflows/ci.yml`) must pass:
- `cargo build --workspace --release`
- `cargo test --workspace --release`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `tools/ci/check_legacy_isolation.sh`

Code-quality expectations beyond the linters:

- **No `unwrap()` / `expect()` in production paths** unless it falls
  into one of the audited categories (lock-poison or
  structurally-impossible-overflow). See `docs/UNWRAP_AUDIT.md`. New
  unwraps need to be added to that audit document.
- **No floating point on the verifier path.** This is non-negotiable.
  If you need fp arithmetic for tooling/diagnostics outside the
  verifier, that's fine; flag it explicitly in the PR.
- **No silent failures.** All input-driven errors return `Result`.
- **Tests are the spec.** Anything you want to be true should be
  asserted in a test, including adversarial scenarios.
- **No new Lean sorrys** in load-bearing theorems.

If your change touches the trust model, the trace-hash contract, or
any ADR'd decision, write an ADR first. ADRs land as a separate
commit before the implementation.

## Commit messages

- Imperative mood ("add X", "fix Y", not "added X" or "fixes Y").
- First line ≤72 chars.
- Body wrapped at ~72 chars; explain *why*, not *what*.
- Reference the gate or ADR number when applicable.

We do not require sign-off lines or DCO at this stage. By submitting
a PR, you agree your contribution is licensed under MIT (per
`LICENSE` and ADR-0005).

## Tests

- Unit tests live in `#[cfg(test)] mod tests` blocks alongside the
  code.
- Integration tests live in `<crate>/tests/`.
- Property tests use `proptest` and live in
  `<crate>/tests/proptest_invariants.rs`.
- Adversarial / dispute scenarios live in
  `<crate>/tests/adversarial_*.rs`.
- Fuzz harnesses live in `<crate>/fuzz/fuzz_targets/`. See
  `docs/FUZZING.md`.

Anything labeled `#[ignore]` is Tier-2 reproducibility (long-running,
needs extra setup); see `REPRODUCE.md`.

## Documentation

- ADRs go in `docs/decisions/`.
- Runbooks go in `docs/runbooks/`.
- Architecture deep-dives go in `docs/`.
- README is the on-ramp; do not let it grow to a kitchen-sink doc.

## Code of conduct

See `CODE_OF_CONDUCT.md`.

## Maintainer review timeline

We aim for a first response within 5 business days. PSL is volunteer
development; reviews are not always immediate. If a PR has been idle
for >2 weeks without response, ping it once.
