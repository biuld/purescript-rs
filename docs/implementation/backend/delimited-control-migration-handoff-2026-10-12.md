# Handoff: Delimited Control Migration and Remaining Acceptance

**Updated:** 2026-10-16. The filename keeps its original date; this file is the
current snapshot. **§20 is the current progress.** Sections 1–18 are history:
§10 is the construction-boundary migration, §§11–12 the operand-level CC
boundary and immutable planning, §13 the bounded model, §14 the optimized
Wasm replay measurements, and §15 Run construction plus the Root regression
slice. Sections 1–14 still use the 39-action inventory from before `Run`
joined the registry. §18 is the native execution and region snapshot from
before the uncurried adapters.
**Objective:** Complete delimited control, then implement native Effect/ST.
**Status:** [DEC-21](../../decision/DEC-21-control-machine-construction-boundary.md)
is implemented. The named schema, a 40-action inventory and checked plans own
construction. Per-IR machine recognizers stay retired. The CC boundary checks
ordered operands; the MIR boundary checks dispatch arms and external tail
targets. R1–R3 have current P8 owners in permanent tests. The bounded model
compares all 40 actions, and §14 records saved-length and queue-shape traversal
on optimized Wasm. The 185 historical recognizer tests have owners in
[the retired-regression document](delimited-control-retired-regressions.md).
That audit does not close gates 4 and 6 by count alone. AE-01–AE-10 and
AC-01–AC-05 are Verified. AE-11 and NE-01–NE-13 are Verified on fingerprint
`fnv1a64-v1:ad19620ada1d2ecf` (§20). Official Effect/ST adapters wrap
`PSRS.Effect.Native`; library-module `Host.lift` of Clock/Random/IO/Console
and `Test.Assert` execute. The D-04 runtime board is 273/413 as of 2026-10-17. A translation
validator is an explicit further assurance choice,
not current work. The
[architecture proof-owner audit](delimited-control-architecture-review-2026-10-14.md#current-proof-owners-2026-10-15)
and the
[lowering implementation audit](delimited-control-lowering-review-2026-10-14.md#implementation-audit-2026-10-15)
remain the review-to-owner tables. §15 records where the code moved past the
39-action text inside §§12–13. §16 records the AE-11 adapter and host-leaf snapshot; §17 continues the
driver runner migration. §18 records native execution, generic checking and
abstract regions. §19 records the uncurried adapters and the driver evidence
from before the public oracle finished. §20 records the finished Effect/ST
and AE-11 acceptance.

The [architecture review and repair evidence](delimited-control-architecture-review-2026-10-14.md#repair-evidence-2026-10-14)
records ownership, regressions, and the moved Wasm ancestry observation.
The pre-repair snapshot and current logs are retained under
`/private/tmp/psrs-control-repairs-2026-10-14/`.

No pushes, PRs, or remote changes were made. No subagents were used.

## 1. Checkout and package identity

| Owner | Current state |
| --- | --- |
| Compiler | `/Users/biu/Projects/purescript-rs`, branch `feat/effect-handlers`; checkpoint commit containing this handoff, based on `6df8bfba59f74659883f8572ac8a88c821a6574e` |
| Independent stdlib | `/Users/biu/Projects/psrs-stdlib`, branch `control-library`, HEAD `d6cf81105046e288e2e9cd9685f8cc47e960c988`; clean and unchanged |
| Compiler lock | Pins that stdlib revision and `fnv1a64-v1:6eb134fa699297f4` |
| Work | Active continuation; complete delimited-control acceptance remains open |

The local checkpoint combines the previously dirty control-verifier draft,
the R1–R3 repairs, permanent tests and their design/acceptance documentation.
Use `git show --stat` on the checkpoint commit for its exact inventory.

The checkpoint implementation spans these topics:

- `control/`: shared dispatch/provenance, Bind/Resume, root/queue execution,
  observable-operation and owned-cell relations; the former
  `control_provenance.rs` is renamed into this directory.
- CC layout/lowering: contiguous computation tags and planned result-pair access.
- CC/MIR reference identity and branch facts: aliases, equivalent joins,
  actual exits and ordered per-path events.
- Paired control verifiers: dispatch, clauses, exits, steps, root/queue execution,
  owned transfer and Start/Once/Cancel publication, alongside existing capture,
  replay, context and finish checks.
- MIR CFG reachability/dominance; planned nominal constructor verification;
  shared `mir/effects.rs` replaces `mir/opt/effects.rs` so optimization and
  semantic checks use the same instruction summaries.
- Driver tests: dispatch, clause steps, execution steps, result provenance,
  `control_queue_steps/` and `control_owned_steps/`.
- Design split and coverage documents: `control-machine.md`, related control
  designs, backend index, algebraic-effects checklist and D-04's earlier
  qualitative update. This review adds an architecture review record and
  updates this handoff; it does not change D-04 measurements or statuses.

## 2. Progress since the 2026-10-12 handoff

The checkpoint commit contains the progress below on top of `6df8bfba`.

**Historical dispatch slice (2026-10-12; full acceptance still open)**

1. Repaired the MIR dispatch dominance rule: the entry preheader's
   initialization jump is distinguished from real loop backedges via
   `value_identity::loop_header`'s contract (`mir/control/dispatch.rs`).
2. Fixed a real shared bug in `mir/cfg.rs::dominators`: unreachable blocks were
   poisoning reachable blocks' dominator sets, so a reachable return block
   appeared undominated. Dominance is now restricted to blocks reachable from
   the entry (the Wasm structurer already worked this way).
3. Projection check semantics: a projection is rejected only when its selected
   case is **proven** (`cc/verify/control/dispatch.rs`,
   `mir/control/dispatch.rs`). Unprovable sources are left to the
   relation-specific verifiers. Dispatch was then restored to the front of the CC
   control chain, so it no longer preempts replay/context-entry diagnostics.
4. Retired the dead v1 `Reenter` tag: the computation variant is now contiguous
   `0..5`, with `OwnedResume` at tag 5. There is no artifact-compatibility reason
   to keep the gap (the machine is compiled in one module; old artifacts are
   rebuilt; there is no persistent checked-IR cache). Touched
   `cc/layout/control/{mod.rs, contracts.rs}`, `cc/lower/control/{mod.rs,
   executor.rs, owned.rs}`, `control/dispatch.rs` (carrier domain
   `[0,1,2,3,4,5]`), the dispatch tests, and the design docs.
5. Dispatch tests now cover, in CC and MIR: coverage (every required
   alternative), wrong projection, non-trapping default, immutable `i32` tag
   (MIR), reference alias/copy preservation (positive), and result origin
   (CC). See §4.

**Clause selection (§6.2, first slice)**

6. New shared contract in `control/provenance.rs`: `ClauseKind { Return,
   Operation }` and `clause_selection_relation`. A source `Return` must apply
   the current scoped context's return clause (field 0); a source `Perform` must
   apply its operation clause (field 1); both from the current context.
7. CC verifier `cc/verify/control/clauses.rs`: on the Executor's dispatch arms
   for tags 0/1, every applied scoped-clause projection must be the clause that
   step owns. It is a **universal** check over applied clause projections: a
   wrong field/frame is rejected. The subsequently added execution-step
   relation checks the required application and its result (see §4).
8. MIR verifier `mir/control/clauses.rs`: the same relation on the real CFG,
   walking each input-dispatch arm (BFS stopping at the loop header) and
   resolving `ClosureCall` callees through reference identity to the scoped case
   `StructGet`.
9. Tests: `cc_source_{return,operation}_applies_its_own_{return,operation}_clause`
   and the MIR equivalents, each swapping the applied clause field.
10. Split `control_dispatch.rs` (which had grown to 619 lines) into
    `control_dispatch/{mod,cc,mir}.rs` to respect the 500-line source limit.

**Documentation split**

11. New design topic `docs/design/backend/fp/control-machine.md` now owns the
    machine representation, per-IR encoding, lowering, flat driver and replay.
    `delimited-control-and-resumable-frames.md` keeps the surface, the
    design-space selection, handler semantics, lifetime and migration, and
    defers the machine to the new doc. `control-elimination-and-handler-scopes.md`
    reconciled its stale machine grammar to the implemented `Root`/`Scoped` +
    `Finish` inventory. `docs/design/backend/README.md` lists the new topic.

## 3. Representation and design contracts to preserve

### Historical continuation: execution-step provenance (2026-10-13)

The historical account and exact test names are in
[the execution-step checklist section](algebraic-effects.md#execution-step-provenance-2026-10-13).
The new slice changes verification only; machine representations, generated
instructions, library sources and the lock are unchanged.

- Each scoped Return/Perform branch must feed the complete clause application
  result to its executor transition, consuming its current answer/request and
  preserving state/root handler. Perform supplies the predeclared raw resumption;
  its factory remains subject to the existing queue/checkpoint capture checks.
- MIR verifies actual branch exits and application dominance. A correct-looking
  call in a dead block cannot supply the proof.
- Bind enters its own body with its own callback and current queue. Resume
  returns its saved answer, appends its replayed queue before current pending work
  and installs the activation from that same checked replay result.
- Thirteen new tests cover 38 malformed-IR mutations. The final focused control
  driver run passes 157 tests with required Wasmtime. The earlier runtime run
  passes all 81 cases across the eleven control integration targets.
- The pinned Node source audit retains 50 packages / 305 modules: 245 identical,
  34 modified, 26 additions, no missing upstream modules. Official control type
  comparison passes 17 cases. Pinned upstream JS RunState/RunChoose/RunHandlers
  each return 42, with their value/order checks.
- Foreign resource capture needs ownership/lifetime evidence retained from the
  canonical ABI into checked guest captures; the existing physical integer and
  local drop/transfer checks do not supply it. Caught-host-trap cleanup/reentry
  remains unimplemented. These obligations have not been weakened.

Evidence is retained in `/private/tmp/psrs-control-completion/` and
`/private/tmp/psrs-control-completion-*.log`. The final public package batch
accepts all 14 cases, including three pure Run checks matching pinned upstream
JS. Official type comparison accepts all 17 cases. Early Run attempts timed out
at 180 seconds; the successful final batch uses 600 seconds and retains real
execution/artifact identities.

The integrated workspace run has exactly the same 71 failed test names as the
reference migration baseline, with no added/recovered names. Two additional
doctest dependency-artifact build errors pass on standalone rerun. Strict
workspace Clippy still fails on the known `psrs-kind` warning; ordinary workspace
Clippy completes with that warning only. The final two environment-identity
guards/tests were checked by the final 157-test control run and rebuilt-CLI
conformance, after the full workspace snapshot. Full workspace was not rerun
after those last guards. See the checklist for exact scope and logs. Ignored
scoreboards and full upstream differential target were not rerun; D-04 gains
only a qualitative coverage update and keeps its existing measurements/statuses.

Normative documents to read together with this handoff:

- `docs/design/backend/fp/delimited-control-and-resumable-frames.md` (surface, selection, semantics)
- `docs/design/backend/fp/control-machine.md` (machine representation, lowering, driver, replay)
- `docs/design/backend/fp/control-elimination-and-handler-scopes.md` (clause scope/lifetime contract)
- `docs/design/backend/fp/cc-ir.md`, `docs/design/backend/fp/mir.md`, `docs/design/backend/fp/data-representation.md`
- `docs/implementation/backend/algebraic-effects.md` (AE/AC acceptance checklist)

Current contracts:

1. The eight `Prim.Control` primitives implement protocol v2. `handle` identity 96
   has arity three. Operation/return clauses execute in the target domain; raw
   continuation invocation returns source-domain `Control` without reinstalling
   the source handler. Deep/shallow policy is ordinary library code.
2. **Computation tags are contiguous `0..5`**: `Return` 0, `Perform` 1, `Bind`
   2, raw `Resume` 3, `Handle` 4, `OwnedResume` 5. The queue is `[0,1,2,3]`
   (`Empty`/`Cons`/`Append`/`Finish`), the context `[0,1]` (`Root`/`Scoped`), and
   the reverse queue `[0,1,3]` (reversed sequences contain no `Append`).
   Unknown computation tags reach the trapping default.
3. Layout planning reserves nominal Control/Resumption carriers and all
   synthesized `SignatureId`s before CC lowering freezes the representation
   table. Do not mutate the table while synthesizing executor/continuation code.
4. Carrier parameters, results, captures and joins use their planned nominal
   representations; only explicit abstract user-payload/storage boundaries erase
   and recover. The machine does not reconstruct source types from erased data.
5. Persistent queues preserve callback/owner sharing; replay rebuilds only
   lifecycle (`Finish`) nodes, each with a fresh child extent.
6. Recursive local functions may return closures before binding every flattened
   parameter; CC supplies remaining parameters through shared lambda adaptation.
   A zero-binder initializer that may call, write or trap is rejected.
7. No concrete Run, State, Reader or Writer identity is recognized by the
   compiler.

### Review repairs: actual exits, constructor tags and joins

The review reproduced two accepted malformed programs and one rejected valid
program. All three now have permanent regression coverage:

- A valid nested queue/context dispatch whose outer computation result was
  replaced with state is rejected. `cc/verify/control/exits.rs` recursively checks
  every executor transition and its containing structured result against the
  actual enclosing exit; all six computation alternatives are covered.
- A recursive executor call whose result is discarded at a normal MIR return
  is rejected by `mir/control/exits.rs`. Scoped clause verification inspects
  actual exit terminators and their returned call results, not the first matching
  instruction. A valid returned call remains accepted.
- `mir/verify/variants.rs` checks all planned nominal variants, including ordinary
  data constructors. Tags remain immutable i32, match their physical case and
  cannot be bypassed by constructing the common base. Copies and equal constants
  at joins retain tag evidence.
- Shared CC reference origins accept equivalent If/TagSwitch joins, excluding
  trapping alternatives. Shared MIR reference origins accept equivalent normal
  joins; `cfg::reachable_from` and `cfg::dominators_from` support proofs within
  one execution step. These checks use IDs, edges and planned layouts.

`control_result_provenance` contains six tests (including all six discarded
computation results and value-sensitive Wasmtime execution returning 43).
Four backend nominal-constructor tests exercise ordinary variants, wrong tags,
copies, joins, direct base construction and mutable tag rejection.

Validation for the repairs: backend library 444 passed; control library 163
passed; all eleven control integration targets 81 passed. Control runs require
Wasmtime. Formatting and diff checks pass. Focused backend/driver Clippy completes
with only the existing `psrs-kind/src/check/synonyms.rs:174` warning; strict
Clippy is blocked by it. Workspace tests/Clippy, scoreboards, the full upstream
differential and public package conformance were not rerun for this focused
repair. Logs: `/private/tmp/psrs-control-review/fix-{backend-final,control-final,integration,clippy,clippy-strict}.log`.

Those three earlier findings were repaired. Root/queue and owned execution
checks have since advanced as recorded below, but the new review reproduces
the pre-repair lifecycle-body and observable-operation gaps. These are repaired
in §8. Foreign capture,
caught-host-trap recovery and complete AE/AC acceptance remain open. Earlier full
workspace evidence is historical and was not refreshed in this review.

## 4. Machine verification after recognizer retirement

The historical per-IR recognizers that earlier revisions of this file described
(`cc/verify/control/*`, `mir/control/*`, and the shared `control/` relations)
have been **retired**. Machine semantics now live in the checked construction
plan (`cc/lower/control/plan/`) and are re-checked by boundary validators at
`cc::verify_module` and `mir::verify_module`. Generic IR verification still
checks operands, types, layout, casts and CFG.

See §10 for the current boundary checks, the assurance tradeoff, and the precise
remaining work. Sections 5–9 are retained as history.

## 5. Historical validation (before the 2026-10-14 review)

Environment: Wasmtime 49.0.2 (`3c8a3e79a`, 2026-10-02), default locked stdlib
`d6cf811`, `PSRS_REQUIRE_WASMTIME=1` for execution.

| Command / scope | Observed result |
| --- | --- |
| `cargo test -p psrs-backend --lib` | 440 passed, 0 failed |
| `cargo test -p psrs-driver --lib tests::control` | 144 passed, 0 failed |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test control_surface --test control_runtime --test control_open_runtime` | all passed |
| `... --test control_scoped_runtime --test control_scoped_surface --test control_scoped_replay --test control_owned_runtime --test control_owned_surface --test control_lifetime --test control_carrier_runtime --test control_primitive_values` | all passed |
| `cargo test -p psrs-driver --lib -- --skip tests::wasi` | 655 passed, 31 failed |
| `cargo fmt --all --check` | clean |
| `cargo clippy -p psrs-backend --all-targets`, `-p psrs-driver --lib --tests` | clean except the pre-existing `psrs-kind` warning |

The 31 driver-library failures are the **same failure set as before this
continuation** (pre-existing Effect-migration errors: `unknown name runEffect` /
`unknown type name Effect`) and are not regressions. The wasi target tests also
fail for that reason. Full workspace runs, ignored scoreboards, and upstream
`purs` differentials were intentionally **not** rerun, per the user's
focused-validation instruction.

## 6. Remaining work and recommended continuation

Historical (pre-review). The current remaining work is stated in **§16**;
this section is retained for the acceptance-audit context it records.

Follow the user's order and validation limits: work in the current checkout,
keep changes focused, and do not run global validation after each small fix.

1. **§6.2 continued — complete the acceptance audit beyond R1–R3.**
   The review defects have permanent rejection tests and shared contracts; do
   not restart these repaired slices. Audit remaining dispatch/context/replay
   equivalences, requirement-level coverage and general cost obligations. Keep
   checked source typing, ordinary SSA/layout legality, machine execution proofs
   and source-to-Wasm observations separate. No-write summaries are insufficient
   for unobservability; observations that deliberately trap belong after the
   verified compiler pipeline. Scope and exact evidence are in the repair record.
2. **§6.3 foreign capture and trap cleanup.** Foreign borrowed-buffer/resource
   capture validity and explicit unsupported-capture rejection; a defined
   caught-host-trap recovery/cleanup boundary with actual recovery execution; and
   general allocation/cost obligations. Do not narrow the normative design.
3. **§6.4 reference-library/API acceptance.** Complete pinned upstream-library
   source and execution comparisons for the generic handler model, including the
   remaining row/representation interactions. Plain import acceptance is not
   evidence that Aff or other FFI executes.
4. **§6.5 integrated audit.** Audit every AE/AC requirement, run its full required
   validation scope, and remeasure the relevant D-04 gates before changing
   statuses. Land any design-document change with the code it describes.
5. **§6.6 native Effect/ST.** Only after complete delimited-control acceptance,
   implement role-correct public native Effect/ST facades, shared ST.Global
   conversion/representation, region boundaries and raw host leaves.

Suggested commit grouping when ready: group dispatch/CFG/tag removal with its
rejection tests and design contract; execution/owned/lifecycle verification with
its tests and coverage documents; include the machine-document split with the
implementation topic it describes. Do not create a documentation-only follow-up
for a code topic or split one semantic proof across trivial commits.

## 7. Pre-repair validation and review findings (2026-10-14)

This is the historical failing snapshot. Repairs and current evidence follow in §8.

Checkout identity is unchanged: compiler `6df8bfba`, locked stdlib `d6cf811`
(clean when inspected in this review). Wasmtime is 49.0.2 (`3c8a3e79a`).

| Scope | Current observed result |
| --- | --- |
| `cargo test -p psrs-backend --lib` | 444 passed |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::control` | 173 passed, 2 failed; both fixture lookups panic before malformed-IR assertions |
| Eleven control integration targets with required Wasmtime | 81 passed, no skips |
| Temporary architecture probes | Identity Check/Close bodies accepted by both CC and MIR; extra Store and always-true TrapIf accepted by MIR |
| Formatting, diff whitespace and local document link targets | Passed |

The two failing tests are
`tests::control_owned_steps::mir_private_cells_are_checked_at_publication`
(line 167) and
`tests::control_owned_steps::mir_cancellation_requires_pending_cancelled_and_clear`
(line 320). Both use a source that leaves the relevant helper absent from the
optimized module. This is an incomplete negative-test fixture, not evidence that
production lowering failed or that its intended malformed mutation was rejected.

The probes establish verifier incompleteness, not a source-level user program
that currently triggers those defects. Production helpers still emit the
intended checks, close writes and callback ordering. These findings nevertheless
block any claim that later transformations are checked against the complete
machine contract. The complete requirements remain Unverified.

Logs and probe source: `/private/tmp/psrs-control-architecture-review-2026-10-14/`
(`backend.log`, `control.log`, `integration.log`, `probes-all.log`, `probes.rs`).
The review reruns focused checks only. Workspace tests, workspace Clippy,
ignored scoreboards, the full upstream differential, source audit and package
conformance were not rerun; earlier evidence remains historical. No Rust changes
or permanent tests were added in this documentation review.

## 8. Current repair checkpoint (2026-10-14)

R1–R3 are repaired and included with the earlier draft in the local checkpoint
commit. The stdlib lock remains unchanged.
Only test dependencies were added to the driver (`wasm-encoder` and `wasmparser`,
using existing workspace pins); no library or runtime package was changed.

- `control/lifecycle.rs` and paired adapters prove Check/Close bodies, including
  own status access, live guard, checked parent advancement and close writes.
- `control/events.rs` owns allocation/read/write/call/refinement/trap observations.
  Full Executor/lifecycle/factory paths include entry prefixes and failed guards.
  Rejection terminates before further observable work, and its executed prefix
  must agree with a checked normal action. Narrowing casts need an explicit
  static/case/callback-use justification. Checked source typing and ordinary
  SSA/layout legality remain the producer assumptions.
- Total-function summaries replace the unsafe no-write exemption; recursive
  calls, fallible reads/traps and MIR CFG cycles are not assumed unobservable.
- Retained Start/Once/Cancel mutation tests now use pre-optimization MIR.
  Optimized physical Start/Once cells have separate publication-witness tests.
- Ancestry cost instrumentation runs on the final Wasm/component after compiler
  verification, with private observer functions and explicit component validation.
  The original long-chain/GC and depth-sensitive observations still execute.
- CC execution verification is organized under `cc/verify/control/execution/`;
  queue MIR mutations are under `control_queue_steps/mir/`. The touched sources
  satisfy the 500-line limit and no longer add the flat execution-sibling layout
  violation. Other pre-existing repository layout violations remain outside this
  repair.

| Final focused scope | Observed result |
| --- | --- |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::control` | 185 passed; no failures or skips |
| Eleven control integration targets, required Wasmtime | 81 passed; no failures or skips |
| `cargo test -p psrs-backend --lib` (also in final workspace) | 444 passed |
| Post-module-layout `control_observable_paths` | 4 passed |
| `cargo fmt --all --check`, `git diff --check`, local document links | Passed |
| `cargo clippy --workspace --all-targets` | Completes; only the pre-existing `psrs-kind/src/check/synonyms.rs:174` `let_and_return` warning |
| `cargo clippy --workspace --all-targets -- -D warnings` | Fails on that existing warning |
| `cargo clippy --workspace --all-targets -- -D warnings -A clippy::let_and_return` | Passes; no other lint allowance |

Logs: `/private/tmp/psrs-control-repairs-2026-10-14/`, particularly
`control-canonical-final.log`, `integration-verified.log`, `backend-verified.log`,
`layout-focused.log`, `clippy-normal-final.log`, `clippy-strict.log` and
`clippy-canonical-final.log`. `repair.diff` compares the Rust changes with `before/`,
so the new repair can be distinguished from the earlier uncommitted work.

No ignored scoreboard, full upstream differential or package conformance rerun
was performed for this verifier repair. D-04 measurements and complete AE/AC
statuses are unchanged. Clone/Rebuild/context step equivalence beyond existing
operand/capture proofs, foreign capture, caught-host-trap recovery and complete
reference-library acceptance remain open.


### Integrated comparison and final-snapshot scope

`cargo test --workspace --no-fail-fast` completed with the **same 71 failed test
names** as `/private/tmp/psrs-control-reference/workspace-after-failing-names.txt`:
no added or recovered names. The failing test targets are CLI `link`, CLI
`source_layout`, driver library (**810 passed / 68 failed**) and driver
`residual_constraints`. They retain the known Effect/FFI migration, constraint
and repository-layout failures. This is a name-set comparison with the retained
historical baseline, not a fresh clean-checkout before/after workspace run.

That workspace run also reported E0463 dependency-artifact errors for backend
and driver rustdoc. Standalone
`cargo test -p psrs-backend -p psrs-driver --doc` then completed successfully
(zero doctest examples in either crate); this is separate build evidence rather
than two extra failing test names.

`workspace-final.log`, `workspace-comparison.json` and `doctest-final.log` retain
the exact observations. The earlier `workspace.log` is an intermediate snapshot:
it included a rejected valid switch join that was subsequently repaired and
permanently rechecked. Use `workspace-final.log` for the comparison.

The final CC module-directory cleanup and path-operand canonicalization came
after the integrated snapshot. Focused control **185/185**, backend **444/444**,
mandatory integration **81/81**, formatting and lint checks were rerun on the
final tree; full workspace was not repeated for those last changes. Event prefix
comparisons resolve observed input operands through each path's aliases while
retaining instruction result IDs, so a shared join cannot disguise another
cell/source or cast definition.

## 9. Lowering architecture review checkpoint (2026-10-14)

The implementation checkpoint is `1e24e5d5`. The subsequent documentation-only
review leaves Rust, the target profile and the locked package unchanged. Its
uncommitted diff comprises the lowering review, the machine design corrections,
the checklist link and this handoff update.

The review identifies early expansion followed by semantic reconstruction,
positional schema duplication, manually composed action coverage, and an
unresolved proof boundary around generic optimization. It recommends a temporary
checked P8 construction plan, with an independently checked expansion boundary
and explicit downstream trust accounting. This recommendation is not a selected
new pipeline and does not authorize removing existing semantic checks.

`control-machine.md` now distinguishes computation/queue/context relationships
and execution/dispatch/saved extent roles. Its replay pseudocode now pops the
traversal stack only when the current queue is empty, matching the code rather
than skipping a saved left queue. The review also separates sequencing
defunctionalization from arbitrary function conversion and replay cost from
persistence.

Fresh required-Wasmtime control baseline: **185 passed, 0 failed, 0 ignored**.
Log: `/private/tmp/psrs-control-lowering-review-2026-10-14-baseline.log`.
No new full workspace or acceptance measurement was made. The next action at the
time was to resolve the schema/action inventory and proof-discharge decision,
then implement the Bind/Return/Cons vertical slice under the review's migration
gates. **§10 records that resolution: the schema, action inventory and checked
plan are implemented and the recognizers retired.** Existing R1–R3 repairs and
all remaining AE/AC obligations remain in force.

## 10. Construction-boundary migration (2026-10-14)

[DEC-21](../../decision/DEC-21-control-machine-construction-boundary.md) now
fixes the construction/verification boundary: P8 checks a named construction
plan and expands it to ordinary CC; ordinary P9/P10 transformations are trusted
under IR-invariant and behavior evidence.

The migration's first stages are implemented. `crates/psrs-backend/src/control/schema.rs`
names every machine carrier, case, field and helper role, and
`cc/control/schema.rs` freezes one module's placement (`RepresentationTable::control_schema`)
and checks it against those names. Layout, generation and verification consume
the names instead of positional offsets; the old role-from-signature inference
is removed. The executor's ad-hoc coverage set became a named `ActionId`
registry whose complete inventory must be observed.

The flat driver is now *built* from a checked transition program:
`cc/lower/control/plan/` describes each transition's selected cases,
projections, ordered operations and terminal outcome, checks dispatch coverage,
allocation arity and the action inventory, then expands it to ordinary CC.
`Machine` no longer carries implicit `context`/`boundary` state or a `recur`
shortcut; `Advance` names all six state components.

The helper bodies and the raw-resume closure adapter are now plan-built too:
`clone_sequence`, `rebuild_sequence`, `start`, `check`, `close`, `once`,
`cancel` and `resume` are constructed from checked plans in `plan/helpers.rs`.
Every machine body is therefore described once and expanded to ordinary CC. The
plan module is split by responsibility and each file stays under the 500-line
limit. An independent bounded model (`plan/model.rs`, test-gated) recovers the
plan's terminal trace for every case-refinement path — the executor and each
helper body — and compares it against a table written directly from the
operational semantics, so a dropped, duplicated or mis-routed transition fails
the comparison. Cost evidence keeps the dimensions separate: the extent ancestry
probe measures finish/ancestor depth, while a replay counter probe
(`probe::instrument_replay`, a private global incremented at each
`CloneSequence`/`RebuildSequence` entry) measures replay invocations.
`replay_invocations_grow_with_requests_at_fixed_ancestry` shows replay work grows
with requests while the ancestry bound stays at 2. The integration targets keep
the ordinary-path cases explicit: reference aliases, partial applications, and
top-level function wrappers used as callbacks through the handle protocol
(`control_scoped_runtime::callback_wrapper_and_adapter_run_through_ordinary_paths`).

The P8 **boundary checker** (`plan/boundary.rs`) reconstructs the emitted
function's machine dispatch/outcome shape and compares it with the checked
plan's shape when each machine body is constructed. The plan is hand-built from
the semantics and the emitter is a generic expander, so an expander defect
(dropped arm, wrong tag, wrong terminal) fails the boundary instead of reaching
the generic CC verifiers; those still own operand semantics.

### Recognizer retirement (Gate 6)

Per the instruction to land the replacement wholesale, the CC and MIR machine
semantic recognizers are removed:

- `cc/verify/control/*` (dispatch, execution, root, clauses, checkpoints, replay,
  owned, lifecycle, initialization, exits, refinements, context_entry,
  finish_creation/consumption, steps) is deleted; `cc::verify_module` now calls
  `cc::lower::verify_boundary` on each function instead.
- `mir/control/*` machine recognizers are deleted; only the helper call binding
  contract remains and module-level MIR machine verification is a no-op.
- The shared `control/` relation layer (`dispatch`, `provenance`, `steps`,
  `owned`, `events`, `lifecycle` and the `execution` relations) is deleted. Only
  the named `schema` and the flat-driver `ActionId` inventory remain.

Replacement evidence: the named schema and checked construction plan own the
machine semantics ([DEC-21](../../decision/DEC-21-control-machine-construction-boundary.md));
the boundary checker re-checks each emitted body's dispatch/outcome against the
plan at `cc::verify_module`; and a MIR boundary checker (`mir/control/boundary.rs`)
re-checks each machine function at `mir::verify_module`: its dispatch arm set and
its non-self tail-call targets must match the plan. Self tail calls are loopified
into loop back-edges, so the plan's self targets are excluded and the check
compares only external helper `ReturnCall`s. Generic IR verification still
checks operands, types, layout, casts and CFG.
`control_carriers::cc_boundary_rejects_a_dropped_machine_dispatch_arm`,
`mir_boundary_rejects_a_dropped_machine_dispatch_arm` and
`mir_boundary_rejects_a_changed_external_tail_target` show the replacements
reject a dropped machine arm and a changed tail target. The backend, focused control, and eleven
integration runtime targets pass.

Assurance tradeoff, stated plainly: the removed recognizers also rejected
arbitrary mutations injected after construction (wrong field, aliased source,
extra store or trap). With them gone, such IR-valid mutations are no longer
rejected, and the negative driver tests that exercised them were removed
together with their modules. This is the "trust semantics-preserving transforms"
branch of DEC-21 taken to its end: the construction boundary, the CC and MIR
dispatch-arm boundary checks, and generic IR validity are the assurance. The
boundary checks cover dispatch coverage, not operand semantics; a translation
validator proving every optimizer intermediate preserves machine semantics
remains open, as do full AE/AC, foreign-boundary and reference-library
obligations.

Validation performed: `cargo fmt --all --check`; `cargo clippy -p psrs-backend
--all-targets`; `cargo test -p psrs-backend --lib` (454 passed); the eleven
control integration targets (value/order/lifetime/GC/stack, required Wasmtime,
82 passed); and
`PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::control`
(10 passed, 0 failed). `cargo test --workspace` has only the pre-existing,
unrelated `psrs-cli --test link` failure (`P3 resolve: unknown name runEffect`).

### Next actions after the construction migration (historical)

The review's construction and enumeration work is landed. The remaining
assurance gap is operand-level and cross-optimization validation:

1. **Operand-level CC boundary check (implemented in §11).** `plan/boundary.rs`
   compares only the dispatch/outcome shape. Extend it to replay the plan's
   `Op` sequence and compare projections/allocations/calls against the emitted
   CC destinations, so an IR-valid operand mutation injected after construction
   is rejected again. Start from `plan/ir.rs` (`Op`, `Operand`, `Cursor`),
   `plan/expand.rs` (the emitter to correlate against) and `plan/boundary.rs`.
   Add positive coverage (the 454 backend and 82 integration tests) and a
   negative that mutates a projected field/index and expects rejection.
2. **MIR operand-level check.** `mir/control/boundary.rs` compares dispatch arms
   and external tail-call targets; a field/index mutation at MIR is not caught.
   If (1) lands, extend the same idea over the physical layout
   (`mir/layout`, `mir/value_identity`).
3. **Translation validator.** No pass-level proof or transformation-aware
   certificate establishes that optimization preserves machine semantics;
   `mir/opt` re-verifies each intermediate with the dispatch-only boundary plus
   generic IR validity. If a stronger claim is required, amend
   [DEC-21](../../decision/DEC-21-control-machine-construction-boundary.md) and
   record the trust boundary.
4. **Acceptance obligations are unchanged.** Full AE/AC, foreign-boundary and
   reference-library requirements remain open; this migration closes none of
   them.

Do not treat the removed negative driver tests as covered: they were deleted
with their recognizer modules. They remain recoverable for re-expression, e.g.
`git show 01f7e66c:crates/psrs-driver/src/tests/` (or the commit before the
retirement). Re-express them against the operand-level boundary if it is added.

## 11. Operand-level CC construction boundary (2026-10-15)

Starting point: `708d65a0dcd3cee840c3426f04f7386780fdb6e4`, branch
`feat/effect-handlers`, clean checkout. The locked stdlib is unchanged. This
continuation implements §10's first next action under DEC-21; it does not restore
the retired recognizers or claim an optimizer equivalence proof.

`cc/lower/control/plan/boundary/` now independently walks the plan and each
emitted CC assignment sequence. Each `Op` binds its actual destination into the
plan environment. Projections check representation, case, index and source;
allocations check their actual fields; curried calls check every callee and
argument; helper calls check the role's symbol and contract; captures check
slots and planned carrier shape. Guards, reads, status writes, callback clears,
dispatch selectors, trapping defaults and actual returned values are checked in
execution order. Extra or missing operations fail even when unused.

Reference casts preserve identity and generic CC verification checks their
legality. Erasure and same-shape aliases are total. A recovery cast must directly
feed the next checked operation or actual exit: unused recovery, re-erasure of
unused recovery, and moving callback recovery before its callback-clear write
are rejected. This prevents treating a potentially trapping cast as invisible
work. The check runs both immediately after construction and at ordinary CC
module verification, and does not call the emitter.

The source fixture reaches P8 successfully. Before the change,
`PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::control` passes
**10** tests. Running the first ten new negative tests against the original
shape-only boundary reproduces **eight accepted mutation categories**: clause
and product indices, helper and allocation arguments, status writes,
missing close stores, extra stores and unused recovery casts. The returned-value
mutation was already rejected by the shape checker; conflicting capture-slot
types were already rejected by generic CC verification. The restored new
boundary rejects the eight gaps. Additional tests cover selector identity,
clause arguments, recovery ordering, and a legal reference alias through the
full backend. `tests::control_boundary` contains fourteen tests; one test each
covers both status-write mutations and both unused-recovery forms.

| Validation | Observed result |
| --- | --- |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::control` | 24 passed, no failures or skips |
| The eleven control integration targets below, required Wasmtime | 82 passed, no failures or skips |
| `cargo test -p psrs-backend --lib` | 454 passed |
| `cargo fmt --all --check`; `git diff --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Blocked by the existing `psrs-kind/src/check/synonyms.rs:174` `let_and_return` warning |
| `cargo clippy --workspace --all-targets -- -D warnings -A clippy::let_and_return` | Passed |

Runtime command:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver \
  --test control_surface --test control_scoped_surface \
  --test control_owned_surface --test control_primitive_values \
  --test control_runtime --test control_open_runtime \
  --test control_scoped_runtime --test control_scoped_replay \
  --test control_lifetime --test control_owned_runtime \
  --test control_carrier_runtime
```

`cargo test --workspace --no-fail-fast` completes with **71 failed test names**,
identical to the retained 2026-10-14 `workspace-final.log`: no added or recovered
names. Targets are CLI `link`, CLI `source_layout`, driver library (645 passed /
68 failed) and driver `residual_constraints`. The source-layout findings are in
unchanged files. This is a historical name-set comparison, not a fresh
clean-checkout before/after measurement. Backend/driver rustdoc also reports
E0463 dependency-artifact errors in that run. Standalone
`cargo test -p psrs-backend -p psrs-driver --doc` passes (zero doctest examples
in either crate); this is separate build evidence.

The integrated workspace snapshot precedes the final capture-shape check,
recovery-order enforcement and four added tests. The final focused 24-test run,
backend run, eleven integration targets, formatting and lint checks cover the
final implementation. The complete workspace was not repeated for those last
changes. No ignored scoreboard, full upstream differential or package
conformance rerun was performed; D-04 measurements and complete AE/AC statuses
are unchanged.

Evidence: `/private/tmp/psrs-control-operand-{baseline,before-negative,negative,focused-final,integration-final,backend-final,workspace,clippy,clippy-final,fmt,doctest}.log`
and `/private/tmp/psrs-control-operand-workspace-comparison.json`. The
`before-negative` run temporarily restores the original boundary and then
restores the new implementation; it is deliberately failing reproduction
evidence, not a current test failure.

Current next actions:

1. **MIR operand-level validation.** `mir/control/boundary.rs` still checks only
   dispatch arms and external tail targets. CC rejection tests do not prove that
   physical MIR field/argument mutations are rejected. Preserve DEC-21's
   explicit P9/P10 trust boundary while choosing a transformation-aware check;
   do not rebuild a second ad-hoc machine recognizer.
2. **Optimizer preservation.** No translation validator establishes equivalence
   across generic optimization. Retain actual optimized runtime evidence and
   keep this assurance limit explicit.
3. **Complete acceptance.** Continue the full AE/AC audit, foreign resource
   capture, caught-host-trap cleanup and pinned reference-library evidence.
   Implement native Effect/ST only after complete delimited-control acceptance.

Changes remain uncommitted. No pushes, PRs or remote changes were made.

## 12. Complete plan coverage and immutable planning (2026-10-15)

The active objective includes both 2026-10-14 architecture and lowering reviews,
and complete delimited-control acceptance. §11 is one completed construction
slice, not the end of that objective. The [lowering review audit](delimited-control-lowering-review-2026-10-14.md#implementation-audit-2026-10-15)
and [architecture proof-owner audit](delimited-control-architecture-review-2026-10-14.md#current-proof-owners-2026-10-15)
now distinguish historical recognizer evidence from current owners and open work.

### Corrected construction contracts

The previous inventory covered only eight root/queue actions despite describing
itself as complete. `control/execution/` now enumerates **39** independent named
selection paths and terminal roles: Executor 20, Clone 7, Rebuild 4, Check 2,
Close 2, Start/Once/Cancel/raw resume one each. Plan checking rejects missing,
duplicate, extra and wrong-terminal actions. Every dispatch covers its complete
family; Rebuild's Append arm explicitly rejects. Unknown physical tags still
reach trapping defaults. A permanent negative test removes each of the 39 actions.

Before a plan can become a private immutable `CheckedPlan`, the checker validates
logical operand roles, parameter and temporary bounds, branch-local scope,
source-specific case refinement, field/index domains, allocation/call arity,
capture protocol and lifecycle status domains. Only a `CheckedPlan` can enter
the emitter. Tests reach the intended rejection owner; the arity test mutates a
complete valid Once plan rather than failing an unrelated inventory check first.

`control/schema.rs` owns representation-neutral field types and helper signatures.
Layout planning consumes those types, and module verification checks actual
fields, private array element types, result/replay products and helper signatures
against them. Equal arity alone no longer establishes schema agreement.

`FrozenControlPlan` privately owns the helper/source-contract bindings, quoted
source identities, resumption and schema placement. Consumers use read-only
queries and named role lookup. Publication validates before freezing, rejects
duplicate roles/callable identities and repeated publication, and rolls back a
failed candidate. Ordinary mutable layout fixtures use
`RepresentationTable::from_parts`; ordinary instructions remain mutable and
subject to validation. Four permanent tests cover publication, retry/rollback,
completeness/uniqueness and same-arity layout/signature mutations.

The machine emitter no longer owns a `FunctionLowerer`: it has an immutable
representation table, its own value declarations, planned layout, helper symbols
and source span. `ControlEmitter` shares checked reference adaptation with source
factories. Source environments and mutable planning are not available to helper
emission. Symbol planning and named queries live in `control/planning.rs`.

### Restored review regressions and remaining model gap

`tests::control_regressions` re-expresses the historical CC lifecycle test and
all four CC observable-path tests at the P8 construction boundary: eight
Check/Close mutations; prefix/default extra calls; extra publication checks;
unused recovery; and a potentially diverging no-write recursive call. The five
tests lower live sources, perform their mutations and reach rejection assertions;
none skips a missing function. Historical diagnostic names are replaced with
the current P8 owner.

An inventory of all 185 test names at `01f7e66c` initially found 179 missing names,
including 77 MIR and 102 CC/source names. Five CC tests are now restored. These
are name comparisons, not semantic coverage measurements: renamed replacements
can cover old cases, and retained names alone do not prove equivalent coverage.
The exact remaining mapping is an acceptance obligation, not closed by counts.
Inventory: `/private/tmp/psrs-control-retired-test-inventory.json`.

At this checkpoint (superseded by §13), `plan/model.rs` compared only selected case paths and terminal
classes. It does not evaluate operands, persistent queue order, replay freshness
or ancestry. The new typed plan checks do not turn it into a behavioral model.
Implement the independent bounded behavioral model required by L4/gate 5 next.

### Validation and continuation

| Final validation | Observed result |
| --- | --- |
| `cargo test -p psrs-backend --lib` | 468 passed |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::control` | 29 passed, no failures or skips |
| Eleven control integration targets, §11's command, required Wasmtime | 82 passed, no failures or skips |
| `cargo fmt --all --check`; `git diff --check`; local documentation link targets | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Fails only on the existing `psrs-kind/src/check/synonyms.rs:174` `let_and_return` warning |
| `cargo clippy --workspace --all-targets -- -D warnings -A clippy::let_and_return` | Passed |
| `cargo test --workspace --no-fail-fast` | 71 failed test names, identical to the retained 2026-10-14 name set; no additions/recoveries |

The final workspace library result is **654 passed / 68 failed** for the driver.
The other failing test targets remain CLI `link`, CLI `source_layout` and driver
`residual_constraints`. As before, this compares names against historical
evidence, not a fresh clean-checkout baseline. Backend/driver rustdoc reports
E0463 dependency-artifact errors in that integrated run; standalone
`cargo test -p psrs-backend -p psrs-driver --doc` is recorded separately.
The final rollback test's additional nonempty source-identity assertions were
checked by a separate four-test publication run after the workspace build;
production code did not change after that workspace snapshot.

Evidence: `/private/tmp/psrs-control-complete-{workspace-final,control-final,clippy-strict,clippy-final,fmt,doctest}.log`,
`/private/tmp/psrs-control-complete-workspace-comparison.json`,
`/private/tmp/psrs-control-frozen-{backend-final,integration-final,publication-final}.log`
and `/private/tmp/psrs-control-restored-review-regressions.log`. The focused
construction baseline was nine passing plan tests at this turn's starting tree,
recorded in `/private/tmp/psrs-control-complete-plan-baseline.log`. No ignored
scoreboard, full upstream differential or package conformance run was performed;
D-04 measurements and complete AE/AC statuses are unchanged. Source and lock
identities remain those of §11; changes remain uncommitted. No remote operations
or standard-library edits were performed.

The next actions at this checkpoint were: (1) independent bounded operational model; (2) complete retired
case-to-owner mapping and corresponding construction/transform regressions;
(3) queue-shape/replay-length cost evidence; (4) foreign lifetime and caught-trap
cleanup/reentry contracts and actual execution; (5) pinned library and complete
AE/AC integrated acceptance. DEC-21 already selects trusted ordinary transforms;
do not silently replace that decision with a second ad-hoc MIR recognizer or
claim that generic SSA/layout validity proves optimizer equivalence. A stronger
translation validator remains an explicit separate assurance choice.

Design input for the remaining trap boundary: the Component Model's
[component invariants](https://github.com/WebAssembly/component-model/blob/main/design/mvp/Explainer.md#component-invariants)
define lockdown after a trap, preventing observation through reentry into that
component instance. Recovery design and execution must account for this target
rule; do not assume an embedder can catch a canonical-ABI trap and invoke a guest
cleanup export on the same instance. This records a required constraint, not a
completed cleanup/recovery implementation or a reduction of the acceptance scope.

## 13. Independent bounded behavioral model (2026-10-15)

This continuation preserves the dirty checkout from §12 and implements the
bounded operational-model requirement in lowering-review L4/migration gate 5.
It adds test/model evidence; production lowering, machine representation and
the locked package are unchanged by this slice. Complete goal acceptance remains
open across both reviews and the AE/AC checklist.

### Independent execution and comparison

`cc/lower/control/plan/model/` replaces the former shape-only model file. The
old terminal tables remain as explicitly named structural tests. New components:

- `interpret.rs` and `operations.rs` execute the **actual** plans, resolving
  parameters, temporary values, selected cases, projections, calls, allocations
  and state transitions on a concrete heap. Self helper tails are trampolined.
- `reference/` directly matches typed computation/queue/context/extent/owner
  values. It does not inspect a plan, Op, Cursor, emitter or production field
  offset constants. Replay specifies a flat list of callback/Finish occurrences
  rather than constructing the plan's traversal stack and reversed nodes.
- `compare.rs` checks returned/published/advanced results, all original object
  contents, original value identity, fresh-reference aliasing and ordered status
  reads/writes, callback loads/clears and applications. Fresh IDs are compared
  bijectively, not by structural equality that could hide shared/reused cells.
- Allocation roles must agree. The flat replay model independently budgets the
  temporary queue nodes it does not construct; extra private or queue allocations
  cannot silently disappear behind fresh-ID normalization. Invalid model inputs
  and exhausted evaluation budgets never count as matching executions.

All **39** named actions are executed and compared, including rejection arms.
The finite tests cover both contexts, all queue heads, target completion and
Finish coalescing, Bind/Resume provenance, pending/consumed/cancelled owners,
callback traps, raw invocation, ancestor expiry and unknown tags. Distinct values
represent root state, current execution extent, dispatch checkpoint and saved
restore parent, so same-shaped substitutions remain observable.

The queue generator enumerates **1,805 expressions through depth 3**, with two
shared callback identities, Finish markers and independent Append subtrees.
Each is checked for successful replay, repeat replay with disjoint fresh
activations, unchanged source objects and rejection when a saved Finish expires.
Callback order/sharing, unique fresh extents, active result and fresh-parent chain
are asserted independently of graph comparison. Exact modeled queue allocation
count is `2 * (Cons + Finish occurrences) + Append occurrences`.

Additional composition tests drive left/right Bind chains through depth 15 to
publication; compose bounded Resume with both dispatch contexts and a distinct
saved checkpoint; abandon source work while completing scoped target/caller
work; and replay an actually shared subqueue containing an owned callback twice.
Both occurrences retain the same owner: its first transfer consumes it and the
second is rejected. Distinct later expired Finish extents/ancestors are checked
before rebuilding, and malformed/unknown reversed tails retain the valid prefix
allocation while never publishing a replay result.

Eighteen semantic mutation tests exercise **20 concrete mutations**. Every
mutated plan still passes logical type/scope/action/terminal checking before
behavioral comparison rejects it. Cases include swapped clauses or state fields,
wrong callback arguments, wrong Finish restore/Handle caller extent, Bind
reentering itself, lost Append-left work, consumer-first Resume, wrong replay
parent or reused checkpoint, wrong replay result, incorrect one-shot phases or
callback clearing, cancelled/consumed confusion, reopening expired extents,
omitted raw checks, and extra status/extent/queue allocations. This demonstrates
behavioral evidence beyond shape checking rather than simply adding positives.

Scope is explicit: scripted callback results and raw-resume closures are model
inputs, not an interpreter for arbitrary PureScript functions. The model does
not prove optimizer equivalence, native GC/stack behavior, FFI ownership, host
cleanup or behavior under allocation failure. Mandatory optimized target tests
and the original complete acceptance obligations remain required.

### Borrowed-result boundary audit

The audit initially tested whether nested borrowed results bypassed the ABI's
top-level guard. Actual `wit_parser::Resolve::push_str` rejects them recursively
before import publication. A probe that unwraps that failed resolution is not
evidence of malformed import acceptance. No duplicate borrow checker or compiler
ownership type was added.

`abi::tests::borrowed_results` now tests the actual boundary: **25** nested WIT
result forms must reach the borrowed-result diagnostic (not a syntax error),
six legal borrowed parameter forms retain their canonical ownership metadata,
and six owned result forms remain supported with callable explicit drops.
These are rejection/metadata claims, not foreign resource execution or complete
capture-lifetime acceptance. DEC-14's library-owned explicit-drop discipline and
the remaining foreign/trap obligations are unchanged.

### Evidence and next work

The construction baseline passed 19 plan tests on the starting dirty tree.
The completed model passes 34 tests, including the two retained terminal-shape
tests. Three additional ABI tests cover the borrowed-result audit.

| Final validation | Observed result |
| --- | --- |
| Backend library, in final workspace run | 503 passed |
| `cargo test -p psrs-backend --lib cc::lower::control::plan::model` | 34 passed, no failures; `behavior-prefixes.log` |
| `cargo test -p psrs-backend --lib abi::tests::borrowed_results` | 3 passed, no failures |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::control` | 29 passed, no failures or skips |
| Eleven control integration targets, §11's command, required Wasmtime | 82 passed, no failures or skips |
| `cargo test --workspace --no-fail-fast` | Same 71 failed test names as §12; no additions or recoveries |
| `cargo clippy --workspace --all-targets -- -D warnings` | Blocked by the existing `psrs-kind:174` `let_and_return` warning |
| `cargo clippy --workspace --all-targets -- -D warnings -A clippy::let_and_return` | Passed |
| `cargo fmt --all --check`; `git diff --check`; local documentation link targets | Passed |

The final integrated driver library result remains **654 passed / 68 failed**.
The failing test targets remain CLI `link`, CLI `source_layout`, driver library
and driver `residual_constraints`. This compares against the immediately
preceding goal-turn final workspace log, not a fresh clean-checkout baseline.
Rustdoc passes in this final integrated run; it does not reproduce the earlier
dependency-artifact errors. Standalone backend/driver doctests also pass (zero
examples). The earlier intermediate workspace run is not the final snapshot.

Evidence: `/private/tmp/psrs-control-behavior-{baseline,prefixes,control,integration,workspace-final,clippy-strict,clippy-final,fmt,doctest}.log`,
`/private/tmp/psrs-control-behavior-workspace-comparison.json` and
`/private/tmp/psrs-control-borrow-result-final.log`. The temporary
`borrow-result-before.log` is retained as audit evidence of a wrong probe
boundary, not a before/after compiler-fix comparison. No ignored scoreboard,
full upstream differential or package conformance rerun was performed; D-04
measurements and complete AE/AC statuses are unchanged. All final Rust changes
are included in the final workspace/lint snapshot.

Continue with the full retired-case mapping and construction/transform
regressions, actual saved-length/queue-shape traversal cost observations,
foreign capture and caught-trap cleanup/reentry, pinned library comparisons and
complete AE/AC acceptance. `control_costs::probe::instrument_replay` currently
increments outside the loopified helper bodies; its entry count does not measure
per-node replay work. The model's allocation formula is not a substitute for the
required optimized-Wasm observation. Native Effect/ST retains the original
post-delimited-control order.

No commits, pushes, PRs, package edits or remote changes were made.

## 14. Optimized Wasm replay traversal costs (2026-10-15)

This slice closes the saved-length/queue-shape observation gap in §13 and L5.
The dirty checkout and all earlier changes are preserved. It does not close
full delimited-control acceptance or the retired-test mapping obligation.

### Observation boundary and functional checks

`control_costs::probe::instrument_replay_steps` instruments the final structured
Wasm after the normal verified compiler pipeline. A private counter increments
at each Clone/Rebuild loop header, including self-tail backedges. The probe
requires both named helpers and exactly one loop per helper; changed target
structure fails the test rather than silently skipping an observation. The
existing entry counter remains separate. An optional Clone activation ordinal
isolates the second replay while preserving its Clone-to-Rebuild transfer.

Each source checks request count and the noncommutative callback recurrence
`value := 3 * value + callbackIndex`, including its final value. The run result
is passed once into a checking function: separate references to a top-level pure
`result` would evaluate the entire run twice and confound replay counts. An
independent allocation-site ancestry guard requires depth zero in these sources.
The ancestry-only artifact must first return 42.

`control_costs/replay.rs` compiles each workload once, validates instrumented
components, measures the smallest successful counter budget C, then reruns C
(exit 42) and C−1 (unreachable trap). Unexpected exits or other trap kinds fail;
they cannot become a successful count or runtime-unavailable skip. Each workload
retains source, original and instrumented Wasm, stderr, Wasmtime version, source
and artifact fingerprints, ancestry bound and threshold reports under the
printed `psrs-control-replay-costs-<pid>` temporary directory.

### Measured traversal

All measurements below use the actual optimized Wasm, required Wasmtime,
source-level result/order/request checks, and ancestry bound zero.

| Saved callbacks N | One-request helper entries | One-request loop iterations |
| --- | --- | --- |
| 0 | 2 | 2 |
| 1 | 2 | 4 |
| 4 | 2 | 10 |
| 16 | 2 | 34 |

The observed linear queue cost is `2*N + 2`: Clone and Rebuild each visit the
callbacks and their terminating Empty. Helper entries alone cannot detect it.

| Callbacks saved at second request | Linear second-replay iterations | Append second-replay iterations |
| --- | --- | --- |
| 0 | 2 | 4 |
| 4 | 10 | 12 |
| 16 | 34 | 36 |

Both shape workloads execute exactly two requests and four helper entries.
The first completes before building the second queue in the linear workload.
The Append workload performs again while consuming the first replay, so its
second saved queue is `Append(Cons^N, Empty)`. Its Clone traversal adds the
Append dispatch and the empty right traversal; callback count/order and extent
depth remain equal. This is observed traversal cost, not timing, native stack
measurement, actual GC allocation counting or a general complexity proof. The
model's separate allocation formula remains model evidence.

### Validation and continuation

| Final validation | Observed result |
| --- | --- |
| Backend library, final workspace run | 503 passed |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::control -- --nocapture` | 31 passed, no failures or skips, including both new traversal tests |
| Eleven control integration targets, §11 command, required Wasmtime | 82 passed, no failures or skips |
| `cargo test --workspace --no-fail-fast` | Same 71 failed test names as §13; no additions or recoveries |
| `cargo clippy --workspace --all-targets -- -D warnings` | Existing `psrs-kind/src/check/synonyms.rs:174` `let_and_return` warning blocks it |
| `cargo clippy --workspace --all-targets -- -D warnings -A clippy::let_and_return` | Passed |
| `cargo fmt --all --check`; `git diff --check`; changed documentation local link targets | Passed |

The final integrated driver library result is **656 passed / 68 failed**.
The failing test targets remain CLI `link`, CLI `source_layout`, driver library
and driver `residual_constraints`. This compares the failure-name set against
§13's final dirty-tree snapshot, not a fresh clean-checkout baseline. The same
integrated run also reports E0463 dependency-artifact errors for backend and
driver rustdoc. Standalone `cargo test -p psrs-backend -p psrs-driver --doc`
passes afterward (zero examples each); it is separate evidence.

Evidence: `/private/tmp/psrs-control-traversal-{baseline,control-final,integration,workspace-final,clippy-strict,clippy-final,doctest}.log`
and `/private/tmp/psrs-control-traversal-workspace-comparison.json`. The final
focused runtime artifacts are under
`/var/folders/f2/h3z3zsy51xq3kr06707r9ct40000gn/T/psrs-control-replay-costs-33066/`.
Runtime identity is Wasmtime 49.0.3 (`f96eabaa1`, 2026-10-14). For example, the
16-callback Append source fingerprint is `fnv1a64-v1:564d03a2e95e03e6`, its
original component is `fnv1a64-v1:1195130ca0218b27`, and the second-replay
threshold report records 36 successful iterations versus a trap at budget 35.
The initial failing probe run retained in `/private/tmp/psrs-control-traversal-first.log` exposed duplicate
evaluation in the test source; `/private/tmp/psrs-control-traversal-second.log` verifies the corrected
single-evaluation sources. It was not a compiler regression.

All final Rust changes are included in the integrated snapshot and final focused
runtime/lint checks. No ignored scoreboard or standalone package conformance
run was performed. D-04 measurements, stdlib lock and complete AE/AC statuses
are unchanged.

The next actions at the end of this slice were the full retired-case mapping,
foreign capture and caught-trap cleanup/reentry, pinned library comparisons and
complete AE/AC acceptance. §15 records the following slice: `Run` became the
40th checked action, and the 24 historical Root tests received an owner mapping.
Native Effect/ST retains the original acceptance order. No commits, pushes,
package edits or remote changes were made for the traversal slice.

## 15. Run construction and the Root regression slice (2026-10-15)

This section is history. Current progress is **§16**. It reconciles the dirty tree of that day with the
[architecture proof-owner audit](delimited-control-architecture-review-2026-10-14.md#current-proof-owners-2026-10-15)
and the
[lowering implementation audit](delimited-control-lowering-review-2026-10-14.md#implementation-audit-2026-10-15).
Both audits already describe the 40-action registry, the bounded model and the
Root mapping. §§12–13 still say 39 actions because they were written before
`Run` entered `ActionId::ALL`.

This documentation pass inspected the source. It did not re-run the §14
commands, so the 503/31/82/71 results and the Wasmtime 49.0.3 traversal numbers
remain measurements of the tree at the end of §14. They do not include the
`Run` plan or the three Root driver tests below.

### Checkout

| Owner | Current state |
| --- | --- |
| Compiler | `/Users/biu/Projects/purescript-rs`, branch `feat/effect-handlers`, HEAD `708d65a0dcd3cee840c3426f04f7386780fdb6e4` (`Build the control machine from checked plans and retire its recognizers`). The working tree is dirty and contains §§11–15. |
| Independent stdlib | `/Users/biu/Projects/psrs-stdlib`, branch `control-library`, HEAD `d6cf81105046e288e2e9cd9685f8cc47e960c988`; clean and unchanged |
| Work | Active continuation. Complete delimited-control acceptance remains open. |

Do not redo DEC-21 construction, the operand-level CC boundary, immutable
`FrozenControlPlan` publication, the bounded model, the §14 traversal probes,
or the R1–R3 CC regressions. Do not restore the retired `cc/verify/control/*`
and `mir/control/*` recognizers. Do not add a second MIR machine recognizer
under the current DEC-21 trust boundary.

### What the source now owns

`control/execution/mod.rs` defines `ActionId::ALL` with **40** paths:
Executor 20, Clone 7, Rebuild 4, Check 2, Close 2, and one each for Run,
Start, Once, Cancel and raw resume.
`plan/check/tests.rs::every_required_action_is_checked_including_helpers_and_rejection_arms`
drops each path and expects rejection. `ControlHelperRole::ALL` has nine roles;
`Run` takes payload, callable and computation and returns payload.

`cc/lower/control/plan/run.rs` builds the normal Run lifetime as one checked
plan: an empty queue, a fresh `Start`, a `Root` context, one `Executor` call
that passes that same root as both the execution extent and the dispatch
context, `Close` of that root after the executor returns, and return of that
executor result. The emitter checks the body against the plan before finishing
the function. The
[retired-regression mapping](delimited-control-retired-regressions.md) records
why this exists: source Run previously emitted those steps in the ordinary
caller and checked only the first three executor operands, so substituting
`Check` for `Close` passed P8 verification. The cited before-probe is
`/private/tmp/psrs-control-run-before-negative.log`. This pass did not
re-execute it.

The CC operand boundary remains `cc/lower/control/plan/boundary/`. It walks the
plan and the emitted assignments in order. The MIR boundary remains
`mir/control/boundary.rs`: dispatch-arm set plus non-self `ReturnCall` targets.
Physical field and argument mutations after P9 are still outside that check.
DEC-21 treats ordinary P9/P10 transforms as trusted once construction has
discharged the machine relation. Generic SSA and layout validity are not that
discharge.

`FrozenControlPlan` in `cc/control/plan/mod.rs` keeps helper bindings, source
contracts, resumption and schema placement private, rejects a repeated
publication, and rolls a failed candidate back. `Machine` in
`cc/lower/control/builder.rs` holds the immutable representation table, its own
value declarations, the planned layout, helper symbols and the source span.
`ControlEmitter` adapts references.

`cc/lower/control/plan/model/` executes the actual plans and compares them with
`reference/`, which matches typed machine values and replays a flat occurrence
list. `behavior.rs` still asserts **1,805** depth-three queue expressions and
includes `Program::Run` in the all-action comparison. The model directory
contains **39** `#[test]` functions. Twenty-two of them are semantic mutations
under `mutations/`: seven flow, seven replay and eight lifetime. The model still does not interpret arbitrary
source callbacks or prove optimizer, foreign-lifetime, host-trap or
allocation-failure behavior.

### Root mapping, source inventory

[delimited-control-retired-regressions.md](delimited-control-retired-regressions.md)
reviews **24** of the **185** test names at `01f7e66c`. The other **161** names,
including checkpoint, context, replay, private-cell publication and inlined
factory cases, still need a case-by-case owner. A deleted name is not lost
coverage and is not replacement coverage.

Present Root evidence, by the mapping's keys:

- **CC-root:** `control_regressions::root::cc_run_requires_its_fresh_root_empty_queue_ordered_close_and_result` mutates a live Run for `MissingClose`, `EarlyClose`, `WrongResult`, `ExecutionRoot`, `DispatchRoot`, `CloseRoot` and `NonemptyQueue`.
- **CC-close:** `cc_run_cannot_check_instead_of_closing_its_root`.
- **Model-run:** `plan::model::runs::run_composes_dispatch_callbacks_and_normal_root_completion`.
- **Model-negative:** the Run cleanup mutation above.
- **Source-runs:** `sequential_and_exclusive_runs_preserve_independent_invocations_after_optimization`.
- **Escape-runtime:** the existing `control_lifetime` integration target. This pass did not re-run it.

`tests::control` has **54** `#[test]` functions in source: 14 in
`control_boundary`, 6 in `control_carriers`, 6 in `control_costs`, and 28 in
`control_regressions`. §14 executed 31 driver control tests.
`cc/control/plan/tests.rs` still has four publication tests.
`abi/tests/borrowed_results.rs` still has three WIT tests. Those WIT tests
reject nested borrowed results and retain borrowed-parameter metadata. They do
not discharge foreign resource capture.

The five fixtures that were pending inside this family are now in
`control_regressions::transforms`, executed by
`cargo test -p psrs-driver --lib tests::control_regressions::transforms`
on 2026-10-15: **2 passed**, no failures. This run did not repeat the §14
backend, integration, workspace or Wasmtime traversal commands.

- `cc_start_rejects_a_status_cell_whose_length_is_not_one` rejects an empty
  Start status cell and a two-element cell at the P8 boundary.
- `pre_optimization_run_accepts_exclusive_sharing_distributed_close_and_copies`
  accepts, on pre-optimization MIR, one Start shared by two exclusive
  successors, Close repeated on both normal successors, Close omitted from a
  trapping successor, and Close applied to a copy of the root reference.

`control_runtime::collection_preserves_array_and_string_captures_across_suspension`
and `control_reference` (6 passed) plus the eleven integration targets
(**83 passed**) and `control-types.mjs` (**17/17**) close AE-01–AE-10 and
AC-01–AC-05. This turn did not remeasure D-04 or run `cargo test --workspace`.

Post-P9 injection of a wrong root or checkpoint operand is outside DEC-21's
promised rejection. Map it to construction plus transform behavior, or keep a
validator only if an explicit decision starts requiring rejection of arbitrary
modified MIR.

### Review obligations that remain open

| Review item | Current state |
| --- | --- |
| L1–L2, gates 1–3 | Checked plans, the 40-path inventory and ordered CC expansion checking are in the source. |
| L3 | DEC-21 stands. `mir/opt` re-verifies intermediates with the dispatch/tail boundary and generic IR validity. No translation validator exists. |
| L4, gate 5 model | Implemented, including Run. Bounded construction evidence only. |
| L5, gate 5 costs | §14's optimized Wasm table stands and was not remeasured here. Ancestry depth, helper entries, loop iterations and queue shape stay separate. |
| L6 | Immutable planning is in `FrozenControlPlan`. Host-handle capture in a control callback is rejected; nested canonical borrows are unsupported captures. |
| Gates 4 and 6 | Recognizers stay retired. All 185 historical names have an owner in the mapping audit. Exact MIR-only negatives stay outside DEC-21 where recorded. |
| R1 | Check/Close bodies are named helper plans plus the restored lifecycle CC mutations. |
| R2 | The P8 boundary accounts for ordered operations, failed guards and recovery casts. The four observable CC tests remain. |
| R3 | Live helpers are required by the inventory. The retired inlined Start/Once/Cancel publication negatives still need an explicit construction or transform owner. |
| AE/AC | AE-01–AE-10 and AC-01–AC-05 Verified; AE-11 Unverified. Official Effect/ST adapters and library-module `Host.lift` of Clock/Random/IO/Console/`Test.Assert` execute in `tests::native_representation` and `tests::assertions`. D-04 measurements unchanged. |
| Foreign and traps | P8 rejects opaque host-handle captures in control callbacks. Canonical nested `borrow<T>` is an unsupported capture. A trap aborts the instance; the artifact has no Control cleanup export. Complete AC-04 buffer-lifetime evidence remains open. |

### Next actions

Continued in **§16**. Keep per-IR machine recognizers retired. Leave DEC-21's
trust boundary in place. A pass-level translation validator requires an
amendment of that decision before implementation.

## 16. Native Effect/ST adapters and host leaves (2026-10-16)

This is the adapter snapshot preceding §17. Delimited-control construction
is the tree in §15. This slice owns AE-11 native host IO on that tree. AE-11
and every NE-* row stay **Unverified**. D-04 numbers stay unchanged until a
fresh scoreboard run. Keep per-IR machine recognizers retired.

Related records:
[native-effect-and-st.md](native-effect-and-st.md),
[algebraic-effects.md](algebraic-effects.md) AE-11 row,
and `/Users/biu/Projects/psrs-stdlib/docs/native-effect.md`.

### Checkout

| Owner | Current state |
| --- | --- |
| Compiler | `/Users/biu/Projects/purescript-rs`, branch `feat/effect-handlers`, HEAD `bca4fea7` (`Give native Effect/ST a phantom-result free representation`). Working tree dirty: P8 WIT live-type layout, native representation tests, `assertions` runner migration, AE-11 docs, `stdlib.lock.json` fingerprint. |
| Independent stdlib | `/Users/biu/Projects/psrs-stdlib`, branch `control-library`, HEAD `d6cf81105046e288e2e9cd9685f8cc47e960c988`. Working tree dirty: official Effect/ST/`Ref` adapters, `PSRS.Effect.Native`/`Host`, WASI raw `*Impl` + `Host.lift`, `Test.Assert.assertImpl` source, docs. |
| Compiler lock | `path` `../psrs-stdlib`, revision **still** `d6cf81105046e288e2e9cd9685f8cc47e960c988`, development fingerprint `fnv1a64-v1:c54b996e0f7e0295`. Change the fingerprint when `lib/`, `conformance/`, or manifests change. Compute it with the Rust hasher in `crates/psrs-driver/src/prelude/package.rs` (`PathBuf` sort). A Python hash will disagree. Move the **revision** only after the independent repository is committed. |
| Work | Continue AE-11 in this checkout. Preserve the dirty trees. No push, PR, issue close, or `cargo test --workspace` unless the user asks. |

Dirty compiler paths:

- `crates/psrs-backend/src/cc/layout/functions/reachable.rs`
- `crates/psrs-backend/src/cc/layout/tests/arrays.rs`
- `crates/psrs-driver/src/tests/assertions.rs`
- `crates/psrs-driver/src/tests/native_representation.rs` (deleted; replaced by `native_representation/`)
- `docs/implementation/backend/{algebraic-effects,native-effect-and-st,delimited-control-migration-handoff-2026-10-12}.md`
- `stdlib.lock.json`

Dirty stdlib paths: `lib/PSRS/Effect/` (new), `lib/Effect.purs`, `lib/Effect/{Ref,Unsafe}.purs`, `lib/Control/Monad/ST/Internal.purs`, `lib/Test/Assert.purs`, `lib/WASI/{Clock,FileSystem,IO,Network,Process,Random}.purs`, `lib/trusted`, `upstream-lock.json`, `docs/{effect-restoration,st,native-effect}.md`.

### What already executes

Official `Effect`, `Effect.Unsafe`, `Effect.Ref`, and `Control.Monad.ST.Internal`
are constructor-hidden newtypes over `PSRS.Effect.Native.Program`. `Program a`
is a phantom-result Free (`data Program a = Program View` with `Val` erasure)
so `type role Effect representational` and `type role ST nominal representational`
are accepted. `toEffect` is `unsafeCoerce`. `unsafePerformEffect` is `Target.run`.
Construction is inert. `run` interprets `new`/`read`/`write` with
`arrayFill`/`arrayWrite`/`arrayIndex` and `perform` by calling the delayed
thunk. `abort` is a request whose handler evaluates `trap`; a strict trap
field in a record would abort every run.

WASI public APIs stay `Effect`. WIT imports are **raw canonical results**.
`PSRS.Effect.Host.lift f = unsafeCoerce (Target.perform f)`. A WIT import
typed as `Effect X` is ABI-invalid: `core_matches_result` does not unwrap
`Effect`. `Test.Assert.assertImpl` is source: `true` is `pure unit`; `false`
writes through `Effect.Console.error` then `unsafeCoerce Target.abort`.
`checkThrows` remains `foreign import`.

P8 guest projection keys a WIT import by its checked scheme `TypeId`.
`live_type_ids` now records Runtime and WIT import schemes as layout roots,
including schemes that appear only on the import. `WASI.Clock` pulls
`WASI.IO`; `pollImpl`/`readImpl` `Array` payloads lived only on those schemes.
That was the `array has no representation` failure while projecting Native
`Cell`/`Array` methods. Keep Clock and `Pollable` together. The layout unit
test is `a_wit_import_scheme_lays_out_array_payloads_without_a_declaration_root`.

Focused evidence, `PSRS_REQUIRE_WASMTIME=1`, 2026-10-16:

```sh
cargo fmt -p psrs-backend -p psrs-driver
cargo test -p psrs-backend --lib cc::layout::tests::arrays
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::native_representation
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::assertions
```

Results: layout arrays **2 passed**; `native_representation` **12 passed**
(11.32s); `assertions` **8 passed** (10.91s). That includes
`official_clock_now_runs_through_effect` (exit 42) and
`official_wasi_and_assert_wrappers_run_through_effect` (`now`, `wallNow`,
`randomU64`, `randomBytes`, `getStdout`, `WASI.Console.log` stdout
`host-leaf\n`, held `assert true`, exit 42). Held and failed `Test.Assert`
paths write then trap. §14 control/integration/workspace commands were not
rerun on this tree.

### Contracts to keep

- Keep per-IR machine recognizers retired. Construction owns machine meaning
  (DEC-21). MIR checks dispatch arms and external tail targets only.
- Keep official public signatures, exports, roles, classes, and remaining
  pure declarations. Repair compiler defects in the compiler.
- Keep WIT leaves at canonical results. Delay them with `Host.lift`.
- Keep unimplemented JS foreigns as unimplemented foreigns. Do not fill them
  with recursion, a fabricated result, or another successful-looking
  placeholder.
- Keep `checkThrows` foreign until a trap can return `Boolean` on the same
  instance. A canonical-ABI or guest trap aborts the component instance.
- Keep AE-11 / NE-* **Unverified** until the remaining rows have required
  evidence. Re-measure D-04 before changing those numbers.
- Work in the current checkout. English docs. Source files stay under 500
  lines. Group a later commit by topic (code, tests, and the design text for
  the same topic).

### Remaining work (do this next)

1. **Driver tests that still name `runEffect`.** Official runners are
   `unsafePerformEffect` / `ST.run`. Many `crates/psrs-driver/src/tests`
   sources still say `runEffect` (`wasi/`, `integration.rs`, `show.rs`,
   `polymorphism_erasure_audit.rs`, and others). `tests::assertions` already
   uses `unsafePerformEffect`. This is a wat-shape review, not a global
   rename. In particular
   `stored_random_bytes_leaves_get_random_bytes_inside_the_effect_closure`
   (`wasi/mod.rs`) asserts compiler-owned Effect closures and `call_ref`
   from the command entry. Native `run` interprets `perform` thunks; rewrite
   those assertions against the new lowering, or drop claims the new
   representation does not make.

2. **`checkThrows`.** Leave it foreign. Instance-terminal traps cannot return
   `Boolean`.

3. **JS-only Effect foreigns.** `Effect.Console` timers (`time`, `timeLog`,
   `timeEnd`, `clear`), `Effect.Exception`, `Effect.Aff`, `Effect.Uncurried`.
   Keep the source contract and report missing target support.

4. **Package pin.** Commit `psrs-stdlib` first, then set `stdlib.lock.json`
   `revision` to that commit and recompute `source_fingerprint`. Until then
   the dirty development fingerprint is `fnv1a64-v1:c54b996e0f7e0295`.

5. **AE-11 / D-04.** Mark AE-11 Verified only after remaining host leaves and
   the issue's validation block. Remeasure D-04 with
   `PSRS_ORACLE=annotations` and `PSRS_REQUIRE_WASMTIME=1` before editing
   those numbers.

Suggested first slice: item 1 on `wasi/` plus any test that fails immediately
with `unknown name runEffect`, with focused
`PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi`. Then
update this section and `native-effect-and-st.md` in the same change.

When the slice touches compiler or test code:

```sh
cargo fmt --all --check
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::native_representation
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::assertions
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi
```

Say that §14 was not a measurement of this tree. Skip
`cargo test --workspace` unless asked. Skip push, PR, and issue close unless
asked. Skip bumping the lock revision until the independent repository is
committed.

## 17. Official runner migration in driver tests (2026-10-16)

Current continuation of §16 item 1. The starting compiler HEAD is
`bca4fea77bda5f79bb44bf6e4660d7bec981d448`; both compiler and independent
package were already dirty.
No package sources, lock identity, compiler lowering, or DEC-21 trust boundary
were changed in this slice. §14 is not a measurement of this tree.

All remaining `runEffect` references under `crates/psrs-driver/src/tests/`
now use an explicit official `Effect.Unsafe (unsafePerformEffect)` import.
The affected cases cover WASI wrappers, linked modules, Show, evaluation
order, polymorphic transport, and ordinary library combinators. The lenient
source-origin diagnostic additionally requires the `UnknownName` span to
select `missingName`, preventing a missing runner from satisfying the test.

The two historical WAT closure tests are now runtime contracts:

- `stored_random_bytes_is_inert_and_repeated_runs_execute_the_continuation`
  checks no output from an unexecuted action, then two runs returning a total
  byte length of 16 and exactly `bytes\nbytes\n`.
- `stored_exit_is_inert_and_interpreted_exit_stops_the_program` checks normal
  exit 0 for a stored exit request, exit 99 for interpretation, and no output
  from the log following the exit.

These cases do not claim a particular `call_ref` location or indirect-call
count. Native interpretation may call ordinary helpers while constructing
requests. The now-unused `wasi/wat.rs` textual call-graph helper is removed.
WIT import and layout assertions in the other WASI tests remain.

Baseline logs retain the exact failing names:

- `/private/tmp/psrs-native-wasi-before.log`: **116 passed, 35 failed**;
  the failures report `UnknownName` for `runEffect` at P3 resolve.
- `/private/tmp/psrs-native-runners-before.log`: **110 passed, 11 failed**
  across the non-WASI filter selection below. Failures name the same
  retired runner; four also report the missing explicit `Effect` type import.
  The source-origin diagnostic passed even with that extra
  unresolved name; its new span assertion closes that false positive.

Validation uses Wasmtime `49.0.3 (f96eabaa1 2026-10-14)` and
`PSRS_REQUIRE_WASMTIME=1` throughout. The combined run uses:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib -- \
  tests::wasi tests::native_representation tests::assertions \
  tests::integration tests::show tests::polymorphism_erasure_audit \
  tests::data_function tests::guards tests::generic_aggregate_audit \
  tests::pattern_matching_audit tests::cc_ir_audit tests::semigroup \
  tests::foldable tests::records tests::typecheck
```

`/private/tmp/psrs-native-runners-final.log` records **288 passed, 4 failed**
(858.91s). All four failures are missing `Effect` type imports in Foldable,
Semigroup and the two Show signature fixtures; all four pass in the following
post-fix rerun. The combined run includes **151/151 WASI**, **12/12 native
representation**, and **8/8 assertion** tests. It was not repeated in full
after the four import fixes.

The three audit modules live at crate root, so the `tests::` prefixes above
select none of their tests. Their actual validation is separate:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib -- \
  cc_ir_audit::effects_observe_callee_and_argument_order \
  generic_aggregate_audit polymorphism_erasure_audit --test-threads=2
```

`/private/tmp/psrs-native-runners-audits.log`: **28 passed**, no failures
(615.61s). This includes the aggregate audit battery, mutable/erased payload
transport, rank-N application, and observed argument order.

The Show/Foldable/Semigroup rerun after adding explicit `Effect` imports is
**11 passed**, with no failures:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib -- \
  tests::show tests::semigroup tests::foldable
```

Log: `/private/tmp/psrs-native-runners-imports.log` (197.76s). Across the
combined run, this post-fix rerun, and the separate audits, **320 distinct test
names pass**, with no unrecovered failure. This is a union of scoped runs,
not a single clean 320-test invocation. The name-by-name reconciliation is
`/private/tmp/psrs-native-runners-verification.json`.

`cargo fmt --all --check` passes. Strict
`cargo clippy -p psrs-driver --lib --tests -- -D warnings` stops in the existing
`psrs-kind` `let_and_return` diagnostic. The same command with
`-A clippy::let_and_return` passes. Logs:
`/private/tmp/psrs-native-runners-clippy{,-known}.log`.
These are scoped driver checks, not a strict workspace Clippy pass.

Next: finish native API and pinned conformance evidence from
[native-effect-and-st.md](native-effect-and-st.md), then the independent
package commit and coherent compiler lock pin. `checkThrows` and JS-only
foreign support remain explicit gaps. AE-11 and NE-* remain Unverified;
D-04 measurements stay unchanged. No workspace suite, ignored scoreboard,
push, PR, or issue close was performed.


## 18. Native execution, generic checking and abstract regions (2026-10-16)

This section is history. Current progress is **§19**. The text below continues §17. The compiler remains on feat/effect-handlers at
`bca4fea77bda5f79bb44bf6e4660d7bec981d448`, with the original dirty tree preserved
and further changes. The independent package remains on control-library at
`d6cf81105046e288e2e9cd9685f8cc47e960c988`, also dirty. No commits, pushes, PRs,
issue operations or workspace test run were performed.

The current source fingerprint is `fnv1a64-v1:b5ec750799a46cc4`; the revision
is deliberately unchanged until the independent repository is committed.
The complete owner, regression, acceptance and command record is in
[native-effect-and-st.md](native-effect-and-st.md#native-execution-and-region-boundary-evidence-2026-10-16).
Do not repeat the old closure or per-IR machine-recognizer migration.

Implemented in this slice:

- Native Program retains Bind as data and interprets an explicit persistent
  continuation stack. Pure-bind callbacks are delayed and repeat on each run;
  20,000-level binds/public loops execute without native stack growth.
- Official Ref and Global conversion behavior, heterogeneous cells, reentrant
  modify, self-reference publication and the null/trap initialization boundary
  have executed public API plus pinned JS observations.
- Ordinary package PSRS.Command.run now configures Effect Unit execution.
  Generic command adaptation still preserves Int and an unrelated Job type.
- Core command closed-type checks, shared free-variable queries and equal-arity
  universal specialization repair generic ownership defects exposed by the
  native integration. Malformed Core and complete backend tests pass.
- HIR source external ownership survives imports; kind/reference checking
  includes foreign annotations at their original source. ST/STRef retain the
  official Region kinds. STArray/STFn target constructors stay private;
  sibling partial-array adapters cross their representation explicitly.
- Independent package conformance owns native-effect.mjs, native-types.mjs and
  their public source cases. All 50 upstream checkouts were fetched under
  `/private/tmp/psrs-native-pinned-upstreams`; all 279 original modules compile
  with official purs. Do not use the emptied historical /tmp checkout paths.

Measured on the stated package: 92 required-Wasmtime control/native/assertion
tests, 875 changed-library tests, 4 generic CLI command tests, 12 Node tool tests,
11 native public cases and 11 actual-upstream type comparisons pass. Strict
scoped Clippy passes without allowances after two lint-only cleanups. Source
audit: 50 packages, 308 modules (241 identical, 38 modified, 29 target), none
missing. The linked acceptance record names exact logs and limitations.
AE-11 remains Unverified; D-04 numbers remain unchanged.

**Do next:** finish uncurried adapters. ST.Uncurried currently returns `f args`
while constructing runSTFn actions, so an explicitly unsafe effect in that
factory could run early. Delay the factory with an ordinary bind of pure Unit.
Effect.Uncurried still has its original unimplemented foreign declarations;
provide constructor-hidden target wrappers with the same signatures, roles and
pure instances, and delay factory application in runEffectFn1–10 as well.
Compare every arity with pinned official JS, preserving strict arguments and
repeated execution. Do not replace unsupported foreign slots with placeholders.
Then finish the NE-01/03/04/08/10 obligations in the checklist, perform a coherent
package commit/pin only after its required evidence, and remeasure the required
D-04 boards before changing their numbers. Keep the full requested objective
active until the actual acceptance audit closes it.

## 19. Uncurried adapters (2026-10-16)

Current continuation of §18. The compiler remains on `feat/effect-handlers` at
`bca4fea77bda5f79bb44bf6e4660d7bec981d448`, dirty. The independent package
remains on `control-library` at `d6cf81105046e288e2e9cd9685f8cc47e960c988`,
dirty. No commit, push, PR, issue operation, workspace suite, or D-04
scoreboard was run. Wasmtime is `49.0.3 (f96eabaa1 2026-10-14)`.

The lock revision is still `d6cf8110`. The development fingerprint is now
`fnv1a64-v1:ba951ab7122a6e5e`. §18's `fnv1a64-v1:b5ec750799a46cc4` is the
package before these adapters. The three driver tests below selected that
package and passed, so this fingerprint matches the sources those tests
compiled.

`Effect.Uncurried` and `Control.Monad.ST.Uncurried` no longer leave `mk`/`run`
as unimplemented foreigns. `EffectFn1`–`EffectFn10` and `STFn1`–`STFn10` are
constructor-hidden newtypes over curried functions that return a suspended
`Effect` or `ST` action. Roles match the official declarations: every
`EffectFn` parameter is representational, and each `STFn` region parameter is
nominal. `mk*N` stores the callback. `run*N` delays application with
`bind (pure unit) (\_ -> f args)` at every arity. The `EffectFn` Semigroup and
Monoid suffix is byte-identical to the pinned `purescript-effect` file at
`a192ddb923027d426d6ea3d8deb030c9aa7c7dda`, from `instance semigroupEffectFn1`
through the end of the file. These adapters do not implement a JavaScript
calling convention. Raw WIT leaves stay canonical and go through `Host.lift`.

The package document `/Users/biu/Projects/psrs-stdlib/docs/native-effect.md`
describes this representation. `manifest.json` registers
`conformance/native-uncurried.mjs` as `native_uncurried_oracle`. The acceptance
checklist's remaining paragraph in
[native-effect-and-st.md](native-effect-and-st.md) still lists uncurried
factory delay as open; that paragraph was not edited in this pass. NE-03 stays
**Unverified**. These cases are one part of that row, not its public-API oracle.

Baseline failures, retained:

- `/private/tmp/psrs-native-uncurried-before.log`: both factory tests failed.
  Effect stopped at P8 because `Effect.Uncurried.mkEffectFn1` and
  `runEffectFn1` had no target implementation. ST stopped at P5 with
  `EscapedSkolem` on the unannotated action.
- `/private/tmp/psrs-native-st-uncurried-before.log`: after the ST action was
  annotated, arity 1 printed `argument\nfactory\nstored\n`. Returning `f args`
  from `runSTFn` ran the factory while constructing the action and did not
  repeat it.

Current driver evidence is
`crates/psrs-driver/src/tests/native_representation/uncurried.rs`. Arity 1–10
for both families expects exit 42 and
`argument\nstored\nfactory\nfactory\n`. The expected integer is the weighted
sum `n(n+1)(2n+1)/6`. The mixed case transports an `EffectFn2` of `String` and
`Int -> Int`, appends two `EffectFn1` values, uses `mempty`, and expects
`left\nright\n`.

This pass ran:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib -- \
  tests::native_representation::uncurried -- --test-threads=2
```

`/private/tmp/psrs-native-uncurried-current.log`: **3 passed**, no failures
(149.38s). Cargo did not rebuild the test binary (0.51s). Earlier same-day
logs are historical: the factory pair passed in 139.72s before `uncurried.rs`
was last written, and the mixed test alone passed in 10.83s after that write.
The three-test log is the measurement of the current file.

`conformance/native-uncurried.mjs` was started and did not finish.
`/private/tmp/psrs-native-uncurried-oracle.log` records accepted target runs
for `EffectFn1`–`EffectFn10` and `STFn1` only. `STFn2` has official stdout and
no `target/run.json`. `uncurried.json` was not written. No oracle process is
still running. That directory is not a 21-case result. The script's intended
set is twenty arity cases plus `MixedInstances`.

`upstream-lock.json` records a false identity for `Effect/Uncurried.purs`.
Both `upstream_sha256` and `vendored_sha256` are
`b4004312b629cf047cc96a3674934eec8badd8bdd58935b9110ea9f7a77b7027`, which is
the adapted file. The pinned upstream source at the lock's commit
`a192ddb9` is still
`0c6db8591d2519d9b18bf2c5ec56f33b6c25ee6f69d3edfbce8482d48771214b`.
`Control/Monad/ST/Uncurried.purs` keeps upstream
`bd2411e5e23d8b446ef2f6b924422071eb99c6c629395df21eb9821a341988a6` and
vendored `22b448946ad3612741adada761a0ff2fc03e0710cc2d6c10f9e2c83c350ca7b2`.
Restore the Effect upstream hash before any source-fidelity count. An audit
that trusts the current lock would classify the adapted module as identical.

`/private/tmp/psrs-native-wasi-current.log` contains 133 lines ending in `ok`
and no `test result` summary. It is an interrupted run, not a 151/151 result
and not a recorded failure set.
`/private/tmp/psrs-native-uncurried-clippy.log` shows `psrs-core`,
`psrs-backend`, and `psrs-driver` finishing in 4.06s with no diagnostic text.
The command flags are not in that log, so it is not a strict workspace Clippy
result. This pass did not re-run formatting, Clippy, WASI, or the oracle.

AE-11 and every NE-* row stay at the statuses in
[native-effect-and-st.md](native-effect-and-st.md). D-04 numbers stay
unchanged. §14 and §18 are not measurements of fingerprint
`ba951ab7122a6e5e`. `checkThrows`, `Effect.Console` timers, `Effect.Exception`,
and `Effect.Aff` remain unimplemented foreigns.

**Do next:**

1. Put `Effect/Uncurried.purs` `upstream_sha256` back to `0c6db859…`, the
   pinned file. Leave `vendored_sha256` on the adapted file. Recompute
   `stdlib.lock.json` `source_fingerprint` with the compiler hasher if that
   edit changes package bytes. Do not copy the adapted hash into the upstream
   field.
2. Finish `conformance/native-uncurried.mjs` through `STFn2`–`STFn10` and
   `MixedInstances`, and keep `uncurried.json`. Do not promote the partial
   directory to a 21/21 result.
3. Continue NE-01, NE-03, NE-04, NE-08, and NE-10 from the checklist. Commit
   the independent package and move the compiler lock revision only after that
   evidence. Remeasure D-04 before changing its numbers.
4. Keep per-IR machine recognizers retired and leave DEC-21's trust boundary
   in place. Keep unimplemented foreign slots unimplemented.

The public oracle and the NE-01/03/04/08/10 rows closed in §20. This section's
fingerprint `fnv1a64-v1:ba951ab7122a6e5e` and its unfinished `uncurried.json`
are historical.

## 20. Effect/ST and AE-11 acceptance (2026-10-16)

Current continuation of §19. Compiler `feat/effect-handlers` was dirty at
`bca4fea77bda5f79bb44bf6e4660d7bec981d448` when the oracles ran. The
independent package commit is `4c73aab52226e8f5e7b1f17052f1a19d717a472e` on
`control-library`. No push, PR, issue operation, workspace suite, or D-04
scoreboard was run. Wasmtime is `49.0.3 (f96eabaa1 2026-10-14)`.

The compiler lock revision is that package commit. The development fingerprint
is `fnv1a64-v1:ad19620ada1d2ecf`. `Effect/Uncurried.purs` `upstream_sha256` is the
pinned file `0c6db8591d2519d9b18bf2c5ec56f33b6c25ee6f69d3edfbce8482d48771214b`;
`vendored_sha256` stays the adapted file. `Data/Number.purs` `vendored_sha256`
matches the file on disk.

`conformance/native-uncurried.mjs` finished: 21/21 accepted, including
`STFn2`–`STFn10` and `MixedInstances`
(`/private/tmp/psrs-native-uncurried-oracle-2/uncurried.json`).
`conformance/native-effect.mjs` accepted 13/13, including `Identity` and
`HostOrder` (`/private/tmp/psrs-native-effect-oracle/native.json`).
`conformance/native-types.mjs` accepted 11/11, including `HiddenSTFn`
(`/private/tmp/psrs-native-types-oracle/types.json`). The source audit is 240
identical, 39 modified, 29 platform additions, 308 modules, 50 packages
(`/private/tmp/psrs-native-effect-audit/inventory.json`). Each oracle records
the same fingerprint and `unchanged_during_run: true`.

The driver rejection test
`a_coerced_non_cell_ref_is_rejected_before_codegen` expects P8
`unsupported aggregate conversion between normalized runtime shapes` for
`unsafeCoerce 1 :: Ref Int`. Together with the host-order test, the filtered
driver run is 3 passed.

NE-01, NE-03, NE-04, NE-08, and NE-10 are Verified in
[native-effect-and-st.md](native-effect-and-st.md). AE-11 is Verified in
[algebraic-effects.md](algebraic-effects.md). The historical 39 modified
modules outside the Effect family are not re-approved. `checkThrows`,
`Effect.Console` timers, `Effect.Exception`, and `Effect.Aff` stay
unimplemented. Filesystem and environment evidence remains the existing
WASI-07 record; it was not re-run on this fingerprint. The 2026-10-17
scoreboard later moved L6/M7 to 273/413. BE-21 stays Partial.

**Do next:** the 2026-10-17 scoreboard moved L6/M7 from 164/413 to 273/413.
`BigFunction.purs` still dies before a diagnostic. Keep per-IR machine
recognizers retired and leave DEC-21's trust boundary in place.
