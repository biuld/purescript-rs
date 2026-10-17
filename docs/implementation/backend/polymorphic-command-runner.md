# Polymorphic command runner

**Design:** [Library-owned effects](../../design/backend/fp/library-owned-effects.md)
and [Native Effect/ST](../../design/backend/fp/native-effect-and-st-handlers.md).

## Problem and ownership

The unchanged official `passing/Sequence.purs` selects
`main :: Effect (List Unit)`. The locked package's runner accepted only
`Effect Unit`, so compilation stopped at P7 command normalization. A command
must execute the action and ignore its returned value; it need not restrict
that value to Unit.

The driver still resolves exactly the configured source runner and selects the
entry. Core owns checked application construction. The library owns execution
and result-discarding policy. No compiler stage recognizes an Effect name or
invents an Effect operation.

## Implementation contract

`command::normalize_entry` checks the runner's complete instance at
`entry_type -> Int` with `Module::checked_instantiation`, the same type relation
used by Core global verification. It does not implement a second type matcher.
Only declared quantified variables may instantiate; repeated variables have one
consistent replacement and nested scopes retain their normal rules.

The runner's declared result must be Int and the selected entry must remain
closed. Monomorphic runners and direct Int entries retain their behavior.
Invalid runner identities and input/result mismatches remain explicit errors.
New type nodes and the generated application are built on a candidate module;
failed validation does not publish a modified type table or entry.

The target-owned `PSRS.Command.run` becomes
`forall a. Effect a -> Int`. Its body still invokes `unsafePerformEffect` once
and returns zero after normal completion. Traps propagate. All 279 non-target
library source entries are byte-identical to the previous locked package.
The helper's target provenance hash and rationale are updated in the independent
package; official pure definitions and public Effect APIs are unchanged.

## Evidence

- `psrs-cli --test command_runner`: six tests pass under mandatory Wasmtime.
  They cover ordinary unrelated newtypes, direct Int entries, String/record/
  function results, repeated-variable inconsistency, wrong runner result,
  identity errors, and report/check-only paths.
- `Sequence.purs`: baseline P7 rejection at `fe6dd78f`; after the compiler and
  target-helper change it compiles, validates, prints exactly `Done\n`, and
  exits zero through the library-owned conformance runner.
- The independent package adds `CommandResult` and `CommandResultTrap` to its
  native conformance batch. They check one visible action, ignored function
  results, and trap propagation before a record result can be returned.
- Library tooling: `npm test`, 12 passed.

The four focused native driver tests also pass under mandatory Wasmtime. The
old test rejecting `Effect Int` was replaced by acceptance/execution evidence
and rejection of an unrelated Array carrier; the runner still accepts only an
instance of its declared input type.

The Native Effect batch has 14/15 accepted cases. Algebraic exceeded its
240-second compilation limit and passed an isolated retry with a 600-second
budget: 382.66 seconds to compile, 0.088 seconds to execute, exact output and
exit 42. The original failed batch is retained unchanged. Together those runs
provide executed evidence for all 15 cases on fingerprint
`fnv1a64-v1:4b052f09c153208f`; they are not reported as a green single batch.

The complete runtime board measured 273/340 program groups under this
command repair. The
[runtime accounting record](../runtime-program-scoreboard.md) keeps its scope
correction and remaining 67 outcomes separate from this command repair, and the
D-04 gate table carries the current number.

The independent package commit is
`50c26d67a78a6b25a9915b28fef981bc178cd767`, on local branch
`fix/polymorphic-command-runner` in `/tmp/psrs-command-runner-stdlib`. The
compiler lock at that measurement pins that revision and fingerprint. The
subsequent Symbol slice pins its descendant `6fcef6cc`; see the
[contextual application record](../frontend/contextual-application-results.md). Verification selects that
worktree explicitly with `PSRS_STDLIB_ROOT`; the sibling package's existing
checkout is preserved. The package commit has not been pushed, and no remote
CI result is claimed by these local runs.

Final Rust validation on 2026-10-17:

```sh
cargo fmt --all --check
PSRS_REQUIRE_WASMTIME=1 \
  PSRS_STDLIB_ROOT=/tmp/psrs-command-runner-stdlib cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

All commands pass. The workspace reports 2,034 passed and five ignored
scoreboards; those boards were executed separately with the release harness.
The final workspace log is `/tmp/psrs-polymorphic-runner-workspace-final.log`.
Earlier failures are retained: one run observed the sibling checkout switch,
and a later run exposed the obsolete Effect-Int rejection assertion. The final
run uses the isolated package and the corrected acceptance/rejection tests.

