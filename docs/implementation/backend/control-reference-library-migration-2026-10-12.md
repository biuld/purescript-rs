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

## Official-source restoration and Partial crash contract (2026-10-12)

The development package carried a target-only `Effect`, `runEffect`, `trap`,
their `psrs:effect` foreign declarations and instances in `Prelude`. Official
`purescript-prelude` owns none of them, and the compiler no longer implements
the withdrawn `psrs:effect` protocol, so those declarations were both a source
fidelity violation and dead. The package restores the official source contract
first and lets the native Effect/ST design replace the withdrawn compiler
protocol afterwards.

- `lib/Prelude.purs`, `lib/Effect.purs` and `lib/Test/Assert.purs` are
  byte-identical to their pinned upstream sources again. `Effect` returns to its
  owning module; `Partial` now takes the checked non-returning storage trap for
  `_crashWith` and keeps its public signature and exports.
- `lib/WASI/*.purs` import `Effect (Effect)` because `Prelude` no longer supplies
  that type.
- The package commit is `407de2618ef2f5b43befc572bdc787d94287f1eb`, source
  fingerprint `fnv1a64-v1:f29cc57fc1e691f4`. `stdlib.lock.json` is updated to
  that revision and fingerprint; default locked consumption loads and passes.

`Partial._crashWith` reaches `Partial.Unsafe.unsafeCrashWith` through the
existing checked `psrs:runtime-storage#trap` operation. A focused program that
reaches `unsafeCrashWith` compiles, links and guest-traps (`wasm unreachable`);
the message has no Wasm trap payload and is not observable, which is the stated
representation boundary rather than a fabricated result.

With `_crashWith` resolved, the unchanged Variant entry advances past P8 library
linking. Its next first blocker is P8 closure conversion:
`aggregate and parameterized types are not supported by the first backend slice`
(`variant-crashfix.json`). That is real remaining backend work, not an Effect
or WIT failure, and no executable is produced for the reference entry yet.

Eight driver unit tests had been failing only because their fixtures used the
removed `Prelude.runEffect`/`Effect` API. They only resolve, typecheck or lower,
so they are migrated to `import Effect (Effect)` with the computation as
`main`. Two further fixtures that shadow the standard library were migrated for
the same reason.

Validation:

- Driver unit-test target: 740 passed / 68 failed, against 731 / 77 for the
  restoration candidate before the fixture migration. None of the eight added
  failure names remain; the remaining failures are runtime Effect/WASI cases
  blocked on native Effect execution or on the P8 closure-conversion slice.
  Log: `driver-after-fixture-migration.log`.
- Whole workspace with mandatory Wasmtime, default locked package:
  `error: 4 targets failed` (CLI `link`, CLI `source_layout`, driver `--lib`,
  driver `residual_constraints`) with 71 failed names, down from 80 for the
  restoration candidate and with no added names. Log:
  `workspace-after-lock-update.log`.
- `cargo fmt --all --check`: passed. `cargo clippy --workspace --all-targets --
  -D warnings`: passed. `npm test` in `psrs-stdlib`: 12 passed.
- Default locked consumption (no `PSRS_STDLIB_ROOT`): the migrated tests and a
  trivial program pass and report the new fingerprint.

Native `Effect.pureE`, `Effect.bindE`, the loop/traversal foreigns and
`Test.Assert.assertImpl` still have no target implementation and fail explicitly
at P8 library linking. Restoring the official declarations is not their runtime
implementation; native Effect/ST and full reference-library acceptance remain
outstanding.

## Reference-Variant compilation (2026-10-12)

With `_crashWith` resolved, the unchanged Variant entry advanced past P8 library
linking and stopped at `backend.cc.lower` with `aggregate and parameterized
types are not supported by the first backend slice`. Tracing the failure showed
it was not a general aggregate gap: `Data.Functor.Variant` declares
`data VariantF :: Row (Type -> Type) -> Type -> Type` / `data VariantF f a` and
`data UnknownF a`, all constructorless, and the layout pass rejected any user
type that was neither an enum, an aggregate, a newtype, nor an opaque foreign
type.

A constructorless algebraic data declaration is uninhabited: no expression can
build one, so its only values come from unchecked coercions (the upstream
modules coerce `VariantFRep f a`, a newtype over a record, to and from
`VariantF f a`). Such a type now uses the erased reference, exactly like a bare
abstract type. The rule is deliberately narrower than "any type without a
resolved layout": a type whose constructors were pruned by Core, or whose fields
are not layoutable, still takes its own error path. `foreign import data` is
excluded — a nullary foreign type is a resource handle and an applied one
already uses the erased reference.

Evidence:

- The unchanged reference entry and its three unchanged upstream Variant modules
  now pass diagnosis (`variant-uninhabited.json`) and execute under Wasmtime to
  `42`. Before the fix the first blocker was the layout rejection; this is the
  reference-library compile/execute milestone for the Variant package.
