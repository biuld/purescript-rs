# Library-Owned Effects: Implementation Evidence

Governing design: [library-owned effects and state dependencies](../../design/backend/fp/library-owned-effects.md).

Status: **Partial**. The state vocabulary, checked primitive type contracts and
generic dependency graph verifier exist. Core verification now produces checked
flow for State-to-Step lambda bodies. CC now retains logical State values and
derives checked flow from actual single-root function instructions. P9 now
automatically projects State/Step calls through straight-line bodies and nested
binary and multiway choices and publishes MIR dependency evidence. Public
recursive Effect loops execute through these checked calls and choices; arbitrary
CFG backedges, raw-State choice results and general scoped transport remain
incomplete.
Explicit world/region runners now publish closed execution evidence and run
source-selected Int entries.
Ordinary source runtime bindings now reach checked CC with their logical
State-to-Step signatures and actual invocation chains. The CC/raw storage
contract and application-arena raw import signatures are checked. Canonical
payload fill/read/write calls now publish actual MIR calls and recovery with
immutable dependency/CFG correspondence. Source alias-write projection reaches
optimized MIR and Core Wasm emission. Non-returning calls now publish checked
terminating paths anchored to their source assignment and raw return contract.
Default CLI/GC-provider composition now executes source alias writes and
ordinary newtype bind wrappers. General provider cycles remain incomplete.
The Core producer's quantifier evidence for quantified record fields,
projections and array elements is repaired, so the candidate ST reference
program compiles and executes to 42 instead of being rejected at P7.
Effect and ST still use the previous locked
library and compiler mechanism. Runtime GC storage primitives execute independently and through the focused
source fixtures; this record does not assert official Effect runtime acceptance.

## Starting point

The 2026-10-09 implementation starts on `stdlib/vendor-core-libraries`, compiler
`9da4fd18535476c87c87373400a209c245fa22b1`, with the selected design documents
already uncommitted. The library lock remains at
`1e01310daa7e56de5ed379029d6a24dcf39e879c`.

The unchanged full-import fixture was diagnosed with:

```sh
./target/debug/psrs diagnose \
  /private/tmp/psrs-stdlib-compile-20261009-8rsdzxc2/Main.purs \
  --out /private/tmp/psrs-state-before.json --timeout 240
```

Outcome: P5 NoInstanceFound, first message `Semigroup (Effect _T396)`,
13,298 ms, no timeout or crash. This is compile evidence only. The snapshot
and replay bundle live outside the repository. No roadmap total is changed.

The same command with `--out /private/tmp/psrs-state-after-foundation.json`
reported the same first blocker in 13,140 ms after the foundation changes.
There is no stdlib compile-acceptance improvement at this checkpoint.

## Requirement-to-evidence map

| Requirement | Status | Evidence and limit |
| --- | --- | --- |
| Primitive identities, kinds and nominal regions | Verified at source layer | HIR `target_state_is_opaque_and_its_region_is_nominal`; driver `the_state_region_cannot_be_safely_coerced` and `state_accepts_the_official_st_region_kind`. State is kind-polymorphic; official `Global :: Region` and a Region-kind rank-N runner pass checking. |
| Ordinary library state newtype/combinators | Verified in focused source execution | Ordinary Task pure/bind definitions retain two callback transitions and execute through an ordinary library runner, returning 46 under mandatory Wasmtime. Actual development-package Effect Bind and public GlobalST reference execution pass; complete API acceptance remains open. |
| Complete newtype callable field contracts | Verified for template, rank-N and focused ADT field transport | Boundary tests retain State regions, whole Step results, nested field applications and returned functions. Mandatory Wasmtime tests execute Reader and structured-domain/result newtypes through a rank-N runner and a generic Holder carrier. Storage parameters use the declared erased field convention. Straight-line State projection is now implemented; scoped transport remains incomplete. |
| Binding-only state primitive exposure | Verified at resolution layer | Driver `unsafe_state_operations_require_an_explicit_binding`; registry schemes remain available for explicit bindings. |
| Primitive type contracts before erasure | Verified at Core/linking layer | Core world/region runner contract and rank-N escape tests; driver invalid-unused-binding rejection. Source origins and complete candidate validation use the existing primitive linker. |
| Generic dependency chain validation | Verified at Core body layer | Core verification derives State-to-Step lambda graphs from actual invocations, including branches, aliases, record patterns and traps. Source correspondence checks invocation coverage/order, argument provenance, region and branch placement. Utility tests also cover loop backedges; CC/MIR loop projection is still missing. |
| Checked zero-width state projection | Partial, with straight-line, binary and multiway P9 lowering | State parameters/declarations are removed only with checked CC provenance. Known Step values become payloads without record allocation; closure signatures and calls agree. Source callback and ordinary Task pure/bind cases reach encoded Wasm. A CC-to-component harness executes two state calls and returns 42. Canonical host calls now have checked adapter-subgraph projection; general CFG and scoped/erased transport remain required. |
| Runtime GC storage and termination | Verified independently | Runtime provider exports fill/read/write/trap with no state bits or linear memory. Mandatory Wasmtime tests execute cross-module calls, alias writes, reference/value identity, repeated reads, zero lengths, null values and trapping sizes/indices/termination; an incompatible immutable-array import is rejected before execution. |
| Ordinary runtime binding contracts | Verified at source and logical CC boundaries | Provider/export identity remains an ordinary foreign binding. Runtime metadata owns source operand/payload relations and physical ABI; driver tests reject malformed unused declarations, unknown exports and retired storage intrinsic names. CC checks retained State/Step slots, operand roles and canonical array storage. Canonical payload fill/read/write calls publish actual MIR calls with checked source correspondence. Non-returning calls refine their source path to checked Trap termination; Monomorphic scalar, record, callback and nested-array payload recovery now executes through ordinary CC conversions. |
| State-aware host and mutable storage execution | Partial | Checked runtime provider calls execute source storage alias-write fixtures. Canonical host adapter plans retain source anchors, physical calls, result adaptation and internal control evidence. Mandatory Wasmtime source-newtype cases execute resource get/drop, widened scalar parameters, record results, list results and variant results. Cyclic storage, general source CFG and complete host/library conformance remain incomplete. |
| Core/CC/MIR dependency production and preservation | Partial | Immutable Core bodies supply checked flow; Core optimizers recheck each pass. CC verifies actual single-root instruction flow. P9 publishes straight-line, binary and multiway MIR correspondence against immutable CC; MIR optimization and final lowering recheck it. Total Core-to-CC correspondence, general CFG/scoped/loop projection and dependency-aware inlining remain required. |
| Final encoding erasure | Partial for checked straight-line, binary and multiway source bodies | Source callbacks, Task pure/bind and checked canonical host adapter subgraphs encode after MIR verification. Public recursive loops compose through checked source calls; arbitrary CFG backedges and general scoped transport remain incomplete. Explicit source world/region runners execute through the normal pipeline. |
| Effect/ST library and command runner migration | Partial; development package wrappers migrated | Generic resolved-runner Core normalization, focused source execution and production manifest selection exist. The independent package has the unpushed State-newtype migration at `7852625` plus an uncommitted migration of 92 target WASI foreign wrappers. Public host/storage fixtures, ten pinned ST JS comparisons and ten public Effect-loop JS comparisons pass; complete API/host conformance and legacy deletion remain. The compiler library lock is unchanged. |
| Integrated source and runtime conformance | Partial | Library-owned pinned ST and public Effect-loop JS comparisons agree with mandatory Wasmtime. Full official composition/API/host acceptance remains required. |

## Continuation order

1. Extend checked body evidence to general scoped state transport beyond the
   implemented explicit world/region boundaries; retain immutable source
   correspondence during call projection.
2. Carry explicit dependencies through CC and MIR, including higher-order
   calls, branches, loops, captures and signature adapters. Connect the graph
   verifier to actual checked operation identities.
3. Implement general zero-width projection and checked GC runtime provider
   linking/calls, then state-aware host bindings. Runtime owns storage execution;
   stdlib owns combinators. Preserve checked types before erasure and in-place
   alias identity. Do not route GC storage through the scalar artifact ABI.
4. Migrate Effect and ST together in the independent package, preserving official
   public source contracts and recording the selected newtype adaptation. The
   quantifier-evidence blocker that previously stopped this step is repaired.
5. Delete the legacy Effect mechanism, validate optimizer/final erasure and
   execute complete conformance evidence.
6. Update the normal lock only after a coherent package is validated. Keep
   partial development packages explicit and outside normal locked consumption.

Do not describe utility-level tests as integrated state ordering, change the
normative design to match this partial checkpoint, or close Effect's migration
on import-only compilation.

## Validation runs

On 2026-10-09:

- HIR primitive registry tests: 9 passed.
- Focused Core state tests: 12 passed before the additional integrated Core
  verification test; the full Core library subsequently passed all 104 tests.
- Driver state source/binding tests: 4 passed.
- Official differential command
  `PURESCRIPT_REPO=/Users/biu/Projects/purescript cargo test -p psrs-driver --test upstream`:
  19 passed, no skips.
- `cargo fmt --all --check`, `git diff --check` and
  `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `sh crates/psrs-runtime/tools/check-reproducible.sh`: the existing numeric
  and allocator artifacts still reproduce byte-for-byte; neither pin changed.
- `cargo test --workspace`: initial run stopped on source-layout violations;
  the new dependency module now uses `dependency/mod.rs`, and an empty legacy
  `program/effects/` directory was removed. The repeated run passed 1,749 unit
  and integration tests with 5 ignored tests, then stopped at rustdoc E0463
  (`can't find crate for psrs_linker`). A standalone backend doc-test rebuild
  passed; the subsequent `cargo test --workspace --doc` passed for all packages.
  The single full workspace command did not exit successfully, so its result
  is reported separately from the successful doc-test retry.
- `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::effects`:
  50 passed, no skips. These exercise the existing mechanism and cannot verify
  the new state representation.

Logs are in `/private/tmp/psrs-state-{workspace-tests,clippy,upstream,legacy-effects}.log`.
The doc-test retry log is `/private/tmp/psrs-state-workspace-docs.log`.
The new runtime provider is independently executable; state target lowering and
integrated Effect runtime execution remain unimplemented.

### Runtime ownership correction

The same checkout now has runtime-owned GC storage/termination ABI contracts
under `psrs-runtime/src/abi/storage.rs` and executable core-module encoding under
`psrs-runtime/src/storage/`. The `gc-storage` host feature encodes a separate
module, rather than compiling a Rust heap or adding a second runtime crate.
State target selection uses these contracts and keeps execution-boundary
lowering distinct. Neither path emits code without checked dependency and
signature projection. The scalar runtime catalog does not advertise this
provider: compiler-owned GC reference linking and composition remain required.

Validation on 2026-10-09 with Wasmtime 49.0.2:

- `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace`: passed, including doc tests;
  1,753 tests passed and 5 were ignored under their existing configuration.
  This run includes four runtime storage tests, not the subsequently added
  incompatible-reference import test.
- `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-runtime --features gc-storage storage -- --nocapture`:
  all five runtime tests passed, including that additional rejection case.
- `cargo test -p psrs-backend target_intrinsics --lib`: 2 passed.
- `cargo fmt --all --check`, `git diff --check` and
  `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- The identical full-import fixture was diagnosed again with
  `cargo run -- diagnose /private/tmp/psrs-stdlib-compile-20261009-8rsdzxc2/Main.purs --out /private/tmp/psrs-state-after-runtime.json --timeout 240`:
  P5 NoInstanceFound, `Semigroup (Effect _T396)`, 13,389 ms, no timeout or crash.
  The locked stdlib has not changed; this runtime correction does not repair
  its source-level first blocker or establish integrated Effect execution.
- `diagnose --compare` against `psrs-state-after-foundation.json` reports a
  compatible cohort, identical inputs and `unchanged_failure`; no difference
  in the observed pass prefix. Canonical artifact-content comparison remains
  unavailable, so this is a compile-outcome comparison only.

Logs: `/private/tmp/psrs-runtime-effects-{workspace,storage,clippy,diagnose,compare,reproducible}.log`.

### Ordinary runtime binding migration

Storage no longer has one HIR intrinsic per operation. The experimental
`StateArrayRead/Fill/Write` entries were removed, with stable slots 89–91
reserved. `ExternalKind::Runtime` retains an explicit provider/export and its
checked foreign signature; the runtime ABI owns source operand/payload relations.
Backend runtime linking resolves all declarations before projection, including
unused declarations, without relying on WIT names or a library action constructor.

The official `Global :: Region` declaration exposed a separate kind defect:
`State :: Type -> Type` rejected the minimal state newtype at P5 with
`KindsDoNotUnify`. `State :: forall k. k -> Type` fixes that shared primitive
contract while retaining a nominal region role. The source test now checks a
Region-kind action, a Global storage binding and a rank-N Region-kind runner.
No official library declaration was changed to make this pass.

The focused driver state suite passes all 9 tests. This is source and binding
contract evidence; valid storage bindings still reject at `P8 runtime projection`
because state dependencies and the GC call ABI are not yet carried through CC/MIR.
The active Effect objective remains incomplete.

Validation for this migration on 2026-10-09:

- `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace`: passed, including doc tests;
  1,759 passed, 5 ignored. All five independent runtime storage tests executed.
- `cargo fmt --all --check`, `git diff --check` and
  `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `PURESCRIPT_REPO=/Users/biu/Projects/purescript cargo test -p psrs-driver --test upstream`:
  19 passed, no skips.
