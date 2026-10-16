# Delimited-control retired regression mapping

This audit maps tests from `01f7e66c` to the DEC-21 construction boundary and
ordinary transformation owners. A name disappearing does not establish either
lost semantics or replacement coverage. The reference inventory contains 185
tests. This document now assigns an owner to every name. That is an audit of
semantic obligations under DEC-21, not a claim that every historical MIR
mutation is still rejected. No global replacement-coverage claim is made.

The mapping is requirement-based. A negative which injects semantic corruption
into already discharged MIR is no longer a promised rejection under DEC-21.
Its semantic obligation moves to checked P8 construction plus transformation
behavior evidence. A generic MIR verifier accepting such a mutation is not
replacement evidence. Positive transformations still need their own coverage.

## Run construction gap found during this audit

Before the current repair, source Run emitted Empty, Start, Root context,
Executor and Close directly into an ordinary caller. Its source evidence checked
only the first three Executor operands. It did not prove the remaining root
operands or normal cleanup. Substituting Check for Close (same signature) passed
P8 verification. The failing before probe is retained in
`/private/tmp/psrs-control-run-before-negative.log`.

Run now has a predeclared three-argument helper role and a complete checked plan.
The source contract calls that role. The plan creates one root, supplies the
same root to execution and dispatch, supplies Empty, closes the root after
execution, and returns that execution result. The P8 ordered boundary checks the
actual helper body. Fresh Start/Run invocations remain ordinary calls, so inlining
and other P9/P10 transformations have their usual preservation obligation.

## Root test family: 24 historical tests

Paths below are relative to `crates/psrs-driver/src/tests/control_root/` at the
reference revision. For paired rows, both named historical tests are included.
Current test modules are under `crates/psrs-driver/src/tests/control_regressions/`
and `crates/psrs-backend/src/cc/lower/control/plan/model/`.

Evidence keys:

- **CC-root:** `root::cc_run_requires_its_fresh_root_empty_queue_ordered_close_and_result`.
- **CC-close:** `root::cc_run_cannot_check_instead_of_closing_its_root`.
- **Model-run:** `runs::run_composes_dispatch_callbacks_and_normal_root_completion`.
- **Model-negative:** `mutations::lifetime::run_cannot_omit_normal_root_cleanup_or_publish_an_unrelated_result`.
- **Source-runs:** `root::sequential_and_exclusive_runs_preserve_independent_invocations_after_optimization`.
- **Escape-runtime:** the required `control_lifetime` integration target rejects escaped raw/resume computations after a completed Run, including use from another Run.

