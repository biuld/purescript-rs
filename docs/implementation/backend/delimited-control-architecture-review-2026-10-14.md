# Delimited Control Architecture Review — 2026-10-14

**Status:** R1–R3 repaired in the local checkpoint commit containing this review;
complete AE/AC acceptance remains open.
The findings and review measurements below describe the pre-repair snapshot.
See [the repair evidence](#repair-evidence-2026-10-14) for current implementation and validation.
The later construction migration retires those recognizers; see
[the current proof-owner audit](#current-proof-owners-2026-10-15) before treating
their historical tests as current coverage.
**Design:** [Control machine](../../design/backend/fp/control-machine.md),
[delimited control](../../design/backend/fp/delimited-control-and-resumable-frames.md)
and [handler scopes](../../design/backend/fp/control-elimination-and-handler-scopes.md).
**Reviewed checkout:** `feat/effect-handlers`, compiler HEAD
`6df8bfba59f74659883f8572ac8a88c821a6574e`, with pre-existing uncommitted
implementation and documentation changes. Locked stdlib `d6cf811` was clean.
**Continuation:** [Current handoff](delimited-control-migration-handoff-2026-10-12.md).

## Architectural assessment

The software continuation architecture remains coherent: checked Core owns
source/target types; immutable CC planning owns nominal machine carriers and
helper signatures; lowering constructs persistent nodes and a guest driver;
MIR fixes physical layouts and loopifies self transitions. Concrete effects,
operation meanings and deep/shallow policies remain library-owned. The review
found incomplete verification boundaries rather than a reason to replace this
architecture.

The current dirty implementation has advanced beyond the previous progress
description. Shared `control/execution/` relations check root handler application,
terminal publication, Cons consumption, Append reassociation and Finish
restoration. Shared `control/owned.rs` checks ordered one-shot transfer,
cancellation and private-cell contents at publication. CC/MIR adapters supply
reference identities and path events. These are substantial local checks, but
they do not establish complete machine semantics.

## R1 — P1: helper identity is accepted as a lifecycle-body proof

**Owners:** `cc/lower/control/lifetime.rs`,
`cc/verify/control/{mod,checkpoints,finish_consumption,root}.rs`, and the paired
MIR control verifiers. Helper callee contracts bind a role to a symbol/signature;
no corresponding Check/Close body relation proves their lifecycle behavior.

**Reproduction:** Lower `control_checkpoints::SOURCE` to CC. For each of Check
and Close independently, clear the helper's assignments and set its result to
its sole extent parameter. `cc::verify_module` returns `Ok(())`. Lower the same
module with `mir::lower_module`; `mir::verify_module` also returns `Ok(())`.
The temporary test is `review_probe_lifetime_helper_bodies`.

**Implication:** A correctly bound call can now perform no liveness check or no
close write. The caller-side proofs still see the expected callee identity and
operand. This is a verifier hole; production lowering currently emits the
intended behavior, and this review does not claim a source-level runtime failure.
AC-03 cannot be closed from callee identity and current positive runtime tests.

**Required repair:** Give Check and Close shared body contracts. Check validates
its own status and every ancestor and rejects closed extents; Close writes closed
to its own status and returns the same extent. Prove the retained body and the
discharged inlined form. Keep function identity, operand provenance and body
semantics as separate evidence. Add negatives for missing/wrong status checks,
skipped ancestry, missing/wrong close stores and wrong returned extent.

## R2 — P1: operation traces do not account for all observable behavior

**Owners:** `mir/control/traces.rs:109`,
`control/execution/observables.rs:111`, `control/owned.rs::publication`,
`cc/verify/control/execution_effects.rs`, and CC/MIR read-only summaries.

**Reproduction:** Compile the same source to optimized MIR. Select executor path
`[Return=0, Empty=0, Root=0]`. Before its normal return, append either:

- an `i32` constant 1 and `Store(address=1, value=1, memory=0, offset=0)`;
- a Boolean constant true and `TrapIf(true)`.

Both variants pass the complete `mir::verify_module` call. The temporary test is
`review_probe_unaccounted_effects`. This establishes accepted extra mutation and
explicit trap instructions; mutated Wasm was not executed in this review.

**Cause:** The MIR trace adapter turns non-array writes into `OtherEffect`,
which the shared root/queue relation ignores. It emits no event for a trap-only
instruction. Allocation and mutation share the same event even though normal
machine steps legitimately allocate frames. Publication also ignores effects
outside tracked private-cell writes. Therefore rejecting additional applications
or ArraySet operations does not prove absence of additional observable work.

Two adjacent boundaries need the same repair, even though the probes above do
not exercise them:

- `read_only_functions` excludes writes/dynamic calls but permits trapping reads,
  explicit traps and static recursion. The shared relation permits arbitrary
  direct calls in that set. No-write is not evidence of totality or termination.
- MIR `branch_facts::exits` and CC `owned::traces` omit paths ending in a trap;
  CC executor effect accounting begins inside computation alternatives. Effects
  before dispatch and effects preceding an exceptional exit need coverage too.

**Required repair:** Use an owner-neutral execution-event contract shared by the
machine relations. Distinguish required allocation, mutation, reads, calls and
explicit traps; justify fallible casts/projections using their checked shapes.
Reject unaccounted operations rather than dropping unknown events. Static calls
need a checked semantic role or appropriate behavior evidence, including possible
trap/divergence; do not infer unobservability from a no-write set. Track exceptional
paths separately and retain the ordered events preceding them. This does not
implement caught-host-trap recovery: that remains a separate required boundary.

## R3 — P2: two intended MIR negatives never reach the verifier

**Owner:** `psrs-driver/src/tests/control_owned_steps/mod.rs:167` and `:320`.

The focused control run fails
`mir_private_cells_are_checked_at_publication` and
`mir_cancellation_requires_pending_cancelled_and_clear`. Both unwrap a missing
helper from the optimized module: the shared source does not keep the relevant
Start/Once/Cancel function alive through inlining/elimination. The mutations and
their intended assertions never run. Corresponding CC tests pass, and the
normal-path owned-transfer MIR mutations run, but neither substitutes for these
publication/cancellation negatives.

**Required repair:** Exercise live Once/Cancel through a suitable source fixture,
test retained helpers at the pre-optimization MIR boundary, and separately cover
actual inlined factory/cancellation operations. Removing the fixture panic by
skipping a missing function would preserve the coverage gap.

## Design corrections made in this review

- Finish closes its owned extent before restoring parent/rest; its inventory
  previously said it closed after rest completed, contradicting the driver.
- Raw capture saves the dispatch context checkpoint, which can differ from the
  active execution extent during replay. Driver pseudocode now uses
  `dispatchExtent(context)` for both root and scoped operations.
- One-shot acquisition loads then clears the callback slot before invoking it,
  with the callback retained by a local GC root. A trap leaves the cleared owner
  Running; no caught-trap recovery is implemented. The old pseudocode cleared it
  after application, and the model incorrectly described owner mutation as the
  machine's only mutation despite mutable extent status.
- The Wasm engine owns frame GC. The guest runtime supplies runtime leaves and
  linear-memory allocation; the compiler-owned linker assembles artifacts.
- The normative verifier contract now includes root/queue operands, owned
  ordering, current publication contents, lifecycle helper bodies and complete
  observable-operation accounting. The latter two remain implementation gaps;
  the design is not narrowed to fit the current verifier.

## Current evidence and limits

Evidence root: `/private/tmp/psrs-control-architecture-review-2026-10-14/`.
Wasmtime: 49.0.2 (`3c8a3e79a`, 2026-10-02). Default locked package;
`PSRS_REQUIRE_WASMTIME=1` for all execution scopes below.

| Command / evidence | Observed result |
| --- | --- |
| `cargo test -p psrs-backend --lib` | 444 passed (`backend.log`) |
| `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::control` | 173 passed, 2 fixture panics (`control.log`) |
| All eleven existing control integration targets | 81 passed, no skips (`integration.log`) |
| `cargo test -p psrs-driver --lib review_probe -- --nocapture` while temporary probes were present | 2 probes passed, reproducing malformed acceptance (`probes-all.log`) |
| `cargo fmt --all --check`, `git diff --check`, local document link targets | Passed |

The integration command lists `control_surface`, `control_runtime`,
`control_open_runtime`, `control_scoped_runtime`, `control_scoped_surface`,
`control_scoped_replay`, `control_owned_runtime`, `control_owned_surface`,
`control_lifetime`, `control_carrier_runtime` and `control_primitive_values`
as `--test` targets. This is focused runtime evidence, not complete AE/AC closure.

Probe source is retained as `probes.rs` in the evidence directory. It was appended
temporarily inside `control_queue_steps/mir.rs` to reuse that fixture's dispatch
navigation, then removed exactly. No production Rust or permanent tests were
changed. Preserve these probes as rejection regressions when implementing R1/R2.

Workspace tests/Clippy, ignored scoreboards, full upstream differential and
package/source conformance were not rerun. Previous workspace failures and
package measurements remain historical. D-04 measurements/statuses are unchanged.
Foreign captured-resource validity, caught-host-trap cleanup/reentry and complete
upstream API acceptance remain required and unverified.


## Repair evidence (2026-10-14)

The repair preserves the dirty compiler checkout and default locked stdlib.
There are no commits, pushes, PRs or package changes. Evidence root:
`/private/tmp/psrs-control-repairs-2026-10-14/`; the `before/` directory retains
the pre-repair backend/test/docs files and `status.before` records the checkout.

### R1: checked lifecycle bodies

Shared `control/lifecycle.rs` verifies both extent cases, each own status slot,
Check's live guard and parent continuation, and Close's closed write and returned
extent. CC and MIR adapters validate actual body exits, including loopified Check.
A matching helper symbol alone cannot discharge this proof. Missing bodies that
still have calls fail ordinary callable verification. The ordinary inliner retains
these branching helpers; it cannot inline an identity mutation because the input
module is checked first, and every optimizer intermediate is checked again.

`control_lifecycle_bodies` supplies permanent CC/MIR negatives for identity,
incorrect status/index/guard, missing Close writes and wrong ancestry. Existing
lifetime, replay and one-shot execution tests exercise the valid bodies.

### R2: execution events and reference refinements

`control/events.rs` now owns ordered observations for every machine relation.
CC traversal retains exceptional paths and cast identities; the CC execution
checks now live under `cc/verify/control/execution/`. MIR `branch_facts`
exposes both all-path and normal-exit views. Entry preheaders and full CC bodies
are checked rather than beginning inside a dispatch alternative. Allocation,
mutation, reads, direct/dynamic calls, reference refinements and explicit traps
are distinguished. An explicit failed machine guard must terminate before any
further observable operation. Preceding events must be a prefix of a validated
normal action; a correct-looking normal branch cannot justify work after failure.

Normal-path casts require static reference compatibility, immutable case evidence
(including single-case nominal variants), or the actual checked callback/result
role. Exceptional prefixes canonicalize observed operands through path aliases and retain
instruction/cast source identity so a trap-only bad cast
cannot disappear from accounting. Arbitrary additional stores, loads, calls,
allocations not feeding the transition, and explicit fallible instructions are
rejected. Checked source callback contracts and ordinary SSA/layout legality
remain producer assumptions; no source types are reconstructed from erased data.

The former no-write call exemption is replaced by total-function summaries. A
least fixed point excludes recursive call SCCs, and MIR also rejects reachable
CFG cycles and fallible/read/write instructions. These summaries do not treat
trapping reads or static recursion as unobservable.

`control_queue_steps/mir/effects.rs` rejects extra stores and TrapIf at root
return, trapping default, loop entry and retained Start publication; it also
rejects unused narrowing casts and a Running write after a failed Pending guard.
`control_observable_paths` covers CC pre-dispatch/default calls, publication
calls, payload recovery and no-write recursive calls. Equivalent If/switch joins
remain positive coverage.

The ancestry observation used to rely on the unsafe no-write exemption. It now
instruments the final structured Wasm after the normal compiler pipeline has
verified and assembled the program. The test adds private locals/functions,
replaces the program core module inside its component without changing existing
imports/exports, and validates the resulting component before execution. Its
intentional budget trap is no longer part of a purportedly valid control step.
The depth-sensitive negative and the long-chain/GC checks still execute.

### R3: retained and inlined publication evidence

The two failing MIR fixtures now mutate pre-optimization `mir::lower_module`
output, where the helper being tested exists; neither test skips a missing body.
`control_inlined_publication` separately mutates optimized Start/Once cells before
publication. A write preserving live contents passes; a write publishing status 2
fails. MIR consumes the source witness and physical allocations to recheck current
contents across the factory window. For Run, the window ends before its executor
call; unrelated caller allocations in exclusive alternatives do not affect the
private-cell proof. The existing shared-root exclusive-branch positive still runs.

Execution-event repairs cover Executor, Check/Close and Start/Once/Cancel.
Clone/Rebuild retain their separate existing operand/capture relations and the
stricter total-call summary; complete replay/context step equivalence remains
part of the acceptance audit.

These repairs close the concrete review findings. Foreign resource capture,
caught-host-trap recovery and full reference-library/AE/AC acceptance remain open.
The exact final command results are recorded in the handoff and checklist.


Final validation: focused control **185 passed**, backend **444 passed**, mandatory
integration **81 passed**. Formatting/diff/link checks pass. Strict workspace
Clippy passes with only the pre-existing `clippy::let_and_return` class explicitly
allowed; unmodified strict Clippy is blocked at `psrs-kind:174`.

The integrated workspace snapshot has the same **71 historical failed test
names**, with no additions/recoveries. Two rustdoc dependency-artifact errors
pass on standalone rerun. The final module-layout and path-canonicalization changes
were followed by focused control/backend/integration and lint reruns; full
workspace was not repeated afterward. See handoff §8 for exact scope, comparison
and log paths. Ignored scoreboards, explicit full upstream differential and package
conformance were not rerun; D-04 measurements/statuses are unchanged.

## Current proof owners (2026-10-15)

DEC-21 changes where these obligations are discharged. The historical repair
sections above describe the former CC/MIR recognizers, not current module paths.
The current implementation uses checked construction and ordinary downstream
transform behavior evidence. [The lowering review audit](delimited-control-lowering-review-2026-10-14.md#implementation-audit-2026-10-15)
records the complete remaining migration scope.

| Review requirement | Current owner | Current evidence and limit |
| --- | --- | --- |
| R1: Check/Close bodies, status access and ancestry | Named helper plans, complete typed plan checking, ordered operand-level P8 boundary checking | `control_regressions::lifecycle` restores all eight historical CC mutations for identity, guard, write and ancestry errors; new plan tests reject changed helper outcomes. Required lifetime/replay runtime execution covers the ordinary retained/optimized path. The removed arbitrary post-P9 mutation checks are not claimed. |
| R2: all normal and exceptional prefixes | The P8 boundary consumes every non-cast assignment in order and checks failed-guard/default bodies, actual sources and exits; recovery casts must feed the next checked operation or exit | `control_regressions::observables` restores all four historical CC tests. Extra stores/calls and missing writes fail; unused recovery, re-erasure and callback recovery moved before clearing are additional permanent negatives. Optimizer equivalence remains trusted and tested under DEC-21; generic MIR verification alone does not prove it. |
| R3: meaningful retained and inlined evidence | Factory/helper plans are required by the 40-action inventory; `checkpoints::cc_start_and_once_status_cells_must_be_initialized_live` rejects a spent initializer; actual optimized lifetime/owned tests execute | No missing helper is skipped. A post-P9 write of a spent cell is a trusted-transform obligation, not a restored MIR recognizer. |
| Encapsulation and truthful boundaries | `FrozenControlPlan`, private `CheckedPlan`, representation-neutral field/helper schema and minimal machine emitter | Publication is validated and single-assignment; same-arity wrong field/signature layouts are rejected. Ordinary mutable IR remains constructible and verified. |
| Independent behavioral machine model | Test-only concrete plan interpreter plus separately written typed operational semantics and flat replay | All 40 actions, bounded queue order/freshness/sharing/ancestry, ordered owned transfer and exceptional prefixes are compared. Twenty-two well-formed semantic mutation classes prove the model distinguishes errors beyond shape/type checking; this is bounded construction evidence, not a formal optimizer or callback-body proof. |
| Remaining acceptance | Original AE/AC requirements | AE-01–AE-10 and AC-01–AC-05 are Verified. AE-11 native host IO remains open. D-04 numbers were not remeasured. |

The current snapshot, including the 40th `Run` action and the Root regression
mapping, is [handoff §15](delimited-control-migration-handoff-2026-10-12.md#15-run-construction-and-the-root-regression-slice-2026-10-15).
§13 records the model checkpoint before that slice.
These implementation improvements do not close complete AE/AC statuses or alter
the normative requirements to match the supported subset.
