# Native Effect and ST: Proposed Acceptance Checklist

**Status:** NE-01–NE-13 Verified on 2026-10-16 at package fingerprint `fnv1a64-v1:ad19620ada1d2ecf`. The independent package commit and compiler lock revision are `4c73aab52226e8f5e7b1f17052f1a19d717a472e`. The D-04 runtime board was remeasured on 2026-10-17 at 273/413.

**Design:** [Native Effect and ST over algebraic handlers](../../design/backend/fp/native-effect-and-st-handlers.md).  
**Date:** 2026-10-11.

This checklist follows compiler protocol removal. Historical State-based
execution evidence is not evidence for this representation. The implementation
notes below record the adapters and focused tests. The coherent pin is
package commit `4c73aab52226e8f5e7b1f17052f1a19d717a472e`.

| ID | Requirement | Required evidence | Status |
| --- | --- | --- | --- |
| NE-01 | Public source fidelity | Pinned source audit; public exports/signatures/roles/classes/instances unchanged; native replacements explicitly justified | Verified |
| NE-02 | Shared Effect/ST representation | Generic source-field and representation evidence; executed official ST.Global.toEffect with persistent references and multiple result types | Verified |
| NE-03 | Inert construction and repeat execution | Public native API oracle for operation counts, strict factory arguments, delayed callbacks, and fresh allocations on repeated runs | Verified |
| NE-04 | Heterogeneous request typing and identity | Int/String/function/record cells in one program; rank-N transports, in-place writes across polymorphic boxing, read/write optimization order, and malformed generic representation rejection | Verified |
| NE-05 | Official Ref semantics | Aliasing, writes, read/modify' callback count and result, re-entrant unsafe callback behavior, and official oracle comparisons | Verified |
| NE-06 | Self-referential initialization | Callback executes once; published self-reference works; unpublished read traps under the documented target adaptation | Verified |
| NE-07 | ST region isolation | Official compiler differential cases for escaping references/actions, cross-region use, nominal Coercible rejection, constructor scope, and host-program injection | Verified |
| NE-08 | Safe mutable-array isolation | Thaw/freeze copy behavior; repeated writes; callbacks; growth and alias transport; explicit unsafe sharing distinguished from safe APIs | Verified |
| NE-09 | Deep stack-safe execution | Mandatory Wasmtime long public loops, MonadRec, bind chains, mixed native requests, and pure generic controls | Verified |
| NE-10 | Native host handler behavior | Actual Console/Clock/Arguments/resource and other supported service execution; exact output/value/call order and lifetime observations | Verified |
| NE-11 | Ordinary command/unsafe boundary | Configured runner executes once and returns zero; unsafePerformEffect repeats; traps propagate; no special Effect entry discovery | Verified |
| NE-12 | User effects interpret into native Effect | Handler constructs native requests without executing early; residual operation rows are preserved or rejected appropriately | Verified |
| NE-13 | Compiler neutrality | No State protocol, Effect discovery, native operation enum, hidden shape inference, or effect-specific optimizer exemption; generic regression evidence | Verified |

Implementation order follows semantic dependency: validate shared generic
representation and typed requests; implement native interpretation and region
boundaries; adapt public modules with fidelity evidence; execute complete API
conformance; only then consider a coherent package pin. Native continuation
support remains an independent decision and validation track.

The proposal itself established no runtime or differential evidence. The
implementation notes below record subsequent focused runs. Keep compiler/package
identities, commands, results, and skips separate; tests of the withdrawn
implementation do not verify these requirements.

## Representation prototype (2026-10-16)

The first implementation step, "validate shared generic representation and typed
requests", lives outside the package and is pinned by compiler regression tests.

The naive continuation-passing free monad
`data Native f a = Return a | Suspend (f (Native f a))` infers a **nominal**
result role: `a` occurs in `f`'s unknown parameter. Annotating
`type role Effect representational` over it is `RoleMismatch`
(`a_naive_free_monad_rejects_a_representational_effect_role`).

The current prototype follows upstream `Free`: the result parameter is phantom
because the view stores `Val`, and `NativeF`'s parameter is the operation
answer, not the rest of the program:

```purescript
data Val
data View f = Ret Val | Bound (f Val) (Val -> Native f Val)
data Native f a = Native (View f)

type Methods next z =
  { new      :: forall a. a -> (Cell a -> next) -> z
  , read     :: forall a. Cell a -> (a -> next) -> z
  , write    :: forall a. Cell a -> a -> (Unit -> next) -> z
  , clockNow :: (Int -> next) -> z
  , abort    :: Unit -> z
  }

newtype NativeF next = NativeF (forall z. Methods next z -> z)
newtype Effect a = Effect (Native NativeF a)
type role Effect representational
newtype ST r a = ST (Native NativeF a)
type role ST nominal representational
toEffect :: forall a. ST Global a -> Effect a
toEffect = unsafeCoerce
```

`liftF` embeds one answer-indexed request. `runNative` reads the payload from
`NativeF` and follows the `Free` continuation; it does not map that continuation
back into `NativeF`. `abort` is a `Unit -> z` thunk: a strict record field whose
value was `trap` aborted every interpretation. Cell storage uses the checked
`arrayFill`/`arrayWrite`/`arrayIndex` primitives. Official `Run` is not this
prototype's carrier: `purescript-run` imports `Effect`, which would cycle once
`Effect` wraps `Run`.

Evidence:

- `a_phantom_free_native_program_accepts_official_effect_and_st_roles` accepts
  the official role annotations.
- `st_global_to_effect_runs_heterogeneous_storage_and_host` allocates an `Int`
  cell and a `String` cell, writes both, reads the WASI monotonic clock, and
  returns `42` through both `runST` and `toEffect` then `runEffect`.

### Host leaves

A WIT import declared as `Effect X` cannot satisfy the ABI: `core_matches_result`
compares the import's resolved result against its canonical result and does not
unwrap `Effect`, so the checked `getStdin :: Effect InputStream` and its
neighbours fail. A raw host leaf declared at its canonical result instead
(`foreign import "wasi:clocks/monotonic-clock#now" clockNow :: Int`) validates
and executes; the interpreter turns it into a Host request and performs the
call.

### Safe constructors and region isolation (2026-10-16)

The prototype no longer exports `ST`/`Effect` constructors or `NativeProgram`.
Safe ST constructors wrap only storage requests (`new`/`read`/`write`). Host
`clockNow` is `Effect Int`. `STRef` and `ST` keep nominal region roles.

Evidence in `tests::native_representation`:

- `safe_st_and_effect_run_storage_host_and_fresh_refs` returns 42 through
  `run` twice, `runEffect`, `toEffect`, and two independently allocated `Ref`s
  (repeated `newRef` does not alias).
- `run_rejects_an_escaping_st_reference` and `run_rejects_an_escaping_st_action`
  are `EscapedSkolem`.
- `a_reference_cannot_be_read_in_another_region` is `TypesDoNotUnify`.
- `nominal_stref_region_rejects_coerce` is `TypesDoNotUnify`. The region parameter is nominal, so unequal regions are a determined mismatch (remeasured 2026-10-22).
- `host_clock_is_not_an_st_request` is `TypesDoNotUnify`.
- `the_st_constructor_is_not_exported` is `UnknownName`.

### Official adapters (2026-10-16)

`psrs-stdlib` now owns `PSRS.Effect.Native`. Official `Effect`, `Effect.Unsafe`,
`Effect.Ref`, and `Control.Monad.ST.Internal` wrap that program. Constructor
hiding and region isolation use the public modules.

Evidence in `tests::native_representation` and `tests::assertions` with the
development package fingerprint `fnv1a64-v1:c54b996e0f7e0295`:

- `official_st_and_effect_run_storage_and_fresh_refs` returns 42 through
  `ST.run` twice, `unsafePerformEffect`, `toEffect`, and two independent
  `Effect.Ref` cells.
- Region negatives are the official `Control.Monad.ST` / `ST.Ref` APIs.
- `official_clock_now_runs_through_effect` and
  `official_wasi_and_assert_wrappers_run_through_effect` execute library-module
  `Host.lift` of monotonic `now`, wall-clock records, `randomU64`,
  `randomBytes`, `getStdout`, `WASI.Console.log`, and a held `assert true`.
- Held and failed `Test.Assert` paths write then trap under
  `unsafePerformEffect`.

WASI host leaves stay raw canonical results. `PSRS.Effect.Host.lift` delays a
thunk until `run`. Canonical guest projection keys a WIT import by its checked scheme TypeId.
`live_type_ids` treats Runtime and WIT import schemes as layout roots, including
schemes that appear only on the import.