| Historical path and test(s) | Current owner and evidence | Status / limit |
| --- | --- | --- |
| `mod.rs`: `cc_rejects_incorrect_root_dispatch_extent`, `mir_rejects_incorrect_root_dispatch_extent` | Run plan Root context and Executor operands; CC-root `DispatchRoot`; Model-run; optimized lifetime tests | Construction rejection restored. Arbitrary post-P9 mutation rejection is outside DEC-21. |
| `mod.rs`: `cc_rejects_incorrect_root_execution_extent`, `mir_rejects_incorrect_root_execution_extent` | Same Run plan; CC-root `ExecutionRoot`; Model-run | Construction rejection restored; ordinary transforms own preservation. |
| `mod.rs`: `cc_rejects_incorrect_root_queue`, `mir_rejects_incorrect_root_queue` | Run plan Empty; CC-root `NonemptyQueue` uses an IR-valid Append of two Empty queues | Construction rejection restored even for behaviorally empty non-Empty nodes. |
| `mod.rs`: `cc_rejects_incorrect_root_missing_close`, `mir_rejects_incorrect_root_missing_close` | Run plan ordered Close; CC-root `MissingClose`; Model-negative; Escape-runtime | Construction rejection and independent lifecycle behavior covered. |
| `mod.rs`: `cc_rejects_incorrect_root_wrong_close`, `mir_rejects_incorrect_root_wrong_close` | Run plan same-root operand; CC-root `CloseRoot`; Model-run | Construction rejection restored; no new post-P9 recognizer. |
| `mod.rs`: `cc_rejects_incorrect_root_early_close`, `mir_rejects_incorrect_root_early_close` | Run plan Executor-before-Close order; CC-root `EarlyClose`; required resume/lifetime runtime tests | Construction rejection restored; target tests require a live root during execution. |
| `mod.rs`: `cc_rejects_incorrect_root_check_instead_of_close`, `mir_rejects_incorrect_root_check_instead_of_close` | Run plan Close role; CC-close reproduces the original missing construction proof; Model-negative independently detects the same-signature substitution | Construction rejection restored. |
| `mod.rs`: `cc_rejects_incorrect_root_start_status`, `mir_rejects_incorrect_root_start_status` | Start plan's one `NewStatus`; `transforms::cc_start_rejects_a_status_cell_whose_length_is_not_one` rejects both an empty cell and a two-element cell at the P8 boundary | Construction rejection restored. The historical MIR mutation is not rechecked after P9. |
| `ownership.rs`: `cc_sequential_runs_cannot_share_a_root` | Run exposes no root argument; checked plan calls fresh Start inside each invocation. Model-run executes two complete invocations and checks fresh closed roots; Source-runs and Escape-runtime exercise optimized calls | Semantic ownership moved to the Run helper; the old caller-side operand substitution is no longer expressible through a source Run call. |
| `ownership.rs`: `mir_sequential_runs_cannot_share_a_root` | Same construction/transform split; Model-run, Source-runs, Escape-runtime | Post-P9 arbitrary semantic mutation rejection retired, not claimed as generic MIR coverage. |
| `ownership.rs`: `mir_sequential_runs_cannot_share_a_status_cell` | Start/Run fresh allocation; Model-run compares independent reference identities and each normal closed status across successive calls | Same trust boundary. Fresh status allocation behavior checked in the bounded model; runtime lifetime evidence is separate. |
| `ownership.rs`: `exclusive_run_branches_share_initialization_and_execute_independent_invocations` | Source-runs executes both branch choices after ordinary optimization. `transforms::pre_optimization_run_accepts_exclusive_sharing_distributed_close_and_copies` places one Start before two exclusive successors and accepts that pre-optimization MIR | The old caller-side CC hoist is no longer the Run shape. This fixture proves generic MIR acceptance of the shared-root CFG. It does not re-encode that CFG to Wasm. Optimized execution remains Source-runs. |
| `paths.rs`: `root_cleanup_can_be_distributed_across_normal_branches` | Same transform test repeats Close on both normal successors of pre-optimization Run and accepts the module | Positive CFG fixture restored at generic MIR verification. |
| `paths.rs`: `root_cleanup_cannot_be_skipped_on_a_normal_branch` | Run plan has one checked normal path with mandatory Close; CC-root `MissingClose`; Model-negative | Construction obligation covered. Generic MIR validity does not prove arbitrary normal-path cleanup. |
| `paths.rs`: `root_normal_cleanup_proof_excludes_trapping_paths` | Same transform test keeps Close on the normal successor and ends the other successor in `Trap`; generic MIR verification accepts it. Caught host traps retain the separate cleanup/reentry requirement | Positive CFG fixture restored. This mapping does not close caught-host-trap recovery. |
| `paths.rs`: `physical_root_and_close_accept_reference_copies` | Same transform test feeds Close a `Copy` of the root reference and accepts the module | Positive physical-copy fixture restored at generic MIR verification. |

The new CC-root `WrongResult` and Model-negative cases additionally protect Run's
actual returned result. They are not counted as historical tests restored by name.

## Lifecycle and observable paths: 6 historical tests

`control_regressions::lifecycle::cc_lifecycle_bodies_reject_identity_wrong_guard_write_and_ancestry`
and `control_regressions::observables` restore the two lifecycle and four
observable CC tests at the P8 boundary. Historical MIR body/effect recognizers
are not rechecked after P9.

## Checkpoint family: 11 historical tests

Evidence keys:

