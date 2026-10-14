# Handoff: Delimited Control Migration and Remaining Acceptance

**Updated:** 2026-10-14, after repairs for architecture review R1–R3.
The filename keeps its original date; this file is the current snapshot.
The subsequent [lowering design review](delimited-control-lowering-review-2026-10-14.md)
reviews the committed implementation at `1e24e5d5`. Its construction/proof-boundary
proposal is unimplemented; review that boundary before extending the recognizers.
**Objective:** Complete delimited control, then implement native Effect/ST.
**Status:** R1–R3 are repaired in implementation and permanent tests, included
in the local checkpoint commit containing this handoff.
Check/Close bodies, full Executor/lifecycle/factory paths, explicit observable operations,
reference refinements and retained/inlined publication contents now have checked
relations. Complete AE/AC acceptance remains outstanding; workspace validation
and the known Effect migration baseline are reported separately below.

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

## 4. What the machine verifiers check now

CC (`cc/verify/control/`) and MIR (`mir/control/`) select the following
checks by their retained helper/source roles:

- **dispatch** (`dispatch.rs`): dispatch covers exactly the live alternatives;
  default traps; projections match the proven selected case; the result
  originates from the current input's dispatch (CC); MIR additionally requires an
  immutable `i32` tag field, the current helper input selector, and that the input
  dispatch dominates every invoke/return/backedge (entry preheader initialization
  excepted).
- **clause selection** (`clauses.rs`): a source `Return` applies the scoped
  return clause; a source `Perform` applies the scoped operation clause.
- **scoped execution** (`clauses.rs`): the next computation is the full
  application result, the first argument is the current source payload and an
  operation supplies its raw resumption. MIR also checks execution dominance;
  both retain state/root handler identity.
- **Bind/Resume steps** (`steps.rs`): current frame operands, queue composition,
  and replayed queue/activation identity on the actual transition.

Additional current checks:

- **root/queue execution** (`execution.rs`): the complete root handler call,
  paired result fields, queue abandonment, terminal state/answer, Cons consumption,
  all four Append-left actions, Finish restoration and required-action coverage.
- **normal-path operation accounting**: CC `execution/effects.rs`, MIR
  `traces.rs` and shared `control/execution/observables.rs` check required dynamic
  applications and selected static helper calls, and reject extra calls/array
  stores. The repaired event contract accounts for explicit traps, other writes and
  refinements as well; exceptional paths terminate before work after a failed
  guard. See the repair evidence for precise trust boundaries.
- **owned execution** (`owned.rs`): Pending guard, Running, load, clear, one
  callback, Consumed and transition order, with current owner/answer/result
  identities. Equality guards have positive CC and MIR tests.
- **initialization/cancellation** (`initialization.rs`): current private contents
  at publication, including intervening writes, and cancellation order. CC
  and MIR negatives execute on retained helpers; separate optimized-MIR
  publication mutations cover physical Start/Once cells after inlining.

Shared relations live in `control/{provenance,dispatch,steps,owned}.rs` and
`control/execution/`; CC and MIR fact propagation lives in `cc/branch_facts.rs`
and `mir/branch_facts.rs`. `Check`/`Close` callee binding is checked, but their
lifecycle bodies are now verified separately by shared `control/lifecycle.rs`
and paired adapters. `control/events.rs` owns ordered events; per-IR refinement
checks justify narrowing recovery. A total-function summary replaces the former
no-write exemption. MIR additionally rechecks inlined Start/Once publication
windows from their physical allocations and source witnesses.

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
No new full workspace or acceptance measurement was made. The next action is to
resolve the schema/action inventory and proof-discharge decision, then implement
the Bind/Return/Cons vertical slice under the review's migration gates. Existing
R1–R3 repairs and all remaining AE/AC obligations remain in force.