- Runtime reproducibility check: numeric and allocator artifacts remain
  byte-for-byte identical to their reviewed pins.
- The identical full-import fixture still reports P5 NoInstanceFound,
  `Semigroup (Effect _T396)`, in 13,636 ms with no timeout/crash.
  `diagnose --compare` against `psrs-state-after-runtime.json` confirms
  identical inputs, a compatible cohort and `unchanged_failure`.

Logs: `/private/tmp/psrs-runtime-bindings-{workspace,clippy,upstream,reproducible,diagnose,compare}.log`.

### Complete callable storage templates

The boundary registry now retains the complete source field, constructor
variables, final callable storage owner/field, parameter types and result type.
It no longer substitutes concrete constructor arguments into canonical newtype
parameter slots or limits domains to a closed type/bare argument. Constructor
transport checks the complete storage result as well as parameters. Common
call-boundary extraction rejects cyclic/dangling spines before layout and keeps
a fixed-arity closure's returned function separate from its own arguments.

The rank-N Reader source reproducer previously failed at P8 with
`constructor transport requires a callable segment adapter`. Both it and the
array-domain/record-result Batch reproducer now execute under mandatory Wasmtime
with exit value 42. Four boundary tests also check an ordinary state newtype's
State/Step template, nested owner evidence, nullary closure results and malformed
call boundaries. These are required foundations, not proof of state projection.

At that checkpoint, the separate `Holder f a` ADT field reproducer failed at P8 with
`abstract constructor transport has no checked constructor binding` after these
changes. It was retained as the ignored runtime acceptance test
`higher_kinded_adt_field_preserves_callable_newtype`; the later constructor-field
evidence section records its repair. The active
Effect objective includes fixing that scope/evidence path, producing actual
state dependencies through CC/MIR, projecting signatures, composing GC providers,
migrating Effect/ST and replacing the legacy entry/synthesis mechanism.

Focused evidence: 11 driver state tests passed, 1 explicit remaining acceptance
test ignored; 4 boundary tests passed. Executing the ignored case explicitly
reproduces the P8 failure, rather than a runtime skip. Logs:
`/private/tmp/psrs-callable-{newtype-before,state-focused,holder-gap}.log`.

The subsequent `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace` completed
successfully, including doc tests: 1,765 passed and 6 ignored. One ignored test
is the explicit higher-kinded ADT acceptance gap above; the other five retain
their existing suite configuration. `cargo fmt --all --check`,
`git diff --check` and
`cargo clippy --workspace --all-targets -- -D warnings` also passed.
Logs: `/private/tmp/psrs-callable-template-{workspace,clippy}.log`.
These checks establish no regression in the current supported paths; they do
not establish integrated State projection or Effect migration acceptance.

### Source Core dependency production

`psrs_core::state::flow` now derives evidence from immutable State-to-Step lambda
bodies. Each input dependency belongs to the actual State binder; every observable
transition addresses its actual source invocation. Ordinary aliases, structural
record fields/patterns, constructor fields and branch joins retain producer
provenance. Unknown state callables remain conservatively observable; a small
body proof permits known inert state functions to pass their input through.
Trap paths have no normal successor. Pure joins preserve previously evaluated
state aliases, including a record's state field evaluated before its payload.

The source certificate retains its arena, lambda, checked region and invocation
references. Its verifier checks graph validity, complete invocation coverage/order,
operand/payload contracts and the source-derived graph's dependency provenance
and branch placement. A graph that is valid in isolation cannot omit/reorder
calls, move a call to another branch or be reused with a different source arena.
Core verification runs this check after ordinary typing and scope checks. Existing
Core optimization rechecks it before optimization and after each pass.

Source tests check two real runtime-bound calls, a conditional's selected
successor and ordinary Task newtype pure/bind definitions. Well-typed stale
returns and replayed predecessors reject at `P7 Core verification`. The unchanged
negative reproducer at `/private/tmp/psrs-state-flow-reproducer/Main.purs` previously
reached `P8 runtime projection` in 227 ms; its final post-change trace rejects
the discarded successor at P7 in 217 ms. `diagnose --compare` confirms identical
inputs and a compatible cohort, with a changed observed stage. This is stronger
rejection evidence, not a positive compile or runtime acceptance improvement.

The focused Core state suite passes 32 tests (19 body-flow tests and the existing
13 contract/graph tests). The driver state suite passes 15 tests with the existing
ADT acceptance gap ignored. These do not establish scoped raw-state captures,
runner root/discharge evidence, complete higher-kinded transport, CC/MIR operation
correspondence, loop lowering or zero-width physical projection. The runtime
bindings still reject missing projection; the old Effect and ST package remains.
Logs: `/private/tmp/psrs-state-flow-{core,focused,before,after,compare}.log`.

Final validation on 2026-10-09:

- `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace`: passed, including doc tests;
  1,788 passed and 6 ignored. The independent storage provider tests execute;
  the higher-kinded ADT acceptance gap remains ignored and unverified.