- **CC-perform:** `checkpoints::cc_perform_cannot_capture_the_execution_extent_as_its_checkpoint`.
- **CC-raw-check:** `checkpoints::cc_raw_resume_must_check_its_captured_checkpoint_before_the_frame`.
- **CC-resume-extent:** `checkpoints::cc_resume_cannot_check_the_current_execution_extent`.
- **CC-resume-order:** `checkpoints::cc_resume_cannot_start_replay_before_the_checkpoint_check`.
- **CC-alias:** `checkpoints::cc_raw_resume_accepts_a_same_shape_alias_of_its_checkpoint`.
- **CC-slot:** `control_boundary::callback_capture_index_must_match_the_plan`.
- **Model-perform:** `mutations::replay::perform_captures_the_dispatch_checkpoint_not_the_execution_extent`.
- **Model-resume:** `mutations::replay::resume_checks_the_saved_checkpoint_not_the_current_execution_extent`.
- **Model-raw:** `mutations::lifetime::raw_resume_checks_its_saved_checkpoint_before_publishing_a_frame`.

| Historical path and test(s) | Current owner and evidence | Status / limit |
| --- | --- | --- |
| `control_checkpoints.rs`: `cc_rejects_missing_raw_resumption_binding` | `cc/verify/ops/table.rs` still requires a valid raw resumption contract whenever helpers exist. `FrozenControlPlan` always stores that contract; freeze tests reject a missing helper role. Setting the old public field to `None` is no longer expressible | Construction/schema obligation retained. |
| `cc_rejects_capturing_execution_extent_as_dispatch_checkpoint`, `mir_rejects_capturing_execution_extent_as_dispatch_checkpoint` | CC-perform; Model-perform | Construction and bounded-model rejection restored. Post-P9 capture mutation is outside DEC-21. |
| `cc_rejects_a_raw_frame_checkpoint_from_another_capture_slot`, `mir_rejects_a_raw_frame_checkpoint_from_another_capture_slot` | CC-slot; plan check `capture_slots_and_product_indices_have_checked_domains` | Construction rejection restored. |
| `cc_rejects_a_raw_frame_built_without_its_checkpoint_check`, `mir_rejects_a_raw_frame_built_without_its_checkpoint_check` | CC-raw-check; Model-raw | Construction rejection restored. |
| `cc_rejects_replay_checking_current_extent_instead_of_saved_checkpoint`, `mir_rejects_replay_checking_current_extent_instead_of_saved_checkpoint` | CC-resume-extent; Model-resume | Construction rejection restored. |
| `checked_checkpoint_reference_aliases_survive_physical_lowering` | CC-alias; Start's accepted same-shape alias in `control_boundary` | Positive identity fixture restored at P8. |
| `control_checkpoint_ordering.rs`: `mir_rejects_a_correct_checkpoint_check_after_replay_has_started` | CC-resume-order | Construction rejection restored at CC. The historical MIR reorder is not a promised post-P9 rejection. |

## Context contracts and entry: 46 historical tests

Finish restoration, replay parents, Handle source-context fields and caller
copies are the same six named Scoped context slots and the three Finish fields.
Current owners:

- Handle fresh enter and Finish reuse: `plan/handle.rs`.
- Target Finish restore: `plan/returns.rs::enter_target` / `return_finish`.
- Replay parent/active/empty accumulators: `plan/helpers.rs` Clone/Rebuild and `plan/driver.rs::resume`.
- **CC-handle:** `checkpoints::cc_handle_cannot_install_its_fresh_extent_as_the_caller_parent`.
- **Model-handle:** `mutations::flows::handle_keeps_caller_and_owned_extents_distinct`.
- **Model-finish:** `mutations::flows::target_finish_restores_its_caller_extent_not_its_owned_extent`.
- **Model-parent:** `mutations::replay::replay_parents_fresh_activations_to_dispatch_not_execution`.
- Positive copies: `control_context_contracts/aliases` and `control_context_entry/aliases` at `01f7e66c` become the accepted CC alias in CC-alias and the MIR copy in `transforms`.

Paired CC/MIR field substitutions of the same slot are one construction
obligation. Post-P9 copies of that mutation are outside DEC-21. The
`mir_source_context_status_is_allocated_inside_the_executor_loop` shape is the
straight-line Handle plan: `FreshExtent` runs inside the executor, then the
ordinary inliner may move it. Loop-header allocation is a P9/P10 layout fact,
not a second machine proof.

