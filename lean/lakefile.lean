import Lake
open Lake DSL

package PSL where
  -- Core invariants of the Percepta Settlement Layer.
  -- See PSL/{Conservation,Determinism,MPT}.lean for the load-bearing theorems.

require mathlib from git
  "https://github.com/leanprover-community/mathlib4.git" @ "v4.12.0"

@[default_target]
lean_lib PSL where
  globs := #[.andSubmodules `PSL]
