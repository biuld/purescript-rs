# Control Reference-Library Migration Baseline

**Date:** 2026-10-12.

**Starting compiler:** `7a6dae8985d39124a019bce7bd3b8b6a421c8166`, with an
execution-order documentation edit.

**Scope:** Prerequisites for the reference-library acceptance required by
[library-owned effects](../../design/backend/fp/algebraic-effects-and-handlers.md).
This is compile evidence, not complete library or delimited-control acceptance.

## Selected reference sources

| Package | Tag | Commit | Source modules |
| --- | --- | --- | --- |
| `natefaubion/purescript-run` | `v5.0.0` | `abec7c343e92154d44b9dafd52b91ee82d32a870` | 7 |
| `purescript/purescript-free` | `v7.0.0` | `e2d8fa8023a864363857834e11393483bced5e38` | 7 |
| `natefaubion/purescript-variant` | `v8.0.0` | `8b2bd73a4202257d80a955c4dd36f6055f149823` | 3 |

Clean tagged checkouts and per-module SHA-256 inventories are retained under
`/private/tmp/psrs-control-reference/`, in `reference-inventory.json`. These are
selected reference pins; the independent package has not imported these packages
or changed its lock. Complete transitive dependency pinning remains required.

## Latest migration state

The continuation from `3d320e29` repairs functional-dependency improvement for
recursively generated instance contexts. The same corrected Variant entry and
three unchanged upstream modules now pass P5 and produce Core. Diagnosis next
rejects at P8 WIT linking: `` `psrs:effect` is not `package/interface` ``. No CC/MIR
or executable is produced for the reference entry. The diagnostic's span matches
the legacy `effectPure` foreign declaration in the locked Prelude. Core's
`prune_unreachable` deliberately retains source-wide WIT/primitive conformance
obligations, and P8 validates dormant declarations too. This is not evidence for
weakening reachability or binding validation. The next migration slice must
address the remaining legacy standard-library declarations under that contract;
no native Effect ABI or VariantF representation failure is established here.

Upstream `Run` wraps `Free (VariantF r)` and imports Aff. Preserve its source
contract for reference acceptance. The ordinary row-based library over
`Prim.Control` is a separate implementation task; replacing upstream `Free`
with `Control` would not prove unchanged-source acceptance. Native Effect/Aff
execution remains outside this pure migration slice.

## First compile boundary and repair

The three unchanged Variant source modules are supplied explicitly to diagnosis
with the development package at `psrs-stdlib` revision
`5d523805c4e2096e68fab5a4f965f62b225ee2af`, source fingerprint
`fnv1a64-v1:a1e5a2f8f4157041`. The first rejection was P1
`ErrorParsingModule` at `Data.Variant.Internal` line 155: `t L.: ts`.

Pattern parsing now uses the same contiguous qualified-symbolic-name recognizer
as expression parsing. Constructor application stops before a qualified operator,
so a left operand such as `L.Wrap x` cannot consume the following qualifier.
The CST retains the operator name, full span and unreassociated operands.

Execution exposed a related resolver defect: imported fixities were registered
only as `Qualifier.(operator)`, while infix expressions and patterns use
`Qualifier.operator`. Both spellings now use the imported fixity in the common
value/type fixity tables. Symbol resolution and reassociation remain their
existing owners.

Two syntax tests cover names, spans, constructor operands, symbolic dots,
multi-component qualifiers and Unicode guarded alternatives. A driver test uses
`Foo.Bar.Wrap` operands and a right-associated `Foo.Bar.:` chain in both
construction and matching; mandatory Wasmtime execution returns 42. Official
`purs 0.15.16` accepts the same two-module case with a type-only foreign declaration
for `intAdd` (`qualified-pattern/official.json`, no errors).

## Diagnosis and next production blocker

`variant-before.json` and `variant-after.json` have identical ordered source
fingerprints. Their comparison advances from P1 to P3. The initial entry fixture
incorrectly imported `Data.Functor.Identity`; this remaining fixture error is not
a library/compiler defect. `VariantMain.purs` corrects it to `Data.Identity`.