## Replay family: 20 historical tests

Clone/Rebuild plans own saved-extent Check, fresh child extents, empty
accumulators, shared callbacks and the rebuilt queue/active pair. Current
owners:

- **Model-reuse:** `replay_must_not_reuse_a_saved_extent_as_a_fresh_activation`.
- **Model-result:** `replay_result_keeps_the_rebuilt_queue_and_matching_activation`.
- **Model-share / left queue:** `clone_cannot_drop_the_left_queue_when_pending_right_work_exists`.
- **Behavior:** `behavior::bounded_replay_preserves_order_sharing_freshness_and_three_extent_roles` (1,805 depth-three queues).
- **CC helper arguments:** `control_boundary::helper_arguments_must_preserve_source_identity` and `dispatch_selector_must_preserve_source_identity` on CloneSequence.

Missing Check, reused activation, wrong parent, live new status, callback
sharing and completed saved extents are those plan operations. Historical MIR
`status_allocation_before_the_rebuild_loop` is a loopification layout; Rebuild
allocates `FreshExtent` on the Finish arm of the helper, then P9 may loopify
self tails. Allocation-before-dispatch (`allocation.rs`) is ordinary CC before
the machine switch; the P8 boundary already rejects extra operations before
dispatch in `control_regressions::observables`.

## Inlined publication and owned cells: 9 historical tests

Start and Once plans allocate a one-element live status cell.
`checkpoints::cc_start_and_once_status_cells_must_be_initialized_live` rejects
writing status 2 at construction. Cancel's ordered Pending/Cancelled/clear is
`plan/helpers.rs::cancel`, `control_boundary` status-write tests, and
`mutations::lifetime` cancellation/phase tests. Owned transfer is
`plan/handle.rs` OwnedResume plus those lifetime mutations.

`control_inlined_publication::inlined_start_and_once_check_current_contents_at_publication`
mutated optimized MIR cells before a publication witness. Under DEC-21 that
write is a trusted-transform obligation, not a MIR recognizer. The construction
initializer and the bounded model cover the factory contents. A post-P9 spent
cell is not a promised generic MIR rejection.

## Dispatch, clauses, Bind/Resume and Finish: 36 historical tests

Evidence keys:

- **CC-default:** `steps::cc_executor_dispatch_default_must_trap`.
- **CC-result:** `steps::cc_executor_cannot_return_state_instead_of_the_dispatch_result`.
- **CC-bind-body:** `steps::cc_bind_cannot_reenter_the_current_computation`.
- **CC-bind-callback:** `steps::cc_bind_cannot_replace_its_callback_with_the_root_handler`.
- **CC-resume-active:** `steps::cc_resume_cannot_replace_replay_activation_with_the_current_extent`.
- **CC-finish-close:** `steps::cc_finish_cannot_close_the_parent_instead_of_its_extent`.
- **CC-finish-parent:** `steps::cc_finish_cannot_restore_the_closed_extent_as_its_parent`.
- **CC-finish-role:** `steps::cc_finish_cannot_exchange_close_for_check`.
- **CC-clause-result:** `steps::cc_scoped_return_cannot_discard_the_clause_result`.
- **CC-clause-field:** `control_boundary::projected_clause_field_must_match_the_plan`.
- **CC-clause-arg:** `control_boundary::clause_application_must_consume_its_planned_argument`.
- **CC-arm:** `control_carriers::cc_boundary_rejects_a_dropped_machine_dispatch_arm`.
- **MIR-arm:** `control_carriers::mir_boundary_rejects_a_dropped_machine_dispatch_arm`.
- **Plan-arm:** `plan/check/tests.rs::every_required_action_is_checked_including_helpers_and_rejection_arms`.
- **Model-bind:** `mutations::flows::bind_enters_its_inner_computation_instead_of_reentering_itself`.
- **Model-resume:** `mutations::replay::resume_appends_captured_work_before_consumer_work`.
- **Model-clause:** `mutations::flows::swapped_same_shaped_clauses_do_not_pass_the_behavioral_model`.
- **Model-finish:** `mutations::flows::finish_closes_its_owned_extent_not_its_restore_parent` and `target_finish_restores_its_caller_extent_not_its_owned_extent`.