- `cargo fmt --all --check`, `git diff --check` and
  `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `PURESCRIPT_REPO=/Users/biu/Projects/purescript cargo test -p psrs-driver --test upstream`:
  19 passed, no skips. The State dependency contract is a target extension;
  this run checks existing official semantics for regressions.
- Runtime numeric and allocator artifacts reproduce byte-for-byte.
- The identical full-import fixture still rejects at P5 NoInstanceFound,
  `Semigroup (Effect _T396)`, in 13,146 ms with no timeout or crash.
  Comparison against `psrs-state-after-runtime-bindings.json` confirms identical
  inputs, a compatible cohort and `unchanged_failure`. An initial comparison
  named a nonexistent intermediate snapshot; only the retry against that
  available baseline supplies comparison evidence.

Logs: `/private/tmp/psrs-state-flow-{workspace,clippy,upstream,reproducible,stdlib,stdlib-compare}.log`.
The active Effect migration remains incomplete. No package lock, official source
or roadmap measurement changed.

### Scoped constructor-field instantiation

The unchanged Holder reproducer rejected at P8 with `abstract constructor
transport has no checked constructor binding`. The first missing evidence was
on construction, before pattern recovery. Aggregate field storage and variant
field recovery now retain a checked template-to-use instantiation with the
owning constructor's parameters. The shared Core relation owns that proof;
ordinary Core verification already checks all fields against the parent's
consistent type arguments. Physical storage continues to use the declaration's
canonical field convention.

The Holder Reader test now executes with exit value 42 and is no longer ignored.
A second Holder test executes an array-domain, record-result Batch with exit
value 42, checking structured input and payload transport. The driver state
suite passes 17 tests with no skips under `PSRS_REQUIRE_WASMTIME=1`.
Logs: `/private/tmp/psrs-state-holder-{before,after,focused}.log`.
This repairs ordinary carrier transport; it does not implement State projection,
CC/MIR dependency correspondence or integrated runtime provider composition.

Validation on 2026-10-09: mandatory-Wasmtime workspace tests passed, including
doc tests, with 1,790 passed and 5 existing ignored tests. Formatting, strict
workspace clippy and `git diff --check` passed. The explicit official frontend
differential suite passed all 19 tests without skips.
Logs: `/private/tmp/psrs-state-holder-{workspace,clippy,upstream}.log`.

The rebuilt CLI diagnosed the identical full-import fixture in 13,096 ms:
P5 NoInstanceFound, `Semigroup (Effect _T396)`, with no timeout or crash.
Comparison against `psrs-state-after-source-flow.json` confirms identical inputs,
a compatible cohort and `unchanged_failure`. Logs:
`/private/tmp/psrs-state-holder-{stdlib,stdlib-compare}.log`.
The independent stdlib worktree and normal package lock remain unchanged.

### Shared call boundaries and zero-width signature planning

The fixed-arity closure reproducer failed with `state function has no state
invocation parameter`, although the equivalent arrow signature was accepted.
Core now owns `call_parts`; State signature checking and CC's ordinary call
boundary extraction consume that same operation. Fixed-arity closure results
and quantified arrow results retain their separate calling boundaries. A
checked Core closure invocation now produces its actual successor dependency,
not an assumed pass-through. Focused Core state evidence: 34 tests passed.
Logs: `/private/tmp/psrs-state-projection-{before,core}.log`.

CC function signature planning consumes a checked CallProjection that retains
the immutable source arena and State signature. State-final calls project to
ordinary physical arguments and the closed Step's payload. Five backend tests
check nullary projection without State/Step storage, ordinary arguments with
a returned function payload, a separately returned state action, nominal region
mismatch and unsupported nonfinal State. The production function-signature
entry explicitly refuses to publish a State call signature without CC
invocation dependency lowering; a storage plan is not execution evidence.
Log: `/private/tmp/psrs-state-projection-backend.log`.

The first workspace attempt stopped at the source-layout check: the new module
roots initially shared names with child directories. Both roots were moved to
their directories' `mod.rs`; the focused source-layout check then passed.
Logs: `/private/tmp/psrs-state-projection-{workspace,source-layout}.log`.
CC record/capture/invocation projection, MIR dependencies, runtime composition
and the Effect/ST package migration remain required.

Final validation on 2026-10-09: mandatory-Wasmtime workspace tests passed,
including doc tests, with 1,797 passed and 5 existing ignored tests. Formatting,
strict workspace clippy and `git diff --check` passed. The explicit official
frontend differential suite passed all 19 tests without skips.
Logs: `/private/tmp/psrs-state-projection-{workspace-final,clippy,upstream}.log`.

The rebuilt CLI diagnosed the same full-import fixture in 13,659 ms with no
timeout or crash. Its first blocker remains P5 NoInstanceFound,
`Semigroup (Effect _T396)`. Comparison against `psrs-state-after-holder.json`
confirms identical inputs, a compatible cohort and `unchanged_failure`.
Logs: `/private/tmp/psrs-state-projection-{stdlib,stdlib-compare}.log`.
No official library source, package lock or roadmap measurement changed.

### Logical State and actual CC instruction flow

On 2026-10-10, CC retains State as a logical zero-payload shape in parameters,
products and instruction operands. The common primitive shape owner prevents
opaque-handle or erased-reference fallback at declaration and expression uses.
Payload boxing/recovery, array storage and physical WIT/MIR layout reject an
undischarged State. Physical signature planning remains separate from CC's
logical State parameter and Step result.

CC module verification now derives single-root dependency graphs from actual
direct/indirect calls, product creation/projection, checked identity/ProductMap
conversion, If joins and traps. Operation evidence borrows the actual CC
assignment and is tied to its immutable module. Re-derivation rejects edited
operation coverage or control flow even when the edited graph is valid alone.
Nominal source-region agreement remains a checked-Core producer guarantee;
this CC verifier checks local instruction provenance, not source kinds/roles.

State pass-through summaries come from checked CC bodies via a least fixed point,
independent of declaration order. Unknown and recursive calls stay observable.
Stale invocation inputs and stale returns reject. Pure joins retain prior
aliases; observable joins require the selected successor. A trapping branch
has no normal successor. Captured State without a checked root, multiple roots,
multiway State switches and general erased State transport remain unsupported.

Focused evidence: 8 backend state tests, 5 signature-planning tests and the
physical-layout rejection test passed. Source callbacks and ordinary Task
pure/bind definitions lower into CC; the callback is an actual indirect call,
and bind retains two ordered operations. These are Core-to-CC acceptance tests,
not Wasm execution evidence. Logs:
`/private/tmp/psrs-state-cc-{focused,projection,mir-boundary,source,library,driver}.log`.

The final unchanged callback fixture at
`/private/tmp/psrs-state-cc-reproducer/Main.purs` rejected at P8 in 240 ms before
the change. After rebuilding, CC lowering completes and the fixture reaches
P9's explicit `UnprojectedState` rejection in 250 ms. The trace comparison
confirms identical inputs, a compatible cohort and `backend.cc.lower` changing
from rejected to completed. The earlier fixture omitted the callback's use and
was replaced before recording the final comparable baseline. Logs:
`/private/tmp/psrs-state-cc-{before,after,compare}.log`.

Total Core-to-CC invocation correspondence remains required beyond the focused
source cases. MIR instruction/CFG correspondence and physical parameter/Step
projection, runner boundaries, scoped transport, runtime provider composition,
Effect/ST migration and integrated conformance remain incomplete.

An additional source case exposed a false stale-return rejection when an ordinary
state function constructed an array and returned its input State unchanged.
Core accepted the pure construction, while CC's summary treated it as a new
dependency. CC now preserves State pass-through for array/data construction;
this summary does not authorize call CSE or suppress allocation effects.
The existing MIR effect classifier still marks calls and allocations as
potentially trapping and memory-affecting. The regression retains the action's
actual array read and verifies its incoming alias, rather than merely finding
an unrelated pure function. The driver state suite passes 20 tests under
mandatory Wasmtime. Logs:
`/private/tmp/psrs-state-cc-array-{before,after}.log` and
`/private/tmp/psrs-state-cc-driver-final.log`.

## Shared CC call projection and MIR entry (2026-10-10)

`cc::state::StateCallProjection` now owns the logical State-to-Step slot
boundary used by actual CC invocation checking. It identifies the final State
parameter and the successor/payload fields without assuming product field order.
The physical signature operation projects ordinary arguments and the payload;
it leaves the original signature and representation table unchanged. Malformed
parameter placement, duplicate states, nullable Step references and products
without exactly one successor and one payload reject. Ordinary call signatures
retain their convention. This plans a call boundary; it does not erase any body
instruction or establish invocation/CFG correspondence by itself.

The common MIR lowering entry re-derives actual CC dependency bodies before
physical layout planning. Ordinary CC typing remains the P8 producer's contract;
this check does not claim to validate every source kind, region or WIT mapping.
A bound call fixture that reuses a stale predecessor now fails dependency
verification before the physical `UnprojectedState` guard. Existing P9 fixture
conventions remain covered by the backend tests.

Physical instruction projection, dependency-preserving MIR optimization and
final encoding are still incomplete. The signature operation is available for
that lowering but has not yet been wired into physical layout emission. No
State parameter or Step storage has been erased by this checkpoint.

Validation is scoped to the affected backend and driver state tests, following
the user's request to avoid repeating workspace-wide checks each iteration.
The backend library suite passes 431 tests with mandatory Wasmtime, including
four projection cases and the MIR-entry regression. Log:
`/private/tmp/psrs-state-projection-backend.log`.
The driver state suite passes 20 tests under mandatory Wasmtime
(`/private/tmp/psrs-state-projection-driver.log`). A subsequent missing-operand
and missing-parameter-metadata regression ensures the direct dependency
recheck rejects malformed inputs instead of indexing absent values. All 14
CC state/projection tests pass after that change
(`/private/tmp/psrs-state-shared-projection-final.log`). Formatting, diff checks
and backend-only strict Clippy pass. Workspace tests and workspace Clippy were
not repeated for this checkpoint.

## MIR-owned dependency evidence (2026-10-10)

`mir::Function.state` now retains a logical `DependencyFlow` alongside concrete
SSA values and instructions. Its private immutable CC source and graph are
checked against actual MIR calls. The straight-line correspondence checker
requires complete invocation coverage, ordered destination identities, direct
callee identity, invocation kind and the normal/trapping successor. Checking
all calls also detects inserted invocations and protects State pass-through
calls rather than relying only on observable graph transitions. It rejects
reintroduced physical slots using logical State identities and requires the
remaining ordinary parameter identities to match.

The common MIR verifier consumes this evidence. The optimizer rechecks it after
each pass; final Wasm lowering already invokes that verifier. Inlining and
tail-call marking conservatively retain bodies carrying the evidence until
their dependency remapping is implemented. A test confirms that an invocation
with unused physical payload remains present after optimization.

This is representation and verifier evidence from hand-built, checked CC/MIR
fixtures. The source pipeline does not yet publish the flow automatically:
ordinary lowering still initializes the field to `None`, and undischarged State
continues to fail physical layout planning. Mandatory publication, automatic
zero-width body lowering, multi-block/loop correspondence, scoped and erased
transport, runtime composition and source-to-Wasm execution remain required.
The design has not been narrowed to straight-line functions.

The affected backend library suite passed 437 tests under mandatory Wasmtime
before the final logical-State-as-zero regression
(`/private/tmp/psrs-mir-state-backend.log`). The focused MIR evidence tests cover
valid projection, missing/reordered/extra invocations, changed producers and
returns, optimization retention, physical State-slot rejection and a trapping
body's actual unreachable instruction. All 7 focused cases pass after the final
changes (`/private/tmp/psrs-mir-state-flow-final.log`). The driver state suite
also passes 20 tests under mandatory Wasmtime
(`/private/tmp/psrs-mir-state-driver.log`). Formatting, diff checks and strict
backend Clippy pass. Workspace tests and workspace Clippy were not repeated.

## Automatic straight-line State/Step lowering (2026-10-10)

P9 now consumes immutable logical CC through a private physical instruction
view. The shared `StateCallProjection` owns the Step successor/payload slots;
the existing reachability analysis selects the callable signatures to project.
Unused provisional signatures are not treated as checked storage requirements.
State parameters and declarations disappear from physical signatures and values.
Known Step construction and payload projection become ordinary payload copies,
without a Step struct allocation. Logical identities remain reserved so lowering
temporaries cannot accidentally reuse a removed State identity.

After physical instruction lowering, P9 automatically publishes
`DependencyFlow` for each projected body before tail-call marking. The MIR
verifier, optimizer and final Wasm lowering check it against immutable CC and
actual calls. The source callback fixture now reaches MIR optimization and
Wasm encoding with one retained transition and Int payload signatures.
Ordinary Task newtype pure/bind definitions also encode, retaining bind's two
callback transitions. The array-payload regression reaches MIR and retains its
array read. These source tests do not invoke a new source-level world runner.

A separate CC-to-component harness executes ordinary State functions under
mandatory Wasmtime and returns 42. Both physical functions are nullary with Int
results; two calls survive optimization and no Step struct is allocated. This
is P9 execution evidence with a selected test entry, not integrated Effect/ST
or source-runner acceptance. Log:
`/private/tmp/psrs-state-projection-runtime.log`.

The unchanged CLI callback reproducer moves from P9 `UnprojectedState` in
252 ms to a passing compilation in 278 ms. The comparison confirms identical
inputs, a compatible cohort and `backend.mir.lower` changing from rejected to
completed. It does not provide a canonical artifact-content comparison. Logs:
`/private/tmp/psrs-state-mir-projected{,-compare}.log`.

Validation: 439 backend library tests passed before the additional execution
harness; all 8 focused MIR state tests then passed with mandatory Wasmtime.
The final driver state suite passes 21 tests, including source-to-MIR/Wasm
callback and Task cases. Logs:
`/private/tmp/psrs-mir-projection-{backend,focused-final,driver-final}.log`.
Formatting, diff checks and strict backend/driver Clippy pass. Workspace-wide
tests, upstream differentials and the full stdlib import were not repeated for
this backend projection change.

General CFG and loop projection, scoped/erased raw-state transport, invocation
expansion for runtime/adapter calls, source world/region runners, checked host
ABI projection, GC provider composition, Effect/ST package migration and
integrated conformance remain required. Unsupported state-aware host calls and
multi-block bodies reject rather than erasing their dependency. The normative
design and overall completion scope remain unchanged.

## Binary choices and explicit trap exits (2026-10-10)

P9 recursively projects binary and nested choices to payload-only MIR block
parameters. MIR dependency correspondence checks each block's predicate,
successors and complete call inventory against immutable source CC. Dependency
and physical CFG block numbers need not coincide: the source instruction tree
anchors the physical allocation order. Negative tests reject swapped edges,
redirected jumps and calls moved between arms. Raw-State choice results,
multiway choices and loops still reject until checked projection exists.

An explicit MIR `Trap` terminator represents a path without a normal successor
or return operand. Common CFG analysis, verification, optimization and both
structured and dispatcher Wasm encoding consume that contract. A trapping
State arm retains the source trap and cannot acquire a fabricated jump to the
payload join. CFG pruning and branch simplification conservatively retain
State-bearing functions; multi-block copy forwarding also waits for evidence
transfer. Dead pure instruction elimination still runs.

The CC-to-component harness executes both ordinary branches (42 and 43), an
unselected trap arm (43), and a selected trap arm (Wasm `unreachable`) under
mandatory Wasmtime after optimization. This remains a CC entry harness rather
than source world-runner or integrated Effect/ST acceptance. Source callback
choices independently reach checked MIR and encoded Wasm.

Validation for this change is scoped to affected MIR, Wasm structuring and
driver State tests: 202 MIR tests, 13 Wasm structuring tests, and 22 driver
State tests pass with mandatory Wasmtime. Strict backend/driver all-target
Clippy, formatting and diff checks pass. Logs:
`/private/tmp/psrs-state-trap-{mir,structure,driver}-final.log` and
`/private/tmp/psrs-state-trap-clippy-final.log`. Workspace-wide tests, upstream
differentials and full stdlib imports were not repeated for this change.
The source runner, runtime host/storage composition, library migration and
integrated conformance obligations remain open.

## Source world/region runners and library composition (2026-10-10)

P8 now lowers explicit `runWorld`/`runRegion` bindings to a logical
`StateExecution` instruction after rechecking Core's primitive contract. CC
validates its closure signature and payload shape and derives a closed
root-to-invocation-to-discharge graph tied to the actual instruction. Ambient
functions without State parameters can carry an empty region graph and these
closed executions; nested scopes keep separate graph namespaces. Core owns
nominal region introduction and escape checking, which CC trusts after erasure.

P9 projects each boundary to one physical closure call with no State argument
and retains its closed graph in MIR evidence. Correspondence rejects removed
calls and changed action operands. Each dynamic call supplies its own logical
root, including repeated calls of the same action; no integer initial State,
State local, or Step allocation is introduced by the boundary.

Generic Step product maps now retain their payload boxing/unboxing while
requiring an identity State-field conversion. Atomic callable adapters lower
to their checked factory call using the source destination, and correspondence
checks the factory's stable producer and source operand in execution order.
Other invocation expansions and general scoped/erased transport remain open.

The same source-world fixture previously failed in P8 with a closure-capture
diagnostic masking the unsupported execution boundary
(`/private/tmp/psrs-runners-baseline.log`). The final focused fixtures now run
through the normal source compiler and component execution helper, with source
entry selection: world 42, rank-N region 43, nested runners 44, repeated action
82, and ordinary Task newtype pure/bind through a library runner 46. A selected
runner propagates the body's integer-division trap. A callable Step payload
executes and returns 47. This last case initially failed at P10 because copy
forwarding changed its call operand without transferring correspondence
(`/private/tmp/psrs-runners-callable-payload-before.log`). Copy forwarding now
conservatively retains State-bearing bodies until alias evidence transfer
exists; dead pure instruction elimination remains enabled. These are focused
source execution cases; no official Effect/ST package or conformance result
is implied.

Validation: 31 driver State tests pass with mandatory Wasmtime
(`/private/tmp/psrs-runners-driver-final.log`), including malformed binding and
nominal-region rejection cases. 139 CC tests and 202 MIR tests pass
(`/private/tmp/psrs-runners-{cc,mir}.log`). Strict backend/driver all-target
Clippy passes (`/private/tmp/psrs-runners-clippy.log`); formatting, diff checks
and the maintained Rust file-length check pass. Full workspace, upstream
differential and stdlib import runs are omitted under the requested focused
validation scope.

Runtime GC host call projection/provider composition, generic configured
command-runner loading and entry normalization, official Effect/ST package
migration and integrated library-owned conformance remain required. Existing
trusted Effect/StateToken paths and the locked package are unchanged.

## Runtime GC type-group compatibility (2026-10-10)

The runtime storage provider declares its mutable nullable-eqref array in an
independent recursion group. P9 previously placed every application definition
in one group, changing that array's canonical identity when unrelated closure,
box or signature types were present. A consumer with the merged group is valid
Wasm but Wasmtime rejects its storage import as incompatible.

The MIR layout planner now partitions complete definitions by reference and
supertype dependencies using strongly connected components. Dependency groups
precede users; mutually dependent definitions remain together. All definition
references, representation and signature indices, product-field keys, closure,
capture, scalar-box and string handles are remapped as one planner operation.
The iterative graph walk handles a 10,000-type dependency chain without Rust
call-stack growth and rejects dangling edges before remapping. MIR verification
also rejects empty groups and references crossing into later groups, while
accepting mutual forward references within a declared group.

A cross-module fixture uses actual `PlannedLayout` definitions and the compiler's
Wasm encoder, then loads the independent runtime provider under mandatory
Wasmtime. The merged-group negative control fails import matching; the new
groups execute fill/write/repeated-read operations, preserving aliases and box
reference identity, and return 78. This is compiler-layout/raw-provider ABI
execution evidence, not source storage binding or checked-linker composition
acceptance. Monomorphic typed-array boxing and in-place source call projection
remain required; copying a typed array is not an acceptable write ABI.

Validation: 14 focused layout tests, 207 MIR tests, 41 Wasm tests and 31 driver
State tests pass with mandatory Wasmtime. Logs:
`/private/tmp/psrs-storage-groups-{layout,wasm,driver}.log` and
`/private/tmp/psrs-storage-groups-mir-final.log`. Strict backend/driver all-target
Clippy passes (`/private/tmp/psrs-storage-groups-clippy-final.log`); formatting,
diff and maintained Rust file-length checks pass. Full workspace and stdlib
import/conformance runs are omitted under the focused validation scope.

Next: extend linker-owned raw signature contracts to checked GC references and
advertise the storage provider, then connect source operand/payload projection
without array copying. The P8 runtime-binding projection rejection remains;
generic command-runner loading, Effect/ST package migration and integrated
conformance also remain open.

## Checked GC runtime catalog and raw contracts (2026-10-10)

The runtime package now advertises its host-encoded storage unit with operation
metadata and an independent types-only schema. Linker catalog conversion is
fallible and resolves declared references before executable artifact validation.
`CoreTypes` is the shared owner for artifact and application raw signatures:
it retains complete recursion groups, bound recursive references, nullability,
field mutability, finality and supertype edges, while resolving module indices.
Unsupported exact/shared-composite/descriptor/continuation contracts and
multi-result raw signatures report errors. Scalar WIT projection explicitly
rejects GC and SIMD types instead of coercing them to scalar ABI types.

Focused tests establish index-offset independence, recursive binder closure,
mutability/nullability/group mismatches, independent storage export validation,
provider selection, and actual application import-contract acceptance/rejection.
The application has an unrelated preceding type, so its storage array index
intentionally differs from the provider's index. This stage accepts the valid
closed contract and rejects nullable or immutable array drift.

Full component composition is still incomplete. The positive integration test
was executed and failed in `wit-component 0.245.1` minimized-library validation:
`array_fill` has equivalent closed array contracts but different module-local
reference IDs. Its `validate_func_sig` compares `FuncType` values directly.
The positive composition test remains explicitly ignored with this reason,
retaining the required acceptance assertion. It is not counted as passing or
runtime evidence. Logs: `/private/tmp/psrs-gc-provider-compose.log` (observed
failure), `/private/tmp/psrs-gc-provider-compose-focused.log` (one passing
provider/application rejection test, one explicitly ignored composition test).

Validation: the linker suite passes 68 tests before the final application test;
the final linker library run passes 25 tests (one added, for 69 passing tests
across the recorded runs). All 31 mandatory-Wasmtime driver State tests pass.
Backend/driver/linker test-target compilation and strict runtime/linker/backend/
driver all-target Clippy pass. Logs:
`/private/tmp/psrs-gc-provider-{linker-all,linker-final,check,driver,clippy-final}.log`.
Formatting and diff checks pass. Full workspace, official stdlib compilation and
conformance are omitted under the user-selected focused scope.

Next: repair cross-module GC compatibility at the composition boundary without
aligning local type indices or discarding type evidence. Source storage binding
projection, in-place canonical storage, generic command-runner integration,
legacy Effect removal, and official Effect/ST package migration remain required.

## Direct raw Core instance assembly (2026-10-10)

Further composition investigation found that the existing tooling also creates
scalar indirect-call shims for private library imports. Resolving cross-module
reference IDs alone is insufficient. An experimental interface scaffold failed
final component validation because the generated shim still had scalar
parameters/results; that implementation was removed. No scaffold, trap body,
modified runtime module, or debug-write path remains in production code.

`psrs-linker::assemble_core` now owns a distinct raw instance assembly stage.
It consumes the checked link plan, validates the encoded application contract,
and connects original provider/application modules directly by imported module
namespace. This stage supports closed acyclic raw instance graphs; unresolved
imports, host interfaces and cyclic provisioning report errors. Its output is a
provisional encoder plus the application instance identity, not a checked target
artifact. Canonical boundaries and final validation/target-world closure remain
required. Original GC definitions, signatures, bodies and module-local indices
are preserved; private calls do not pass through scalar WIT shims.

A mandatory-Wasmtime integration fixture uses selected catalog storage bytes
and the original application bytes, then adds a canonical CLI boundary in the
test. The resulting component validates and executes fill/write/read with an
aliased array. It requires both the observed payload 78 and reference identity
to agree. Changing only the expected payload to 79 makes the CLI command fail,
proving that the application assertions execute. Nullable array contract drift
is rejected before assembly. This establishes direct raw-instance/component
execution, not generic `compose` WIT integration or source storage projection.
The old positive `compose` test remains explicitly ignored with its actual
reference-ID/scalar-shim limitation; it is still a required acceptance case.

Evidence: `/private/tmp/psrs-gc-core-assembly-runtime-final.log` records two passing
GC provider tests and one explicitly ignored generic-composition test, including
the value-sensitive command execution. Strict linker/backend/driver all-target
Clippy passes (`/private/tmp/psrs-gc-core-assembly-clippy.log`). The general raw
instance stage must next integrate with canonical world assembly and shared
memory/host provisioning; official Effect/ST migration remains open.

Final focused rerun: 70 linker tests pass with mandatory Wasmtime and one
explicitly ignored generic-composition acceptance test; 31 driver State tests
pass (`/private/tmp/psrs-gc-core-assembly-{linker,driver}.log`). Formatting and diff
checks pass. Full workspace and official stdlib/conformance remain unrun under
the focused validation scope.

## Canonical world boundaries for raw GC graphs (2026-10-10)

The normal `compose` entry now selects direct raw instance assembly when checked
source/provider interfaces contain GC references. Verified artifacts retain the
complete declared interfaces after executable-contract validation, including
required operation interfaces, so composition does not infer capability from
library or Effect names. Scalar-only graphs retain the existing assembly path.

WIT tooling consumes a declaration-only application projection with unchanged
public function types/indices and storage declarations. Private imports become
local declarations for inspection; their implementation bodies are not supplied
to tooling. The projection is then removed entirely. Original application and
provider bytes are inserted through raw Core instance assembly, and canonical
aliases are explicitly remapped to the actual application module and instance.
The transformation tracks root/nested scopes and outer-module references,
rejects missing/repeated module identities or unexpected/repeated application
provisioning, and requires both module and instance correspondence. Full Wasm
validation and the checked external-world closure precede artifact publication.
This projection is not a foreign implementation and never executes.

The previously ignored positive `compose` GC test is enabled and passes. It
asserts that both original Core modules survive byte-for-byte in the component,
then executes its CLI command under mandatory Wasmtime. Payload 78 and aliased
reference identity pass; the same program with expectation 79 fails. An
independently selected WIT world preserves a public `u32` parameter and result;
a valid Core module with an incompatible `i64` public parameter is rejected.
Tooling diagnostic chains are preserved so this rejection includes its actual
ABI mismatch. The latter case establishes world/signature acceptance, not a
runtime parameter-value claim. Projection tests cover import-only exports,
complete GC signatures, identity rejection, and index/scope remapping.

Validation: 77 linker tests pass with zero ignored tests, including both direct
and normal composition runtime fixtures
(`/private/tmp/psrs-gc-world-boundary-linker-final.log`). All 31 mandatory-Wasmtime
driver State tests pass (`/private/tmp/psrs-gc-world-boundary-driver.log`). Strict
linker/backend/driver all-target Clippy and formatting/diff checks pass. Full
workspace, official stdlib compilation, and library conformance remain unrun
under the user-selected focused scope.

Remaining: raw graphs with host imports or cyclic shared-memory provisioning,
source storage operand/payload projection and canonical in-place storage,
generic language command-runner integration, legacy Effect removal, and
Effect/ST source/runtime conformance. Closed raw GC world composition is now
implemented; it does not establish those broader requirements.

## State callables in polymorphic payload storage (2026-10-10)

Bare callable erasure previously tried to box the terminal State argument.
The shared State call projection now registers a terminal unary logical State
signature with a complete Step result and an erased payload during immutable
layout planning. Ordinary arguments retain the existing erased curry protocol;
State is passed directly and is never captured. Recovery maps only the Step
payload and preserves its field order, labels and nominal source region.

The fresh-region reproducer exposed an adapter factory hidden inside a conversion
sequence. A second reproducer returning an ordinary callable exposed the same
loss inside a Step ProductMap. CC now emits sequence steps and product field
conversions as explicit assignments before dependency publication. The factory
operands and destinations survive into the immutable invocation inventory;
the MIR correspondence verifier remains unchanged. Malformed product map
metadata remains subject to aggregate verification, and expanded field
conversions are checked as ordinary CC operations.

Mandatory Wasmtime source tests execute a stored State callable (42), ordinary
arguments before State (43), a stored array of State callables (44), a recovered
fresh-region callable (45), and a callable Step payload (46). Replacing the
fresh-region action with a RealWorld action rejects at P5. The array case proves
callable transport, not mutable storage alias preservation: array conversions
still copy storage.

After the product conversion change, all 36 driver State tests, 140 CC tests and
207 MIR tests pass. All six generic aggregate audit tests and 21 polymorphic
erasure audit tests pass, as does strict backend/driver all-target Clippy.
Formatting and diff checks pass. Evidence:
`/private/tmp/psrs-state-slot-{driver,cc,mir,generic,polymorphic,clippy}-final.log`.
The full workspace suite, stdlib compilation and official conformance were not
run under the user-selected focused validation scope.

Remaining: canonical alias-preserving source array storage and ordinary runtime
call projection; conversion calls nested in array iteration; host/cyclic provider
provisioning; generic command runner and legacy Effect removal; official
Effect/ST migration and conformance. Valid source storage bindings still report
the explicit P8 projection rejection.

## Canonical source array storage and alias preservation (2026-10-10)

The mutable alias reproducer stores `[10, 20]` in a polymorphic Holder, recovers
the array, writes 99 through the original reference and reads through the alias.
Before the change mandatory Wasmtime returned 10 instead of 99
(`/private/tmp/psrs-state-array-alias-before.log`). Source array layouts now share
the erased-element protocol, including concrete and nested arrays. Elements are
converted at construction, fill, writes, indexing and pattern extraction;
passing the array through polymorphic storage retains the existing reference.
The same source test now returns 99. Pure array updates still clone storage.

Text byte codecs use an explicit private integer-buffer conversion. Canonical
WIT lists box/unbox erased scalar elements, retaining byte range checks. Lists
of records require the semantic projection after element erasure: a shared
payload storage projection selects the same record protocol as the payload
planner, retaining labels and nested field evidence. String layout reachability
also includes checked external projections, so `list<string>` results whose
elements are unused still reserve their required GC string type.

Existing structural tests now require shared source array identities and absence
of polymorphic reconstruction, alongside the original value assertions. This
does not remove ArrayMap for explicit private-buffer conversion or change the
source type checker: distinct element types remain distinct checked types.

Source runtime storage calls still stop at the P8 projection boundary. The
canonical source storage correction is a prerequisite for their in-place ABI;
it does not yet establish integrated runtime provider execution from source.

Focused validation during this slice passed: 37 driver State tests, 17 source
array tests, six generic aggregate audit tests, four parameterized-shape tests,
21 polymorphic erasure tests, nine pattern audit tests, seven byte/codec tests,
146 WASI tests, 140 CC tests and 207 MIR tests. The WASI scope was extended after
canonical storage changed externally returned record and string lists; its final
run has zero failures and zero ignored tests. Strict backend/driver all-target
Clippy, formatting and diff checks pass. Evidence:
`/private/tmp/psrs-state-array-{driver,source,generic,shapes,polymorphic,patterns,bytes,wasi,cc,mir,clippy}.log`.
The full workspace suite, stdlib compilation and official library conformance
remain unrun under the user-selected focused scope.

## Runtime-owned source/raw call projection contract (2026-10-10)

`psrs-runtime::StorageCallProjection` checks the relationship between the
runtime's source storage contract and its raw export ABI. It pairs operands in
order and distinguishes raw payload results, normal void-to-Unit results and
non-returning results. Mismatched arity, operand roles, result types and return
properties reject before a source consumer uses the contract. Compiler binding
validation now consumes this checked runtime projection rather than independently
reconstructing the source/raw arity and return relation.

This is data-only ABI metadata. It introduces no compiler IR dependency, State
materialization, runtime implementation rewrite or successful placeholder for
source execution. The existing P8 source projection rejection remains until
checked MIR calls, result recovery and invocation correspondence consume the
contract. In particular, Unit-producing writes must lower through a real void
call, and a non-returning export must not fabricate a normal successor.

Validation: both runtime projection tests pass, covering all four operations and
five malformed contract variants. The source binding rejection matrix and valid
scheme matrix pass as separate driver tests. The no-std allocator-feature build
checks for `wasm32-unknown-unknown`; strict runtime/backend/driver all-target
Clippy, formatting and diff checks pass. Logs:
`/private/tmp/psrs-runtime-call-{projection,source-contracts,valid-contracts,no-std,clippy}.log`.
No runtime executable was changed, and no new source-to-runtime execution is
claimed. Full workspace, stdlib compilation and conformance remain unrun.

## Runtime binding metadata across the backend boundary (2026-10-10)

The backend side table now has explicit runtime bindings alongside WIT imports.
Core extraction preserves stable symbol, provider/export, source module, source
span and checked scheme identity. Core-boundary validation compares these
records with the authoritative source externals and rejects missing, duplicate,
unrelated or substituted entries. Direct CC input also checks binding inventory;
it cannot bypass the outstanding runtime projection by supplying a side table.

This preserves the information required by the future raw-call lowerer without
embedding provider names in target-neutral CC or changing a source declaration
into an intrinsic. The source P8 projection rejection and direct-CC P9 rejection
remain explicit. No source storage execution is claimed yet.

Validation: nine binding tests, 37 driver State tests and 207 MIR tests pass.
Strict backend/driver all-target Clippy passes. Logs:
`/private/tmp/psrs-runtime-binding-{metadata,state,mir,clippy}.log`.
The full workspace suite, stdlib compilation and conformance were not run.

## Source runtime calls and the CC/raw contract (2026-10-10)

Ordinary runtime externals now retain their declaration signatures in CC. Layout
planning includes checked runtime declaration schemes even when unused. Calls
and partial applications obtain parameter/result conventions and checked
instantiation from the authoritative `Core.external_types` scheme, through the
same callable boundary used for ordinary declarations. This fixes a real
mismatch between a polymorphic external's stored Step payload and a concrete
use-site result; no provider name or storage opcode is added to CC.

Source fixtures publish and verify two consecutive reads, a fill/write/read
successor chain, and a partially applied read. The original source-to-CC P8
projection guard is removed. Runtime bindings still stop at `P9 runtime
projection` before physical calls are emitted, so this is CC evidence rather
than integrated runtime execution.

Before that remaining rejection, binding validation checks every runtime CC
signature against the runtime-owned raw contract. It requires a final State,
checked Step fields, ordinary operand arity, Integer indices/lengths/Unit and
non-null canonical erased-element arrays. Tests reject noncanonical scalar
arrays, products substituted for arrays, nullable array references, provider
and export substitution, and malformed logical call signatures. Core still
owns source element equality and nominal region agreement; CC shape checks do
not reconstruct these erased semantic facts or authorize dependency erasure.

Focused validation: 13 backend binding tests and 40 driver State tests pass with
zero ignored tests. `PSRS_REQUIRE_WASMTIME=1` is enabled for the driver run; the
three new runtime-call fixtures validate CC rather than execute source runtime
storage. Logs: `/private/tmp/psrs-runtime-cc-contract.log` and
`/private/tmp/psrs-runtime-cc-state.log`. Strict backend/driver all-target Clippy,
formatting and diff checks pass (`/private/tmp/psrs-runtime-cc-clippy.log`).
Full workspace tests, stdlib compilation
and official library conformance remain unrun under the user-selected scope.

## MIR invocation operand correspondence (2026-10-10)

Inspection of the raw-call boundary exposed an existing shared verifier gap:
MIR correspondence checked call destinations and producers but did not compare
ordinary call operands. A valid checked source body and MIR certificate could
therefore survive substitution of an argument with a different value of the
same physical type. The focused regression failed before the production fix
because the mutated call was accepted.

The common control correspondence verifier now compares the complete ordinary
operand sequence for direct and closure calls against immutable CC, removing
only values whose checked CC declarations have State shape. Source IDs, order,
arity and duplicate occurrences remain authoritative. Existing factory calls
and closed runner invocation checks retain their explicit operand contracts.
A raw ABI adapter must provide checked conversion correspondence; this repair
does not authorize replacing those checks with type compatibility.

Focused regressions reject same-typed substitution, operand reordering, missing
and extra arguments. The indirect-call regression lowers a real CC closure
signature through MIR, then proves that module verification rejects an operand
substitution. All 16 MIR State tests and 40 driver State tests pass with zero
ignored tests and mandatory Wasmtime enabled. Logs:
`/private/tmp/psrs-state-call-operands-before.log`,
`/private/tmp/psrs-state-call-operands-after.log`, and
`/private/tmp/psrs-state-call-operands-driver.log`.
The runtime raw-call projection remains incomplete at P9; full workspace,
stdlib compilation and library conformance remain unrun.

## Runtime raw import signatures in the application arena (2026-10-10)

Runtime bindings now enter P9's private physical planning after their retained
CC contract is checked. Their physical callable signatures remove the final
State and project Step to its payload through the shared State call protocol;
immutable logical CC remains authoritative. State-aware WIT imports retain the
existing rejection. The generic layout reachability result now retains its
actual direct-call set, so raw runtime type planning selects only referenced
bindings instead of introducing a second storage-specific call scanner.

The runtime import planner consumes the runtime-owned raw projection and the
application's actual `PlannedLayout`. Integer operands become i32; raw elements
become nullable eqref. Array IDs come from the application's representation
mapping, and the resulting target definitions must be mutable arrays of
nullable eqref. It does not reuse runtime-private type indices or fabricate an
i32 handle. Raw writes and traps both return void, while their result contracts
remain distinct Unit and Never projections.

Planning rejects contradictory immutable, non-null-element and scalar-element
array target definitions. Tests cover all four raw signatures and retained
State-free physical callable signatures. Source State fixtures now reach these
checks before the same explicit P9 call-projection guard. No raw invocation or
MIR module is published for these bindings yet: boxing/recovery, real void-call
handling, non-returning control flow, immutable invocation correspondence and
provider identity through linking remain necessary for source execution.

Validation: two raw-signature tests, 13 binding tests, 16 MIR State tests and
40 driver State tests pass with zero ignored tests. Mandatory Wasmtime is enabled
for the State runs. Strict backend/driver all-target Clippy, formatting and diff
checks pass. Logs: `/private/tmp/psrs-runtime-mir-signature.log` and
`/private/tmp/psrs-runtime-mir-signature-{bindings,state,driver,clippy}.log`. Full workspace tests,
stdlib compilation and official library conformance remain unrun under the
user-selected focused scope.

## Raw runtime identity through MIR and target linking (2026-10-10)

MIR imports now have optional explicit runtime provider/export identity. The
ordinary source call symbol remains unchanged, and WIT/generated/artifact
imports retain their existing contracts. Runtime signature planning copies this
identity from the source binding. MIR verification and target linking check
provider/export lookup, raw operand/result types and canonical array storage;
a source runtime descriptor cannot replace a reserved generated/artifact
binding. Both planning and verification use one raw storage type predicate.

The backend converts the application's actual MIR GC arena into a types-only
Wasm schema, using the existing target encoder conversion, and asks the
linker's `CoreTypes` owner for closed raw signatures. Complete recursion groups
and application-local type mappings survive this boundary. Target selection
uses the runtime unit's operation identity/version rather than inventing an
intrinsic or copying the offered provider's signature as the expected consumer
contract. A valid MIR mutable-eqref array with different finality is rejected
by linking, proving that the actual consumer definition reaches the comparison.

A hand-built raw MIR consumer now passes verification, checked provider
selection, Wasm emission, component composition and mandatory Wasmtime
execution. It allocates an array initialized with 40, preserves an alias,
performs a real void write of 99, reads through the alias, casts/unboxes the
stored payload and returns a comparison against 99. Changing only the expected
value to 100 produces a failed command, proving the execution check depends on
the stored value. This is backend raw-call
execution evidence, not source State/Step projection or official library
acceptance. The source P9 guard remains.

Execution uses `wasi_cli: false` for a closed provider graph. The default CLI
profile adds a host exit import and currently rejects GC-provider composition
with `raw instance assembly requires a closed provider graph`. Supporting that
host graph remains required; this test does not narrow the design or assert
default CLI acceptance. Unknown/substituted providers and exports, wrong
scalar/result/nullability contracts and reserved-symbol substitution reject at
both MIR and linking boundaries.

Focused evidence is recorded in `/private/tmp/psrs-runtime-provider-linking.log`:
four linking tests pass, including the two new raw runtime tests, with zero
ignored tests and mandatory Wasmtime enabled. The final value-sensitive run is
`/private/tmp/psrs-runtime-provider-execution.log`. Two raw-signature tests,
16 MIR State tests and 40 driver State tests also pass with zero ignored tests;
strict backend/driver all-target Clippy, formatting and diff checks pass. Logs:
`/private/tmp/psrs-runtime-provider-{mir,state,driver,clippy}.log`.
Full workspace tests, stdlib
compilation and library conformance remain unrun.

## Source-anchored raw instruction plans (2026-10-10)

`StorageInvocation` now obtains the actual call assignment from a checked CC
State body, resolves its ordinary binding and uses the application's planned
raw signature. Its private immutable facts retain caller ownership, original
ordinary operands, source result destination, logical/physical value types and
fresh temporary frontier. Reserved provider identity is checked through the
shared MIR runtime import verifier. Temporaries cannot reuse logical State or
other source value identities.

For canonical erased payloads, fill/read/write instruction plans emit real raw
calls. Nullable-eqref operand adaptation has explicit cast witnesses. Read
recovery consumes the actual nullable result and writes the checked non-null
source payload destination. Write emits `CallVoid`, followed by Unit's normal
integer representation; it creates no physical State. The invocation verifier
checks operand identity/order, actual producer, fresh typed temporaries,
recovery provenance and complete instruction coverage. It rejects an omitted or
retargeted write, fabricated Unit, substituted same-typed index, detached result
recovery and an extra invocation.

P9 exercises these plans for actual referenced source calls before the existing
whole-function guard. The resulting sequences are checked intermediate plans;
they are not yet inserted into a published MIR function. Immutable dependency
and CFG correspondence must consume the plans before that guard can be removed.
Direct monomorphic element declarations still require the shared canonical
payload conversion boundary; this slice does not replace it with private
boxing rules or an arbitrary reference cast. Non-returning operations remain
explicitly rejected pending their distinct CFG projection, rather than
fabricating a normal successor or payload.

Focused validation: six runtime tests and 40 driver State tests pass with zero
ignored tests (`/private/tmp/psrs-runtime-invocation.log` and
`/private/tmp/psrs-runtime-invocation-driver.log`). The driver run requires
Wasmtime; the new per-invocation tests inspect and mutate raw instruction plans
rather than execute source runtime storage. Full workspace tests, stdlib
compilation and official library conformance remain unrun.

Strict backend/driver all-target Clippy, formatting and diff checks pass;
Clippy evidence is `/private/tmp/psrs-runtime-invocation-clippy.log`.

## External binding ownership and whole-function raw calls (2026-10-10)

This checkpoint supersedes the earlier source P9 guard and MIR provider-lookup
claims. Storage provider/export lookup, physical array-definition checks and
source-to-raw signature planning now belong to `bindings/runtime/target.rs`.
MIR's provider identity is opaque: ordinary MIR verification does not interpret
storage export names. The binding adapter supplies physical signatures and a
provider-independent `RawCallResult` convention. Target linking continues to
validate provider identity and the actual consumer GC types against the runtime
catalog. A hand-built MIR import with substituted provider identity is
structurally valid MIR but fails binding/link validation. Source-derived calls
also reject that mutation through their immutable checked import evidence.

`RawInvocation` plans are now inserted into actual function blocks. Dependency
correspondence retains the source assignment, owning block, instruction range,
typed temporaries and checked import. Returning fill/read/write calls publish
MIR only after their instruction witnesses and source CFG correspondence pass.
Unused valid runtime declarations do not produce imports. Required
non-returning CFG adaptation and monomorphic payload conversion remain explicit
unsupported boundaries.

Whole-function rejection tests cover same-typed index substitution, changing or
removing the checked provider identity, deleting an actual void write and
fabricating its Unit recovery. Normal returning operations pass MIR optimization.
The source regression includes fill, an array alias, an ignored write result
and a read through the alias; it reaches optimized MIR and Core Wasm emission.
A source execution attempt exposed dead-instruction deletion of Unit recovery.
Functions retaining dependency certificates now also defer dead pure deletion,
matching the existing restrictions on copy/CFG/inlining transformations. This
is an explicit optimization limitation until these passes transfer immutable
source correspondence; optimization integration is not complete.

Source storage execution was attempted but not established. With
`wasi_cli: false`, the complete artifact pipeline rejects at P11 target
capabilities. The existing default-CLI GC provider/host composition obligation
also remains. The source regression consequently proves projection and Core
Wasm emission, not a runnable composed source artifact or library conformance.
The independent hand-built raw MIR alias-write test still composes and executes
under mandatory Wasmtime, including its value-sensitive failing comparison.

Focused evidence: nine raw-call/backend tests, 41 driver State tests and two
runtime linking tests pass, with zero ignored tests. Driver and linking runs
require Wasmtime. Logs are `/private/tmp/psrs-effect-boundary-{mir,driver,linking}.log`.
The failed source execution investigation is retained in
`/private/tmp/psrs-effect-source-storage.log`. Full workspace tests, full stdlib
compilation and official Effect/ST conformance remain unrun.

Strict backend/driver all-target Clippy passes
(`/private/tmp/psrs-effect-boundary-clippy.log`).

## Default CLI composition with GC providers (2026-10-10)

This checkpoint supersedes the prior default CLI/host-graph blocker for closed
GC storage providers. Core preparation instantiates independent acyclic provider
modules and retains the original application module without instantiating it.
The WIT facade keeps permitted host imports while replacing private imports
with local declarations. Its imported-function permutation is applied to public
exports, globals and element segments. After WIT tooling produces canonical host
instances, attachment instantiates the original application with those instances
and the checked raw provider instances. The facade never supplies executable
application bodies. Module and instance remapping accounts for component scopes;
final validation and exact host-import closure checks remain mandatory.

The backend raw MIR alias-write test now uses the default CLI target, including
its real WASI exit host import. Expected 99 succeeds and expected 100 fails.
More significantly, an ordinary source State/Step program now compiles through
the normal default artifact pipeline and executes under mandatory Wasmtime:
fill 40, alias the array, write 42, then read through the alias. Comparing the
result with 42 succeeds; comparing with the old value 40 fails. This establishes
source-to-component storage execution and alias preservation for this fixture.

An additional source execution fixture defines ordinary `Task` newtype,
`bindTask`, fill/write/read wrappers and `runTask`. Its callbacks sequence real
storage operations through the library-owned combinator and return 42 under the
default CLI. No Task/Effect-name dispatch or whole-combinator runtime operation
is involved. This is library-form composition evidence, not official Effect/ST
package migration or conformance.

Remaining obligations include non-returning invocation CFG projection,
monomorphic payload conversion, general loops/scoped transport, dependency-aware
optimization, shared-memory/provider cycles and providers requiring later host
provisioning. The disabled-CLI complete artifact profile is still unsupported;
this change executes using the supported default target rather than weakening
that capability check. The locked Effect/ST package and legacy command mechanism
have not been migrated.

Evidence logs: `/private/tmp/psrs-effect-host-{boundary,gc,backend,source,newtype}.log`.
Six boundary tests, four GC composition tests, two backend runtime tests and the
two new mandatory source execution fixtures pass. Full workspace tests, full
stdlib compilation and official library conformance remain unrun.

The final focused driver State cohort passes all 43 tests, with zero ignored
cases and mandatory Wasmtime (`/private/tmp/psrs-effect-host-driver.log`). Strict
linker/backend/driver all-target Clippy, formatting and diff checks pass; Clippy
evidence is `/private/tmp/psrs-effect-host-clippy.log`.

## Non-returning external call projection (2026-10-10)

This checkpoint supersedes the prior non-returning P9 rejection for checked
runtime calls. The binding adapter retains its provider-independent `Never`
convention separately from normal void-to-Unit. Source invocation planning keeps
that immutable convention with the actual source assignment and checked import.
Emission produces the real `CallVoid`, then a typed bottom projection
(`Unreachable`) and a `Trap` terminator. No runtime payload, physical State or
Unit constant is produced.

The checked source dependency graph describes abstract normal-return
continuations. Target correspondence refines the exact certified non-returning
invocation to a terminating path. Physical lowering and correspondence stop at
that invocation; later assignments on that path are not emitted. Normal sibling
branches retain their calls and join edges. Refinement is justified by the
frozen checked raw return contract, not by export-name recognition in MIR or by
assuming all void calls terminate. The invocation verifier requires its actual
producer, bottom destination and full instruction coverage; whole-function
verification additionally requires the invocation range to end the block and
its terminator to be Trap. Changing the checked import, substituting Unit,
adding a physical continuation or attaching a normal return rejects.

Mandatory source execution covers direct trap, a normal-looking read after trap,
constant choices and choices controlled by a real runtime read. A stored 0
selects termination; a stored 1 selects the normal sibling and returns 42. The
source uses ordinary foreign binding metadata and the default CLI pipeline.
Provider termination does not become a new storage intrinsic or Effect handler.

Evidence: the source runtime case passes all six scenarios with mandatory
Wasmtime (`/private/tmp/psrs-effect-never-source.log`). Backend raw invocation
and whole-function tests are recorded in `/private/tmp/psrs-effect-never-mir.log`.
Official Effect/ST migration, monomorphic payload conversion, general loops,
scoped transport and dependency-aware optimization remain incomplete. No full
workspace or full stdlib acceptance claim is made.

Final focused validation: 10 backend raw-call tests, 16 MIR dependency tests and
44 driver State tests pass, with zero ignored tests. Mandatory Wasmtime applies
to the driver execution run. Logs:
`/private/tmp/psrs-effect-never-{mir,state,driver}.log`. Strict backend/driver
all-target Clippy, formatting and diff checks pass; Clippy evidence is
`/private/tmp/psrs-effect-never-clippy.log`. Full workspace tests and full stdlib
compilation remain intentionally unrun.

## Generic Core command entry normalization (2026-10-10)

`psrs-core::command::normalize_entry` consumes resolved entry/runner symbols and
checked Core. It requires a monomorphic source runner `T -> Int` and a
monomorphic entry of type T, while preserving direct Int entries. It allocates
a fresh symbol in the entry's source namespace and constructs ordinary Core
`runner main`, preserving the selected main range. Candidate verification is
transactional: missing identities, invalid schemes, nominal input mismatch or
verification failure leave the original Core unchanged. The helper never names
Effect, synthesizes combinators or adds a runtime dispatcher.

The driver frontend lowering now has an explicit runner selection path exercised
by a focused source harness. It resolves the named source declaration once,
normalizes before pruning and retains ordinary runner dependencies. The existing
production entry selection and legacy Effect path continue to use their prior
mode. Manifest parsing, explicit runner-root loading and production selection
are **not yet wired**; this is checked Core conversion and source execution
evidence, not completed package configuration or legacy-mechanism removal.

Four focused cases cover an unrelated `Command` newtype, direct Int entry
preservation, missing runner, polymorphic runner, non-Int result, distinct nominal
input type, rollback on failed selection and an ordinary State newtype main
without an explicit runner call. The last source fixture uses the default
backend artifact pipeline and returns 42 under mandatory Wasmtime. Its runner is
an ordinary source function in a separate module. Evidence:
`/private/tmp/psrs-command-driver.log`. Strict Core/driver all-target Clippy passes
(`/private/tmp/psrs-command-clippy.log`). Official Effect/ST migration, package
configuration and legacy deletion remain incomplete.

The final focused State cohort passes all 48 tests, with mandatory Wasmtime and
zero ignored cases (`/private/tmp/psrs-command-state.log`). The existing legacy
entry cohort passes all 15 tests (`/private/tmp/psrs-command-legacy-entry.log`),
checking that extraction of frontend lowering preserves entry precedence,
source origins and the existing compilation path. Formatting and diff checks
pass. No full workspace or stdlib compilation run was performed.

## Production package runner selection (2026-10-10)

This checkpoint supersedes the preceding checkpoint's unwired production
selection limitation. The driver now reads
`compiler_contract.command_runner = { "module": "Runner", "function": "run" }`
from the package manifest. Executable compilation adds its trusted module and
import closure as an explicit root, resolves a source identity in the trusted
prefix and normalizes checked Core before pruning. Compile, report and diagnosis
share this path. A configured runner bypasses legacy TrustedEffect discovery
and lexical runEffect restrictions. The unchanged locked package has no runner
configuration and temporarily retains its legacy path.

CLI integration cases use a synthetic development package, separate from the
official vendored sources. Main constructs `Job Int`, an ordinary State newtype,
without importing or invoking Runner. Mandatory Wasmtime observes 42; a direct
Int entry observes 7. The State newtype case also imports a Prelude containing
an unrelated opaque Effect declaration without legacy operations, proving that
runner configuration does not activate the legacy contract. Negative cases
reject a missing trusted module, missing declaration, non-Int result,
polymorphic runner and nominal input mismatch. An isolated check-only case
accepts the source despite an absent executable runner root; report and diagnosis
reject that configuration with library-origin diagnostics.

These cases live in `crates/psrs-cli/tests/command_runner.rs`. They establish
production entry selection, not official Effect/ST package migration. The lock,
official sources and legacy deletion remain unchanged; full stdlib acceptance
and general dependency/optimization coverage remain incomplete.

Final focused evidence: all four CLI integration cases pass with mandatory
Wasmtime, including successful report/Core dump and traced diagnosis retaining
the runner root (`/private/tmp/psrs-command-package-regression.log`). The entry
filtered driver cohort passes 21 tests with zero ignored tests
(`/private/tmp/psrs-command-package-entry.log`). Driver/CLI all-target strict
Clippy, formatting and diff checks pass. The manifest parsing cohort previously
passed all four package tests (`/private/tmp/psrs-command-package-tests.log`).
No full workspace tests or full stdlib compilation were run.

## Real-package migration candidate and callable transport (2026-10-10)

A development copy of the independent package is retained at
`/private/tmp/psrs-effect-state-package`, with preparation script
`/private/tmp/psrs-effect-migrate.py`, package-relative `migration.patch` and
`migration-status.json`. It restores the official Prelude surface, places the
hidden Effect newtype in Effect, retains the pinned official Effect instances,
implements foreign combinator slots as ordinary source functions, configures
PSRS.Command.run and changes PSRS.ST.Action/storage to the shared State
protocol. This is a migration candidate, not an accepted package revision.
Official API auditing, Effect.Ref, host I/O and failure wrappers, ST.Global
behavior and conformance remain unverified. The original independent checkout
and compiler stdlib lock are unchanged.

The first real candidate failure was official `Functor Effect`'s `map = liftA1`
at P8. Abstract callable-constructor transport attempted to recover the entire
Step product through ordinary payload storage, including its State field.
The repair makes the shared protocol registry retain the successor and erase
only the payload. Constructor transport consumes that registry and the existing
State projection for the result conversion; it adds no Effect name dispatch.
Protocol planning now rejects missing signatures and invalid State call shapes.

The same `Main.purs` and package fingerprint recovered from P8 rejection to full
compile acceptance. Comparable traces and their comparison are retained as
`effect-diagnose{,-after}.json` and `effect-compare.log` under the candidate root.
The comparison reports compatible inputs and a changed `backend.cc.lower`
status; canonical artifact-content comparison remains unavailable.
`main :: Effect Unit; main = pure unit` executes with exit 0. A second source
uses official class methods to bind `pure 40` to `pure (value + 2)` and executes
through Effect.Unsafe with exit 42. These executions establish those two
operations in the candidate, not full official Effect support.

The ST reference program is no longer rejected by Core scope verification.
Importing Control.Alt with `main = 0` had reproduced three
`type variable is outside its quantifier scope` errors at its ordinary
`alt = append` definition, which located the defect in the producer's
quantifier evidence rather than in the ST region API. Core scope verification
now opens the leading quantifiers of a record field, projection and array
element before checking that child, and `row_fields` rejects a cyclic row
instead of looping. The eight `record_scope` unit tests cover the repair and
its negative cases; reverting the scope change fails them with the original
message. With that repair the candidate's ST reference program compiles and
executes `Ref.new 40`, `Ref.write 42` and `Ref.read` to exit 42 under Wasmtime,
and the `Control.Alt` import type-checks with `main = 0`.

This is candidate-package compile and execution evidence. It does not migrate
the locked official package, audit the official ST surface, or establish
library conformance; the package lock, official sources and the legacy Effect
mechanism are unchanged.

The focused State cohort passes 49 cases with mandatory Wasmtime, including a
new unrelated State newtype transported through `forall f a. f a -> f a` and
executed for value 42. The CC cohort passes 140 tests. Logs are
`state-tests.log`, `transport-test.log` and `cc-tests.log` in the candidate root.
Full workspace tests and full stdlib acceptance remain unrun.

Three protocol-planning tests also pass (`protocol-tests.log`): a callable with
an ordinary argument followed by State retains both parameters and its Step
successor, while missing signature ownership and a State call without Step
reject. Strict backend/driver/CLI all-target Clippy, formatting and diff checks
pass. The final constructor-transport execution check passes after switching
all consumers to the shared registry result.

### Quantifier-evidence repair verification (2026-10-11)

The earlier claim that the ST reference program remained rejected at P7 was
stale: it described an intermediate state before the scope repair landed. With
the current tree and `PSRS_STDLIB_ROOT` set to the candidate package,
`AltMain.purs` (`import Control.Alt (alt)`, `main = 0`) type-checks, and
`STMain.purs` compiles and executes `ST.run` with `Ref.new 40`, `Ref.write 42`
and `Ref.read` to exit 42 under Wasmtime 49.0.2.

Attribution was confirmed by reverting `verify/scopes/expr.rs` alone: the three
positive `record_scope` cases fail with
`type variable is outside its quantifier scope` and the five negative cases
still pass. The candidate package sources were not modified after the original
log was written, so the earlier failure came from the compiler state at that
time rather than from the package.

Validation on 2026-10-11:

- `cargo test --workspace --no-fail-fast`: passed, 1,921 passed, 0 failed and
  the 5 existing ignored official-suite scoreboards; doc-tests ran with no
  failures. Two independent runs agree.
- `cargo test -p psrs-core --lib record_scope`: 8 passed.
- `cargo fmt --all --check`, `git diff --check` and
  `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- The 7 source-layout violations previously reported by
  `crates/psrs-cli/tests/source_layout.rs` are repaired by module-directory
  splits; that target passes.