- `cc::layout::tests::uninhabited` covers the bare and applied forms and checks
  that a type with a constructor is not swallowed by the rule.
- `tests::uninhabited::a_constructorless_data_type_round_trips_through_unsafe_coercion`
  compiles and executes a self-contained coercion round-trip to `42`.
- Backend unit tests: 438 passed. Driver unit tests: 742 passed / 68 failed,
  with exactly the same 68 failure names as before the change, so no regression.
- Whole workspace with mandatory Wasmtime: the same four failing targets and 71
  failed names as before the change, with no added or recovered names. Log:
  `workspace-after-variant.log`.

The remaining groups are unchanged: native Effect/ST API execution, the
`Effect`/WASI runtime fixtures, the CLI link/source-layout and residual
constraint targets, and the other reference packages (`Run`, `Free`) plus the
ordinary row library over `Prim.Control`. The earlier "first backend slice"
closure-conversion message still covers genuinely unsupported shapes (for
example open rows and unresolved type constructors); those remain real work.

## Free/Run dependency closure (2026-10-12)

The reference `Free` package imports `Control.Monad.Trans.Class` and
`Data.CatList`, and its `Control.Monad.Free.Class` reaches
`Control.Monad.Except.Trans`, which imports `Effect.Exception`. Those modules
were not in the pinned package. The closure is now pinned at upstream tags:

| Package | Tag | Commit |
| --- | --- | --- |
| `purescript-catenable-lists` | `v7.0.0` | `09abe1f4888bc00841ad2b59e56a9e7ce7ebd4ab` |
| `purescript-transformers` | `v6.0.0` | `be72ab52357d9a665cbf93d73ba1c07c4b0957ee` |
| `purescript-exceptions` | `v6.0.0` | `afab3c07c820bb49b6c5be50049db46a964a6161` |
| `purescript-aff` (for Run) | `v7.1.0` | `6adec6ff048a7876f74c294c440374cd21342d39` |

With those sources supplied unchanged next to the seven `purescript-free`
modules and the entry `FreeMain.purs`, the Free closure has no missing module
and reaches P5. Reproduce with `psrs build FreeMain.purs <free + catenable +
transformers + exceptions sources>`; `free-files-list.txt` records the exact
source set in the evidence root.

Two real frontend defects blocked it and are fixed:

1. A local `where`/`let` binding signature is elaborated as an
   `ExprKind::Typed` node, and P4 desugaring recursed into the expression but
   passed the written type through unchanged. A type operator chain in a local
   signature therefore survived to kind checking as `UnloweredTypeOperator`
   (`Control.Monad.Free`'s `go :: Free f ~> Free g`). `desugar_expr` now
   desugars the type of both `Typed` and `TypeApplication`, matching the
   top-level and pattern paths.
2. A bare `(~>)` was unconditionally resolved to the builtin function arrow.
   That made `(->) Unit` report `PartiallyAppliedSynonym` (breaking
   `Control.Monad.Trampoline`'s `type Trampoline = Free ((->) Unit)`), while
   the official `(~>) Array` diagnostic only matched by coincidence. A bare
   `~>` now resolves through the imported type-operator alias (official Prelude
   aliases it to `NaturalTransformation`; a user may alias it to
   `Prim.Function`), and only an unresolved `~>` falls back to the arrow. The
   function arrow is a builtin constructor and is never a partially applied
   synonym inline. L3 kind agreement is unchanged at `PartiallyAppliedSynonym`
   12/12, so `failing/TypeSynonyms9.purs` and `PASTrumpsKDNU2.purs` still agree.

The next real blocker is **polymorphic local signatures** (rank-1 local
generalization). `Data.CatList`'s recursive `where` `foldl :: forall b c. ...`
and `Control.Monad.Free`'s `go :: Free f ~> Free g` both report `EscapedSkolem`
and `TypesDoNotUnify` at the recursive use. The written `forall` is
re-generalized against a rigid instantiation instead of being instantiated per
use, so the local binding is effectively monomorphic. This is a typechecker
slice, not a desugar or kind issue.

Run additionally needs `Effect.Aff` and `Effect.Aff.Class`. Its closure pulls
`refs`, `parallel`, and `datetime` on top of the modules above, and `Effect.Aff`
execution itself is gate-blocked on native Effect, so Run cannot be completed in
this pure source-migration track.

Validation for these fixes:

- Whole workspace with mandatory Wasmtime: the same four failing targets and 71
  failed names as before the change, with no added or recovered names. Log:
  `workspace-after-fixity.log`.
- L2 resolution, L3 kind, and L4/L5 type scoreboards with annotations ran and
  printed their reports; L3 `PartiallyAppliedSynonym` agreement is 12/12.
- `cargo fmt --all --check` and
  `cargo clippy --workspace --all-targets -- -D warnings`: passed. Regression
  tests cover the local-signature desugar and the partial function arrow.