The corrected entry, with the same three unchanged Variant modules, reaches P5
and reports one `NoInstanceFound` in `Data.Functor.Variant` line 90, at the
`foldableCons` instance context: `FoldableVFRL _T192 _T2090`.
`variant-entry.json` retains the ordered sources and observed pass prefix.
At the initial checkpoint it produced no accepted Core/CC/MIR or executable
artifact. The continuation below repairs the class/row constraint boundary and
reruns the unchanged package sources, preserving the instance and its constraints.

Official `purs` parsing of all three upstream Variant modules reports only
`ModuleNotFound` for the deliberately incomplete input closure, with no
`ErrorParsingModule`. This proves parsing only; official whole-library type and
runtime comparisons remain unverified.

## Initial validation

- `cargo fmt --all --check` and `git diff --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast`: nonzero,
  with the same 175 failed test names and nine failing targets as the
  `7a6dae89` root-validation baseline; no added or recovered failed names.
  Driver unit tests report 643 passed / 162 failed. Backend tests report
  435 passed, syntax tests 37 passed, and all 81 control integration tests pass.
  Failure-name equality does not establish identical failure causes.
- `PURESCRIPT_REPO=/Users/biu/Projects/purescript PSRS_ORACLE=annotations
  PSRS_SUITE_FILTER=Operator cargo test -p psrs-driver --test suite
  l1_parse_scoreboard_against_purs -- --ignored --nocapture`: passed,
  28/28 agreement (16 passing, 10 failing, 2 layout), one FFI exclusion.
  This is a filtered parser result, not a new full gate measurement.

Logs are retained in `workspace.log`, `workspace-comparison.json`, `clippy.log`
and `operator-scoreboard.log` under `/private/tmp/psrs-control-reference/`.
Full ignored scoreboards, complete official reference-library type/runtime
comparison, package audit and public package conformance were not run. No D-04
gate number or full AE/AC requirement status changes.

## Recursive instance-context improvement (2026-10-12)

The outer wanted-list fixed point improves constraints already in the worklist.
Selecting an instance subsequently creates fresh context obligations. Those
ordinary class obligations previously entered exact given lookup without the
same improvement. While constructing the `TraversableVFRL` cons superclass,
the selected `FoldableVFRL` cons instance introduces a fresh residual row. Its
recursive context can determine that row from the given `TraversableVFRL`'s
superclass and `rl -> row` dependency, but that improvement was missing. The
diagnostic cites the selected instance's context declaration at line 90; it is
not a failure in that instance's method body.

The shared `solve_constraint_with_chain` entry now runs the existing
`improve_one` operation before primitive dispatch, given lookup and instance
selection. Recursive contexts and requeued obligations use the same rule;
rigidity, branch commitment and exact given matching remain enforced by their
existing owners. No operation/library identity participates in the fix.

Three driver tests cover a reduced RowList/row superclass declaration, its
missing-residual-dictionary rejection, and a user-class `Describe`/`Value`
dictionary path that executes to 42. Official `purs 0.15.16` accepts both
positive programs and rejects the negative with `NoInstanceFound`; executing
the official user's class program with Node also returns 42.

The identical corrected Variant input closure advances from P5 rejection to P8
WIT rejection. `variant-nested-row-comparison.json` and
`variant-nested-row-after.json` retain the same ordered input fingerprints and
the produced Core. `nested-row-before.json`, `nested-row-after.json` and their
comparison record recovery of the reduced superclass declaration. All reports
remain under `/private/tmp/psrs-control-reference/`.

Validation:

- `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib nested_instance_context
  -- --nocapture`: three passed, including actual Wasmtime execution.
- `cargo fmt --all --check`, `git diff --check` and
  `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast`: nonzero, with
  the same 175 failed names / nine failing targets as `3d320e29`; no added or
  recovered failed names. Driver unit tests report 646 passed / 162 failed;
  typechecker unit tests 135 passed; all 81 control integration tests pass.
  Logs: `nested-row-workspace.log`, `nested-row-workspace-comparison.json` and
  `nested-row-clippy.log`. Failure-name equality does not prove equal causes.

Full ignored scoreboards, official reference-library execution, source audit
and public package conformance were not rerun. Neither package lock nor D-04
gate measurements change; full reference-library and delimited-control
acceptance remain outstanding.