- `crates/psrs-driver/src/tests/cc_ir_audit.rs` asserted the generic dictionary
  adapter inside a `ProductMap` plan. CC now emits product field conversions as
  explicit assignments, so the audit follows the adapter to its top-level
  conversion and additionally checks that its result is rebuilt by `ProductNew`.
  Reverting `emit_product_map` fails the audit.

These repairs do not migrate the locked official package. The stdlib lock, the
official sources and the legacy Effect mechanism are unchanged, the official
suite scoreboards remain unmeasured, and Effect/ST API auditing and conformance
remain open.

## Canonical host invocation correspondence (2026-10-11)

A source probe calling `get-stdout` and then explicitly dropping the returned
stream first rejected at P9 with `UnprojectedState`. The external guest
projection retained the logical State parameter and complete Step even after
the external signature was physically projected. Projecting ordinary arguments
and the Step payload removes that layout requirement without deleting or
renumbering the representation table. Production layout planning recomputes
reachability from the physical module; unused logical Step entries are not
physical layout obligations.

The same probe then exposed the missing correspondence between a source call
and its canonical adapter. Canonical calls change the target identity, may widen
operands or use return areas, build aggregate results, and perform cleanup.
The adapter now builds an immutable plan in an isolated instruction builder
before actual emission. The plan retains source anchors, the checked binding,
instructions, temporary declarations, generated blocks and its exit port.
MIR dependency verification checks the complete adapter and contracts only that
checked subgraph when comparing source invocation order and control edges.
Existing runtime invocation evidence continues to check actual physical block
and instruction anchors, including when a canonical adapter precedes it.