| Historical family | Current owner and evidence | Status / limit |
| --- | --- | --- |
| Dispatch coverage (cc/mir missing alternative) | CC-arm, MIR-arm, Plan-arm | Construction and MIR dispatch-set rejection restored. |
| Wrong projection / trapping default / result origin | CC-default, CC-result; `projections_require_the_selected_case_of_their_own_source`; `dispatch_selector_must_preserve_source_identity` | Construction rejection restored. Immutable i32 tags are ordinary MIR variant verification. |
| Dispatch alias/copy of the selector | CC-alias (checkpoint file) and `control_boundary::reference_alias_on_the_actual_machine_exit_is_accepted` | Positive identity restored at P8. |
| Clause selection return vs operation field | CC-clause-field; Model-clause | Construction and model rejection restored. |
| Clause result, payload, raw resumption, state/handler | CC-clause-result, CC-clause-arg; Perform's ResumeClosure plan | Construction restored for result and payload. Dead-block MIR calls are ordinary CFG; they are not a second machine proof. |
| Bind body/callback/queue | CC-bind-body, CC-bind-callback; Model-bind | Construction restored. |
| Resume answer, replayed extent, append tail | CC-resume-active; Model-resume; `replay_projection_index_must_match_the_plan` | Construction restored for activation. Append order is the model mutation. |
| Finish close/restore/parent/Check-as-Close | CC-finish-close, CC-finish-parent, CC-finish-role; Model-finish | Construction restored. Optimized MIR copies of those mutations are outside DEC-21. |
| Finish reference aliases | `transforms` physical copy and CC-alias | Positive identity restored. |

## Helper contracts, queue, result, carriers, costs and source contracts: 33 historical tests

| Historical family | Current owner and evidence | Status / limit |
| --- | --- | --- |
| Exchange Check/Close; missing helper contract | `steps::cc_finish_cannot_exchange_close_for_check`; `steps::cc_helper_call_cannot_drop_its_semantic_contract`; `RepresentationTable::verify_control_call`; `mir/control/contracts.rs` still binds helper symbols on calls | Construction and ordinary call-contract verification retained. Long-request execution without tail-call capability remains the `control_helper_contracts` runtime case at `01f7e66c`; required integration targets still execute long request chains. |
| Queue same-shaped substitutions; placed schema | Plan inventory, boundary operand checks, Model queue mutations, `control/schema.rs` | Construction restored. |
| Extra writes/traps; unused narrowing; failed-guard prefix | `control_regressions::observables`; `control_boundary` extra-store and unused-cast tests | Construction restored. Historical MIR path injection is outside DEC-21. |
| Ordered root callbacks | Required `control_runtime` / scoped runtime targets; model Bind/Cons order | Optimized execution retained. This pass did not re-run those targets. |
| Result provenance and equivalent joins | CC-result; root `WrongResult`; `control_boundary` returned-value and accepted alias tests | Construction restored. Equivalent If/switch joins are ordinary CC reference identity. |
| Wrong machine constructor tag | Ordinary MIR variant verification (`mir/verify/variants`) | Not a machine recognizer. |
| Eight operations, join carriers, weakening to aggregate | Live `control_carriers.rs` | Still present. |
| Ancestry/GC cost probes and malformed pipeline | Live `control_costs/` including `replay.rs` | Still present. §14 records saved-length observations. |
| Dangling source contract | `steps::cc_control_evidence_rejects_a_dangling_contract`; `cc/verify/ops/control.rs` | Ordinary CC evidence verification retained. |
| Same-shape body from the target domain; unsaved source body | Core source/target typing plus `ControlEvidence` operand identities in `cc/verify/ops/control.rs` | Source-domain proof. Not re-expressed as a post-erasure machine recognizer. |

## Continuation

Every name in the 185-test inventory now has a construction, ordinary IR, model,
runtime or explicit DEC-21-limit owner. Gates 4 and 6 are an audit of that
assignment; they are not closed by the new test count. Foreign capture,
caught-host-trap cleanup/reentry, pinned reference-library comparisons and
complete AE/AC acceptance remain unchanged.