At the adapter checkpoint above every row was Unverified; the current table
supersedes that snapshot. `Effect.Uncurried` and the remaining NE rows closed
in the [2026-10-16 acceptance](#acceptance-2026-10-16). `checkThrows`,
`Effect.Console` timers and `Effect.Aff` stay explicit
unsupported JavaScript foreigns. The compiler lock revision is
`4c73aab52226e8f5e7b1f17052f1a19d717a472e`.


### Driver runner migration (2026-10-16)

The remaining driver unit-test sources use the official
`Effect.Unsafe.unsafePerformEffect` boundary with explicit imports. Show,
Foldable and Semigroup cases also import the public `Effect` type explicitly.
The source-origin diagnostic test checks the exact `missingName` span.

WASI construction tests now observe execution rather than requiring the old
compiler-owned Effect closure layout. A stored exit request permits exit 0;
interpreting it exits 99 before subsequent logging. A reusable random-byte
action runs its logging continuation twice and returns two eight-byte lengths.
These are focused inertness/repetition/termination observations, not a full
NE-03 or NE-10 conformance claim. The unused textual WAT call-graph helper is
removed. See [handoff §17](delimited-control-migration-handoff-2026-10-12.md#17-official-runner-migration-in-driver-tests-2026-10-16)
for baseline and final commands, results, package identity, and remaining work.

Focused validation requires Wasmtime 49.0.3: the combined driver run includes
151/151 WASI, 12/12 native representation and 8/8 assertion tests. Four missing
`Effect` imports in other families were repaired and pass in their 11-test
rerun; a separate 28-test audit run passes. Together these cover 320 distinct
passing names, with no unrecovered failure; they are not one full-suite run.
Formatting passes. Strict driver Clippy stops on the existing `psrs-kind`
`let_and_return`; allowing that one lint passes. Workspace tests and D-04
scoreboards were not run.


## Native execution and region-boundary evidence (2026-10-16)

Current package content is `fnv1a64-v1:b5ec750799a46cc4`; the compiler HEAD
remains `bca4fea77bda5f79bb44bf6e4660d7bec981d448` with a dirty working tree.
The package revision still names `d6cf8110`; a coherent package commit/pin is
outstanding. These observations do not measure D-04 or close AE-11.

`PSRS.Effect.Native` now retains Bind as a data node and drives an explicit
persistent continuation stack. The former `bind (pure x) k` executed k while
constructing the program and the 20,000-level case exhausted the Wasm call
stack. Both baseline failures are retained in
`/private/tmp/psrs-native-{inert,stack}-before.log`. Public constructor arguments
remain strict; callbacks execute on every interpretation. The deep case covers
20,000 left and right binds, 20,000 Effect iterations with observed host writes,
20,000 further MonadRec iterations, and 20,000 ST iterations.

Shared compiler repairs, with no native-type recognition:

- Core command normalization checks free variables in the entry/runner type,
  rather than rejecting quantifiers used only within a declaration body.
  Scope checking, substitution capture checks and checked matching share that
  forall-aware free-variable operation.
- Universal subsumption can specialize `forall a. a -> a` at
  `forall b. Array b -> Array b`, even with equal binder counts. Invariant
  positions retain their rigid relation. Positive and malformed Core cases
  cover specialization, invalid generalization and inconsistent inputs.
- HIR foreign declarations retain their original source module when imported.
  Ordinary and foreign value signatures use one module-owned annotation
  iterator for kind/reference checking. THIR external schemes retain that
  owner. The ST/STRef public kind is explicitly Region -> Type -> Type.
- Target STArray and STFn constructors remain hidden, matching the original
  foreign-data interfaces. Public partial-array adapters use an explicit
  private representation coercion at the foreign-slot boundary.

Evidence for the Verified rows:

| Rows | Authoritative evidence |
| --- | --- |
| NE-02 | Official Effect/ST share the same Program field and retain their roles; GlobalCells and the official facade regression execute heterogeneous cells and persistent Global references. |
| NE-05 | Modify oracle plus public Ref tests check aliasing, Unit write result, modify/modify' results, one callback, and reentrant write-before-final-publication order. |
| NE-06 | Self oracle records one callback and the same self-reference; Unpublished explicitly records official JS null versus the documented target trap and no successor output. |
| NE-07 | All 279 actual pinned upstream modules compile; native-types compares 11 public cases for region escape, nominal coercion, kind, constructor hiding, host injection and Global conversion. |
| NE-09 | Deep public loops and mixed storage/host requests execute under required Wasmtime; public Run interpretation also exercises the source computation library. |
| NE-11 | Configured PSRS.Command.run executes Effect Unit once, returns zero after normal completion, preserves Int entries and propagates traps. Unsafe execution repeats; a different entry result type is rejected. |
| NE-12 | An arbitrary Query functor interpreted by public Run.interpretRec creates a reusable Effect; output is stored/handler/handler. An unhandled residual row is rejected. |
| NE-13 | Production-source identity audit finds no Effect/ST/native-family recognition. Generic command-runner integration executes an unrelated Job newtype; shared Core/backend tests exercise the same operations. |

`native-types` establishes acceptance/rejection, not complete diagnostic parity:
EscapeAction is official TypesDoNotUnify versus target EscapedSkolem, and
CoerceRegion is official TypesDoNotUnify versus target NoInstanceFound.
All other recorded codes agree. The cases use real upstream modules, not an
opaque replacement facade. Native JS observations compare actual pinned
foreign slots; custom Run integration and deep stack safety are explicitly
separate target requirements.

Commands/results:

- `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib -- tests::native_representation tests::assertions tests::control --test-threads=2`: **92 passed**, no skips; `/private/tmp/psrs-native-sealed-driver-rerun.log`.
- The eight changed library crates (`psrs-hir`, `psrs-resolve`, `psrs-kind`, `psrs-desugar`, `psrs-thir`, `psrs-core`, `psrs-typecheck`, `psrs-backend`) pass **875** library tests; `/private/tmp/psrs-native-sealed-libraries.log`. Backend 510 and kind 40 were repeated after the two lint-only cleanups, in `/private/tmp/psrs-native-closure-tests.log`.
- `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-cli --test command_runner -- --test-threads=2`: **4 passed**; `/private/tmp/psrs-native-generic-command.log`.
- `cargo fmt --all --check` passes. Strict Clippy for those eight crates plus driver, `--lib --tests -- -D warnings`, passes without lint allowances; `/private/tmp/psrs-native-closure-clippy.log`.
- Library `npm test`: **12 passed**; `/private/tmp/psrs-native-sealed-node-tests.log`.
- Library `conformance/native-effect.mjs`: **11/11 accepted** with Wasmtime 49.0.3; `/private/tmp/psrs-native-sealed-public/native.json`. Artifacts, source/compiler/runtime hashes and exact output are retained per case. The Array case covers safe copies, generic writes, growth aliases, one callback, partial access and valid unsafe ownership transfer.
- Library `conformance/native-types.mjs`: **11/11 accepted**; `/private/tmp/psrs-native-type-differential/types.json`.
- Full source audit of all **50** pins: **308** modules, **241** identical, **38** modified, **29** target additions, no missing upstream module; `/private/tmp/psrs-native-sealed-audit/`. This inventory does not itself approve every source difference or close NE-01.

The first sealed driver attempt started before its lock fingerprint was updated
and failed 90 cases at stdlib selection. The corrected complete rerun above
passes. It was not a backend regression. Earlier package fingerprints and
measurements in this document remain historical snapshots.

The rows left open by this sealed snapshot closed in the
[2026-10-16 acceptance](#acceptance-2026-10-16). Workspace tests and scoreboards
have not been run.

## Acceptance (2026-10-16)

Compiler `feat/effect-handlers` at `bca4fea77bda5f79bb44bf6e4660d7bec981d448`,
dirty at measurement time. Independent package `control-library` commit
`4c73aab52226e8f5e7b1f17052f1a19d717a472e` records that tree. Lock revision
matches it. Content fingerprint `fnv1a64-v1:ad19620ada1d2ecf`, matching
`stdlib.lock.json`. Each oracle records `unchanged_during_run: true`. Wasmtime
`49.0.3 (f96eabaa1 2026-10-14)`. `purs` 0.15.16. No workspace suite and no D-04
scoreboard ran.

`upstream-lock.json` again records the pinned `Effect/Uncurried.purs` upstream
hash `0c6db8591d2519d9b18bf2c5ec56f33b6c25ee6f69d3edfbce8482d48771214b` and the
adapted vendored hash `b4004312b629cf047cc96a3674934eec8badd8bdd58935b9110ea9f7a77b7027`.
`Data/Number.purs` vendored hash is the file on disk,
`00fc8a398b00d73ca4d1aef17f31ac0160a041135436df7af3f3acb0f738ba11`.

| Rows | Evidence |
| --- | --- |
| NE-01 | Audit of all 50 pins: 308 modules, 240 identical, 39 modified, 29 platform additions, no absent upstream module, no direct self-recursion (`/private/tmp/psrs-native-effect-audit/inventory.json`). Effect-family lines outside value foreigns are representation declarations: `Effect`, `Ref`, `ST`, `STRef`, `EffectFn1`–`10`, and `STFn1`–`10` foreign-data lines; `ST.Uncurried` and `Effect.Uncurried` module headers; `Data.Array.ST` export `STArray` without the target constructor, its uncurried import, and the `STArray` foreign-data line; `Data.Array.ST.Partial`'s uncurried import. Console, Unsafe, and Assert have empty outside-FFI lists. Public signatures, roles, classes, and instances stay. The other modified modules, including `Data.Function.Uncurried`, are historical and are not re-approved by this row. Three `declaration_removed` foreigns are outside this family: `Data.Array.NonEmpty.Internal.traverse1Impl`, `Data.Array.fromFoldableImpl`, and `Data.Unit.unit`. |
| NE-03 | `PureBind` prints `argument\nstored\ncallback\ncallback\n` and exits 42. `Allocation` returns a fresh second cell (`first` 42, `second` 0) and exits 42. `native-uncurried.mjs` accepts EffectFn1–10, STFn1–10, and `MixedInstances` (21/21). `run*N` delays with `bind (pure unit)`. |
| NE-04 | `Identity` reads and writes `Int`, `String`, and a record through a rank-N `Ref`, keeps the cell after a polymorphic write, discards a write between `before` and `after`, and runs a rank-2 `STRef` inside `ST.run`. Exit 42, stdout `before\nafter\n`. `unsafeCoerce 1 :: Ref Int` is rejected at P8 closure conversion with `unsupported aggregate conversion between normalized runtime shapes` before codegen (`tests::native_representation::identity::a_coerced_non_cell_ref_is_rejected_before_codegen`). |
| NE-08 | `Arrays` accepts safe thaw/freeze copies (`original` `[1, 2]`, `frozen` `[7, 2]`), clone length 2, growth to 3, partial index 10, one `array-callback`, and the documented unsafe ownership transfer. Exit 42. |
| NE-10 | `HostOrder` stores a log action, then runs `log "first"`, `arguments`, `log "second"`, `randomBytes 2`, and `now`. Exit 42, stdout `first\nsecond\n`, empty stderr. Console, clock, random, and arguments therefore run in source order; a stored host action stays inert. Filesystem and environment evidence remains the existing WASI-07 record and was not re-run on this fingerprint. |

Commands:

- `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib -- tests::native_representation::identity tests::native_representation::host::stored_host_work_stays_inert_and_runs_in_source_order -- --test-threads=2`: **3 passed** (11.47s).
- `node /Users/biu/Projects/psrs-stdlib/tools/conformance.mjs audit`: exit 0 (3.42s).
- `conformance/native-types.mjs`: **11/11 accepted**, including `HiddenSTFn`; `/private/tmp/psrs-native-types-oracle/types.json` (33.01s).
- `conformance/native-effect.mjs`: **13/13 accepted** (`PureBind`, `GlobalCells`, `Modify`, `Self`, `Unpublished`, `Deep`, `Allocation`, `Arrays`, `Algebraic`, `Command`, `CommandTrap`, `Identity`, `HostOrder`); `/private/tmp/psrs-native-effect-oracle/native.json` (241.41s).
- `conformance/native-uncurried.mjs`: **21/21 accepted**; `/private/tmp/psrs-native-uncurried-oracle-2/uncurried.json` (281.61s).

The original unsupported-foreign list is historical: `Test.Assert.checkThrows` now uses the native exception channel, and Console timers use the ordinary library implementation in the [timer checklist](console-timers.md). `Effect.Aff` remains unimplemented. Per-IR machine recognizers stay retired. DEC-21's trust boundary stays in place. `unsafeThaw` shares the array reference inside a new cell; JavaScript object identity of that thaw is a documented target difference.

Current synchronous exception support is recorded by [EX-01..EX-18](effect-exception.md).
The former Effect.Exception unsupported entry is superseded by that separate
consumer; its source-error catch does not recover budget exhaustion or traps.