A variant-result execution case also exposed a concrete result being passed
directly into an abstract Aggregate block parameter. Canonical results are now
constructed at their projected concrete types before conversion to an abstract
aggregate reference or erasure through the existing payload protocol. A malformed
State signature no longer falls back to ordinary WIT conformance checking.

Focused evidence:

- `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend --lib`: 479 passed,
  zero failures and zero ignored tests. This includes two new mutation tests
  covering canonical targets, operands, Unit results, import signatures,
  invocation reordering and added calls.
- `PSRS_REQUIRE_WASMTIME=1 PSRS_STDLIB_ROOT=/Users/biu/Projects/psrs-stdlib
  cargo test -p psrs-driver --lib tests::state::host -- --nocapture`:
  three passed, zero ignored tests. Five source-newtype execution paths cover
  resource get/drop, widened `u64` input, record result, `list<string>` result
  with nonempty argv, and nested variant result followed by a checked runtime
  storage read in the same State chain. Changing a variant adapter's
  internal switch edge rejects during module verification. The list returns
  the observed argv count of three; the other paths return 42.
- The CLI resource probe builds with the explicit development package and
  `wasmtime run` exits with 42. Before/after logs are
  `/private/tmp/psrs-wit-state-resource-before.log`,
  `/private/tmp/psrs-wit-state-resource-after.log`, and
  `/private/tmp/psrs-wit-state-resource-plan.log`.
- `cargo fmt --all --check`, `git diff --check`, and
  `cargo clippy -p psrs-backend -p psrs-driver --all-targets -- -D warnings`
  pass. Test logs are `/private/tmp/psrs-effect-backend-after.log` and
  `/private/tmp/psrs-effect-host-driver.log`.

The compiler lock and sibling package are unchanged by this slice. A live count
of the development package finds 92 WIT foreign declarations returning Effect:
Clock 5, Random 5, Network 27, IO 20, Process 3, and FileSystem 32. The earlier
handoff's count of 55 does not cover that current surface. Their ordinary
library wrapper migration and complete API/conformance evidence remain open.
General source dependency loops, multiway control, scoped transport, provenance
requiredness, legacy deletion and optimizer coverage remain separate obligations.
No workspace test suite or official scoreboard was rerun for this slice.

## Public WASI wrappers in the development package (2026-10-11)

The independent `psrs-stdlib` checkout now has an uncommitted host-wrapper slice
on top of `7852625`. All 92 target WASI foreign declarations use private
State-stepping source signatures. Their public signatures, exports and existing
pure wrapper algorithms are preserved. A target-only `PSRS.Effect.fromState`
owns the narrow coercion into the package's hidden Effect newtype; it does not
execute the callable or manufacture a State value. The package inventory now
records that helper and the previously unclassified `PSRS.Command` as target
sources. No official source file changed in this host-wrapper slice.

The first public resource program rejected at P8: a foreign function used as a
value was looked up only among local source declarations. CC now consumes its
checked external scheme through the common partial-application path with zero
supplied arguments. Source identity, checked use types, normal conversions and
closure construction therefore retain the same owner as partially supplied
foreign calls. The regression uses an unrelated `Native` newtype and executes
get/drop to 42; it contains no Effect name or compiler-special library adapter.

The library-owned runner accepted all six versioned public API fixtures under
`conformance/effects/` in the actual package:

- Resource get/drop returns 42.
- Console construction and repeated execution print exactly `first`, `later`,
  `later`, each on its own line, and the configured Effect Unit runner exits 0.
- Arguments consumes the canonical host list and returns 42.
- Clock decodes its record, creates/drops a duration subscription and returns 42.
- The unchanged official GlobalST conversion allocates, writes and reads 42.
- Exit returns 47 and the following console action produces no output.

The evidence reports are `/private/tmp/psrs-effect-package-<case>/run.json`,
with case names `resource`, `console`, `arguments`, `clock`, `globalst`, and
`exit`. All reports require Wasmtime, check exact outputs and record unchanged
package fingerprints. The umbrella WASI import compiles with the actual
development package; this is compile acceptance, not execution of all imports.

`node conformance/st.mjs /private/tmp/ps-pkgs/purescript-st
/private/tmp/psrs-effect-state-st-oracle` evaluates ten observations against
the pinned official ST JS implementation. The same package runner accepts its
generated source with exit 42 in `/private/tmp/psrs-effect-state-st-runtime`.
This includes cell allocation/read/write, map/bind, while/for/foreach and empty
or false conditions that must not invoke their callback. It does not establish
general dependency CFG transformation or every public ST API.

The pinned source audit in `/private/tmp/psrs-effect-package-audit` checks all
41 reference packages and retains 170 identical / 36 modified official modules,
155 nonrecursive replacements, 3 removed foreign declarations and 130 retained
foreign declarations. Target additions increase from 24 to 26: Command obtains
provenance and the new helper adds one module. The earlier unclassified module
count of one becomes zero.

Focused compiler host tests pass 4/4 with mandatory Wasmtime, and package Node
tests pass 11/11. Formatting, both repository diff checks and backend/driver
all-target Clippy pass. Logs include `/private/tmp/psrs-effect-foreign-value.log`,
`/private/tmp/psrs-effect-foreign-host-after.log`, and
`/private/tmp/psrs-effect-foreign-clippy.log`. Compiler staging remains untouched;
no changes were committed or pushed. The normal compiler lock stays at
`1e01310`, so development package consumption must remain explicit.

Complete host/API conformance, the failure wrapper, provenance requiredness,
general scoped/multiway dependency control, legacy deletion and optimizer
coverage remain open. Neither the full workspace suite nor official scoreboards
were rerun for this slice.

## Suspended failure and mixed provider composition (2026-10-11)

The target package now exports `PSRS.Effect.fail`, an ordinary library wrapper
around the checked non-returning runtime storage trap. An unused constructed
action returns 42; executing the action after console output produces exactly
`before\n`, then Wasmtime's terminal `unreachable` trap (observed exit 134),
with no following output. No Effect-specific intrinsic or fabricated normal
State successor was added.

The first combined console/trap case rejected at P11 because the raw provider
schedule attempted to instantiate the scalar allocator before the application.
Its unavailable imports were `env` and `__main_module__`. Composition now keeps
providers with reference-valued import/export interfaces and their transitive
consumers in the raw Core graph. Scalar-interface providers use the existing
canonical library/shim provisioning, retaining their checked scheduling flags.
The allocator therefore receives the original application's memory and heap
boundary. Unsupported raw dependency cycles still reject with the unresolved
provider namespaces included in the diagnostic.

The new facade test checks retained scalar imports, relocated raw function
exports, and unchanged complete type signatures. Its first run incorrectly
used function indices as `CoreTypes.signature` type indices; the test now checks
the actual function type entries. The new source regression executes a canonical
string-list call followed by GC storage read or trap in the selected branch.

Focused validation:

- `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-linker`: all unit and integration
  targets pass, including raw GC storage and allocator heap-boundary execution.
- Driver `tests::state::runtime_cc`: 8/8; `tests::state::host`: 4/4, both with
  mandatory Wasmtime and explicit development package consumption.
- Actual package runner accepts failure, deferred failure, Console, Arguments,
  and GlobalST against the rebuilt CLI. Reports are
  `/private/tmp/psrs-effect-failure-final-<case>/run.json`, with case names
  `failure`, `failure-deferred`, `console`, `arguments`, and `globalst`.
  Every report records unchanged package fingerprint
  `fnv1a64-v1:d3b145fb3d987017`.
- Actual package tool tests: 12/12. Expected-trap mode requires successful
  compilation, completed nonzero execution without a process signal, exact
  stdout and the terminal Wasmtime diagnostic; compile/launch failure, timeout
  or successful execution cannot satisfy it. Full stderr remains captured.
- Pinned source audit covers all 41 checkouts, using the exact checkout paths
  retained by the previous audit. Counts remain 170 identical / 36 modified
  official modules, 26 target additions, 155 nonrecursive replacements,
  3 removed foreign declarations and 130 retained foreign declarations.
  Evidence is `/private/tmp/psrs-effect-failure-final-audit/inventory.json`.
- Formatting, both repository diff checks, and linker/backend/driver all-target
  Clippy pass.

No workspace suite or official scoreboard was rerun. The compiler index remains
at 259 paths, the lock stays at `1e01310`, and no changes were committed or
pushed. Complete API conformance, provenance requiredness, general scoped and
multiway dependency control, optimizer evidence and legacy deletion remain open.

## Required MIR dependency provenance (2026-10-11)

The old module verifier only checked `Function.state` when it was present.
Clearing that field on a projected canonical host function therefore accepted
its physical body without any dependency correspondence checks. The new removal
regression fails when the inventory guard is disabled: `verify_module` returns
`Ok(())` where rejection is required. This comparable baseline is retained at
`/private/tmp/psrs-effect-inventory-without-guard.log`.

P9 now derives one immutable module inventory from the logical CC source before
physical projection. That inventory owns dependency requiredness and also drives
which functions receive projection evidence. Per-function witnesses must refer
to that same source. Retained declarations must be original source declarations
or explicitly registered P9 conversion helpers. Clearing a witness, clearing a
planned module's inventory, using another source's witness, or renaming a
projected declaration to an unregistered identity is rejected. Optimizer entry
verification enforces the same requirement. Whole-function reachability pruning
may still remove an unused dependency function; requirements for live functions
remain immutable and are checked after pruning.

Directly constructed physical MIR uses an explicit empty inventory and may not
attach unowned dependency witnesses. For dependency-free direct CC, the inventory
does not newly claim ordinary CC typing. An initial implementation accidentally
broadened that claim and rejected nine existing ABI cast fixtures; moving
inventory construction to the projection owner and preserving the existing
conditional typing contract restored those cases without changing their bodies.

Focused validation with mandatory Wasmtime:

- Backend library tests: 483/483, zero ignored, including four new regressions
  for missing witnesses, foreign-source witnesses, identity substitution and
  legitimate dead-function pruning.
- Driver State tests with the actual development package: 54/54, zero ignored.
  This includes source runners, scoped callable transport, runtime storage,
  canonical host adapters, command entry and selected-branch termination.
- Formatting and both repository diff checks pass; backend/driver all-target
  Clippy passes. Logs are `/private/tmp/psrs-effect-inventory-backend.log`,
  `/private/tmp/psrs-effect-inventory-driver.log`, and
  `/private/tmp/psrs-effect-inventory-clippy.log`.

This closes the missing per-function evidence bypass for production P9 modules.
It does not prove general scoped transport, multiway/loop correspondence or
optimizer graph rewriting. Complete API conformance and legacy deletion remain
open. No full workspace suite or official scoreboard was rerun, the 259 staged
paths remain unchanged, and the compiler lock stays at `1e01310`.

## Structured multiway State projection (2026-10-11)

The focused three-arm CC probe rejected at P8 with
`CC state switch requires checked multiway dependency projection`; its baseline
is `/private/tmp/psrs-effect-switch-before.log`. CC now shares dependency
fork/join construction between binary and multiway choices. The representation-
independent graph validates every case/default dependency transfer, and trapping
arms never acquire a normal join edge. Concrete switch labels and predicates
remain owned by CC and are checked against actual MIR.

P9 recursively projects every switch arm and checks source block parameters,
case labels, selector, default edge, invocation placement, join payload operands
and return operands. Nested binary choices within a switch retain both joins.
Raw-State choice results still reject explicitly before an entire effectful
switch could be erased; zero-width control-result projection remains open.

The strengthened join check exposed the scalar constant pass removing source
join parameters and jump arguments. Constant folding may still preserve scalar
destinations, but block-parameter materialization now retains source dependency
anchors until it has a checked evidence-transfer operation. An all-constant
payload join regression verifies that optimization leaves those parameters.

Focused mandatory-Wasmtime evidence:

- Core dependency graph tests: 6/6, including stale case/default edge rejection.
- Backend State tests: 27/27; optimizer tests: 8/8. Six actual component runs
  cover two case labels and default, with and without a trapping first arm.
  Normal selected values are 42/43/44; only the selected trap terminates.
  Mutations of selector, label, target, default, call placement and same-typed
  join payload reject through dependency verification.
- Driver State tests: 56/56. The new constructor case executes all three
  source selections, and a predecessor replay after the choice rejects at Core.
- Actual package `conformance/effects/Choices.purs` combines three selections
  through ordinary Effect Bind and checks values 40/41/42 before returning 42.
  Its fourth arm contains an unselected runtime trap. The runner report is
  `/private/tmp/psrs-effect-switch-package-final/run.json`, with unchanged
  package fingerprint `fnv1a64-v1:7160abec40561f94`.
- Formatting, both repository diff checks, and core/backend/driver all-target
  Clippy pass. Logs use the prefix `/private/tmp/psrs-effect-switch-` with
  suffixes `backend.log`, `optimizer.log`, `driver.log` and `clippy.log`.

An adjacent storage projection gap remains explicit: a monomorphic
`Array Int -> Int -> State RealWorld -> Step RealWorld Int` foreign read is
accepted at source binding but rejected at P9 for missing checked canonical
payload recovery. Its retained source and log are
`/private/tmp/psrs-effect-monomorphic-storage-result.purs` and `.log`.
The package fixture uses the catalog's polymorphic source signature and its
existing checked payload transport; this does not close monomorphic raw-ABI
adaptation. Neither a constant result nor permissive physical cast was added.

General loop projection, scoped/erased transport, optimizer graph transfers,
complete API conformance and legacy deletion remain open. No full workspace
suite or official scoreboard was rerun. Compiler staging remains 259 paths;
no commits, pushes or compiler lock updates were made.


## Monomorphic storage payload protocol (2026-10-11)

The preceding monomorphic read baseline is now repaired at Core-to-CC protocol
selection. Core still checks nominal regions and element equality at source
use types. Runtime catalog Element operands and Element results adopt the shared
payload protocol only after those checks; Array, Int, Unit and non-returning
roles retain their contracts. Ordinary CC conversions perform boxing and
recovery. Record conversion uses the supplied storage field representations
alongside checked source field types, rather than rebuilding those shapes from
the concrete source type alone.

The new runtime matrix exposed a callable mismatch: entering erased storage
used the payload curry protocol, but recovery selected a concrete closure cast.
Erased payload endpoints now use their authoritative owner before ordinary
concrete callable adaptation. Int, Number, Boolean, String, record and callable
values execute correctly after write/read; the Int case retains 2147483647,
and the callable case executes the stored replacement callback.

Two monomorphic declarations importing array_read also exposed two independent
linker constraints. Application checking now validates every occurrence against
the complete planned Core signature, accepting matching aliases and rejecting
conflicting types. Component embedding then normalizes checked duplicate
function import names and remaps all function references. A focused structural
case checks calls, exports and element initializers. This normalization does
not replace application bodies or merge source invocation evidence.

Focused validation with mandatory Wasmtime:

- Backend library: 489/489, including catalog role preservation and existing
  malformed storage, dependency and canonical host adapter checks.
- Driver State: 60/60. Four new tests cover scalar/record/callback recovery,
  partial read, nested array sharing through two source aliases, and checked
  CC protocol selection before MIR layout.
- Linker: 80/80 across its library and integration tests, zero ignored.
- The library-owned runner accepts the actual development package with
  ordinary Effect Bind and monomorphic nested-array read/write aliases. Writing
  through the first retrieved element is observed through the second slot;
  execution returns 42 with empty stdout/stderr. Report:
  /private/tmp/psrs-effect-storage-aliases-final/run.json. Package fingerprint
  remains fnv1a64-v1:7160abec40561f94.
- Backend/driver/linker all-target Clippy, formatting and working-tree diff
  checks pass. Logs use /private/tmp/psrs-effect-storage-protocol-final- with
  backend.log, driver.log, linker.log and clippy.log suffixes. The staged-only
  diff check still reports three existing Markdown hard-break spaces at design
  lines 3, 6 and 12; the index was not rewritten.

This closes the concrete monomorphic storage recovery gap, including callable
payloads and alias-sensitive nested arrays. It does not close general loop or
scoped transport, optimizer graph transfers, complete API conformance or legacy
deletion. The full workspace and official scoreboard were not rerun. Compiler
staging stays at 259 paths, the lock stays at 1e01310, and no commits or pushes
were made. The package runner input is retained under /private/tmp; its compiler
regressions are maintained in the driver and linker tests.


## Public recursive Effect loop evidence (2026-10-11)

The actual package's ordinary whileE, untilE, forE and foreachE definitions
already compose through checked calls and branch projection. A combined public
program using unchanged GlobalST reference adapters executes to 42; its report
is /private/tmp/psrs-effect-loops-before/run.json. This evidence must not be
misreported as arbitrary dependency CFG backedge support or stack-safe looping.

The independent package now maintains conformance/effect-loops.mjs,
conformance/effects/Loops.purs and Loops.observations.json. The oracle requires
a clean purescript-effect checkout at the package pin before importing its
actual Effect.js. Ten observations compare while condition/body trace 1212121,
until invocation counts 3 and 1, exclusive for order 12345, negative-bound order
1234, foreach order 314, and four skipped callbacks that would trap. The generated
Wasm input checks each independently and returns 42 only when all agree.
The runner accepts /private/tmp/psrs-effect-public-loops-runtime/run.json,
recording the unchanged package fingerprint fnv1a64-v1:7160abec40561f94 and
actual compiler/runtime hashes. After installing the oracle, fixture and package
documentation, the maintained fixture is independently accepted with package
fingerprint fnv1a64-v1:db4998eb78158e83 in
/private/tmp/psrs-effect-public-loops-versioned/run.json. Each report confirms
its own package remained unchanged during execution; the installation changes
the package fingerprint between the two runs. Package Node tests
pass 12/12 with zero skips; syntax and both working-tree diff checks pass.
No official library implementation or inventory
was changed in this slice.

A separate public MonadRec Effect baseline rejects at P8 library linking:
Effect.Ref._new, Effect.Ref.read and Effect.Ref.write have no target
implementation. The retained source is /private/tmp/psrs-effect-tailrecm.purs;
its failed runner report is /private/tmp/psrs-effect-tailrecm-before/run.json.
The official foreign contracts remain intact. Reference adapter migration is
the next library integration obligation; a tailRecM-specific intrinsic or fake
reference implementation would not satisfy it.

The current requirement map above is refreshed to reflect verified multiway,
monomorphic payload and focused official-loop evidence. Full API/host conformance,
arbitrary CFG backedges, scoped transport, optimizer graph rewriting, legacy
deletion and coherent package pin acceptance remain open. No Rust source was
changed, no workspace or scoreboard was rerun, and the 259-path index and old
compiler library lock remain unchanged.
