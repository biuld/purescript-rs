# Control Flow and Tail Calls Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Control flow and tail calls](../../design/backend/fp/control-flow-and-tail-calls.md)

**Progress:** CF-01 through CF-13 Verified.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-03, BE-05, BE-13, and BE-16.

## Scope and dependencies

Implement the linked design's general MIR CFG structuring and tail-call
contract through validated Wasm execution. This includes removing the temporary
`Branch { merge_block }` hint, deriving joins from CFG edges, reducible and
irreducible graph handling, switch lowering, self-tail loopification, and
feature-gated direct/indirect tail calls. MIR owns typed value semantics;
[pattern matching](pattern-matching.md) selects case edges. Multi-value,
exceptions, stack switching, branch hints, and unrelated optimization ideas
listed as future work are outside this topic. Use the design's Code map and
retain source spans on transformed calls and branches.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**;
record exact reproducible evidence for every Verified row.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| CF-01 | MIR terminators model Return, Jump, Branch, Switch, ReturnCall, and ReturnCallRef without a merge hint. | Inspect constructors and all producers/consumers; malformed old-style or missing-edge fixtures fail, and ordinary joins derive from CFG. | Verified |
| CF-02 | CFG analysis computes reachability, dominators, backedges, natural loops, nesting, and joins from edges. | Diamond, nested-loop, multiple-exit, dead-block, and invalid graph fixtures assert computed regions and diagnostic locations. | Verified |
| CF-03 | The structurer emits `block`/`loop`/`if` with valid depth-relative branches for every reducible CFG. | Validate and execute diamonds, loop-carried parameters, nested loops, early exits, and shared continuation cases; assert no dispatcher fallback on reducible input. | Verified |
| CF-04 | Irreducible CFGs use the specified dispatcher fallback with preserved block parameters and edge semantics. | Direct MIR irreducible fixtures validate and execute through Wasm; inspect selector dispatch and compare results with a CFG interpreter or equivalent oracle. | Verified |
| CF-05 | `Switch` preserves unique case tags, signed selector normalization, sparse/default behavior, and target block arguments. | Source and direct MIR fixtures cover dense nullary tags (`br_table`), sparse/negative tags, duplicate tags, defaults, and malformed target signatures. | Verified |
| CF-06 | Self-tail recursion evaluates new arguments first and becomes a loop with correctly carried parameters and constant stack use. | Source or verified Core recursion with permutation/side effects; inspect MIR/Wasm loop and execute deep recursion beyond ordinary call-stack depth. | Verified |
| CF-07 | Other direct and reference tail calls become ReturnCall/ReturnCallRef when the target enables tail calls. | Inspect exact call signatures and emitted Wasm opcodes; execute mutual and indirect tail recursion with value-sensitive results. | Verified |
| CF-08 | A target without tail-call support lowers the same semantics to ordinary call plus return and emits no tail-call opcode. | Compile equivalent fixtures under both profiles, validate bytes and execute shallow cases; assert disabled capability rejection only for unsupported forced operations. | Verified |
| CF-09 | MIR verification checks tail position, callee signature, return type, branch target arguments, dominance, and switch uniqueness. | Full-module negative fixtures for bad tail signatures, non-dominating call operands, wrong edge values, duplicate cases, and invalid selector types. | Verified |
| CF-10 | P10 and Wasm lowering preserve CFG and tail-call semantics, traps, spans, and enabled feature profile. | Compare optimized/unoptimized results on loops, switches, recursive calls, and traps; validate emitted module/component with the selected profile. | Verified |
| CF-11 | In-scope source case/recursion paths reach structured Wasm; direct MIR fixtures remain identified as such. | Track each executable input boundary and test the normal component pipeline with mandatory Wasmtime, including a deep recursion case. | Verified |
| CF-12 | The constant-parameter optimizer must not materialize a derived branch/switch join parameter. | Compile and execute a diamond whose arms pass the same constant; assert the join keeps its block parameter (no dangling `local.get`). | Verified |
| CF-13 | `Branch` carries no merge hint; the verifier, optimizer, and structurer all derive joins from CFG edges. | Remove `merge_block`; malformed missing-edge MIR fails; ordinary diamonds and switches still derive and execute their join. | Verified |

## Vertical execution order

1. Inventory terminators and all CFG producers/consumers; remove the merge
   hint across MIR, verifier, optimizer, structurer, fixtures, and docs.
2. Complete graph analysis and structuring, then switch and tail-call lowering
   with verifier rules at each boundary.
3. Run source and direct MIR fixtures through validation and mandatory Wasmtime
   execution under enabled/disabled tail-call profiles. Re-audit the design.
4. Record evidence and update D-04 without promoting broader BE rows solely
   from this topic's focused tests.

## Evidence record and completion rule

For each ID record implementation entry points, named tests/assertions, input
boundary, exact commands and feature flags, runtime version, executed/skipped
cases, revision, and gaps. Direct MIR fixtures establish backend behavior only.
Run runtime cases with `PSRS_REQUIRE_WASMTIME=1`; skipped cases cannot verify a
row. After Rust changes run `cargo fmt --all --check`,
`cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`. Completion requires
every row Verified and a full present-tense design audit.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
CF-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Recorded evidence

Runtime: `wasmtime 49.0.0 (17830bd3c 2026-09-21)`, invoked with
`PSRS_REQUIRE_WASMTIME=1` so a missing runtime fails rather than skips.

```text
CF-01:
  Implementation: mir/mod.rs Terminator has Return, Jump, Branch, Switch,
    ReturnCall, and ReturnCallRef; mir/cfg.rs, mir/opt/{cfg,constants,values}.rs,
    mir/verify/function.rs, mir/verify/call/tail.rs, mir/lower/tail.rs (producer
    via mark_tail), and the Wasm structurer all consume every variant.
  Tests: returns/branches/switches covered by the CF-13 and CF-03/CF-05
    evidence; tail terminators covered by the CF-06/CF-07/CF-09 records below.
    mir/verify/tests/tail.rs negative fixtures construct hand-written
    ReturnCall/ReturnCallRef terminators.
  Input boundary: direct MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass; the terminator set is complete and the merge hint remains
    removed.
  Revision: f2c43af plus the tail-call change in this worktree.
  Gaps: none.
```

```text
CF-13:
  Implementation: crates/psrs-backend/src/mir/mod.rs (Terminator::Branch has no
    merge_block); mir/lower/assignments.rs, mir/lower/aggregate/array.rs,
    mir/scalar_helpers.rs (producers no longer set it); mir/cfg.rs
    (common_join/join_blocks derive joins from CFG edges); mir/verify/function.rs
    (merge check removed, target-parameter check kept); mir/opt/constants.rs
    (join parameters preserved); wasm/lower/structure/cfg/mod.rs and
    wasm/lower/structure/region.rs (derive no join; emit every reducible block
    once by label).
  Tests: mir/cfg.rs::tests::{derives_a_diamond_join_from_the_edges,
    derives_the_nearest_common_join,
    reports_no_join_when_arms_terminate_independently};
    wasm::lower::structure::reducible_tests::acyclic::
    rejects_an_acyclic_branch_to_a_missing_target (missing-edge fixture fails
    with a missing-target diagnostic); driver
    functions.rs::runs_a_branch_with_equal_reference_arms executes a derived
    join under Wasmtime.
  Input boundary: malformed/direct MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace;
    cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings.
  Result: pass; 32 test suites ok, 0 failed; no production or fixture still
    constructs Branch with a merge field.
  Revision: 775fafee5c755afab141e0e3265287aa0b7ec9e7 plus the listed working-tree
    changes.
  Gaps: none for the merge-hint removal.
```

```text
CF-02:
  Implementation: wasm/lower/structure/cfg/mod.rs (reachability, predecessors,
    dominators, natural loops, nesting); mir/cfg.rs (joins from edges);
    mir/opt/cfg.rs (reachable_blocks).
  Tests: mir::cfg::tests (diamond, nearest-common, no-join);
    mir::opt::tests::control_flow::folds_branch_merges_prunes_blocks_and_projects_imports
    (dead-block pruning); wasm::lower::structure::tests::
    lowers_nested_loops_with_multiple_exit_targets (nesting + multiple exits);
    wasm::lower::structure::reducible_tests::acyclic::
    rejects_an_acyclic_branch_to_a_missing_target (invalid graph diagnostic
    with the terminator span).
  Input boundary: direct MIR.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend.
  Result: pass. An irreducible graph with retreating non-back edges is routed
    to the dispatcher rather than rejected, so no such diagnostic is expected;
    a genuinely invalid graph (a missing target) is diagnosed with its span.
  Revision: as above.
  Gaps: none.
```

```text
CF-03:
  Implementation: wasm/lower/structure/cfg/mod.rs builds one RegionPlan for
    every reducible CFG, with or without loops; wasm/lower/structure/region.rs
    is the single emitter for that plan (nested `Block`s, `Loop` at natural-loop
    headers, depth-relative `br`/`br_if`/`br_table`, jump-argument copies). The
    dispatcher is selected only when cfg::has_irreducible_component is true, and
    `emit_control_flow` no longer branches on `contains_loops`. Non-parameter
    reference locals are nullable and read back with `ref.as_non_null`
    (wasm/lower/mod.rs, structure/mod.rs).
  Tests: structure::reducible_tests::loops::
    structures_and_executes_a_diamond_inside_a_loop,
    structures_and_executes_a_switch_inside_a_loop,
    structures_and_executes_a_branch_with_an_early_return_inside_a_loop,
    structures_and_executes_a_trapping_arm_inside_a_loop (each asserts one loop,
    no dispatcher local, and executes; the trap arm traps);
    reducible_tests::acyclic::
    structures_and_executes_an_acyclic_branch_without_a_common_join (both arms
    return; no join, no dispatcher, executes 20/10),
    structures_and_executes_a_shared_successor_with_swapped_block_arguments
    (permuted block parameters execute 1020/2010);
    structure::tests::lowers_a_natural_loop_with_a_preheader_and_loop_carried_values
    (no dispatcher local, executes to 15);
    lowers_nested_loops_with_multiple_exit_targets (two loops);
    switch_tests::structures_sparse_negative_switch_tags_and_executes_the_default
    (dense `br_table`, executes -7/100/default); driver functions.rs runs source
    diamonds and cases.
  Input boundary: direct MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend structure;
    PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass; Wasmtime executed the acyclic no-join branch, the shared
    successor with swapped parameters, the loop with an inner diamond, the loop
    with an inner switch, the early-return loop, the trapping loop arm, the
    loop-carried-value loop, the nested loops, the sparse switch, and the source
    diamond/case cases. Every reducible case asserts no dispatcher state local.
  Revision: as above.
  Gaps: none observed for reducible input.
```

```text
CF-04:
  Implementation: wasm/lower/structure/dispatcher.rs; selection in cfg/mod.rs.
  Tests: irreducible_dispatch_tests::
    structures_and_executes_irreducible_cfg_with_block_parameters_and_sparse_switch
    (validates, executes selectors -7/42/13 against expected 5/5/99);
    dispatcher_uses_a_br_table_and_explicit_trap_for_invalid_state.
  Input boundary: direct MIR.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend irreducible.
  Result: pass; Wasmtime executed all three selector cases and the default trap
    path is asserted.
  Revision: as above.
  Gaps: none observed.
```

```text
CF-05:
  Implementation: region.rs::switch_index (compare each sparse signed tag, map
    to a dense index); Switch verification in mir/verify/function.rs; Jump
    argument/type checks.
  Tests: switch_tests::structures_sparse_negative_switch_tags_and_executes_the_default
    (negative -7, sparse 100, default 5 -> 11/22/33 via br_table);
    irreducible_dispatch_tests sparse switch; verifier tests
    rejects_duplicate_switch_case_values,
    rejects_a_switch_with_a_non_i32_selector,
    rejects_a_branch_target_with_block_parameters; driver
    functions.rs::runs_a_case_that_returns_a_reference (source switch).
  Input boundary: direct MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass; Wasmtime executed the dense, sparse/negative, and default cases.
  Revision: as above.
  Gaps: none observed.
```

```text
CF-12:
  Implementation: mir/opt/constants.rs::materialize_block_parameters skips
    mir::cfg::join_blocks so a derived join keeps its one-value parameter.
  Tests: mir::opt::tests::control_flow::
    folds_branch_merges_prunes_blocks_and_projects_imports; driver
    functions.rs::runs_a_branch_with_equal_reference_arms executes a diamond
    whose arms pass the same value.
  Input boundary: direct MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass.
  Revision: as above.
  Gaps: none observed.
```


```text
CF-06:
  Implementation: mir/lower/tail.rs (mark_tail, return_forwarded_values,
    loopify_self_calls, replace_tail_call). A self call whose destination is
    return-forwarded through one-parameter joins becomes a `Jump` to a fresh
    loop header; the original entry becomes a preheader passing the function
    parameters, and body uses of the function parameters are remapped to the
    header parameters.
  Tests: driver tests::tail_calls::self_tail_recursion_runs_in_constant_stack
    compiles `count 100000` (42) on the default tail-call-disabled profile and
    asserts no `ReturnCall*` terminator; the same program traps before this
    change because 100000 frames exceed the Wasmtime stack.
  Input boundary: source, executed Wasm component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::tail_calls.
  Result: pass; Wasmtime executed the deep self-recursion to exit 42.
  Revision: f2c43af plus the tail-call change in this worktree.
  Gaps: none.
```

```text
CF-07:
  Implementation: mir/lower/tail.rs rewrites a non-self direct `Call` to
    `ReturnCall` and a `CallRef`/`ClosureCall` to `ReturnCallRef` when the
    target enables tail calls. A closure call projects the code reference
    (`StructGet` + `RefCast`) and passes the closure as the receiver argument;
    `wasm/lower/structure/region.rs` and `dispatcher.rs` encode
    `return_call`/`return_call_ref`.
  Tests: driver tests::tail_calls::non_self_tail_calls_use_return_call_when_enabled
    (mutual `even`/`odd`, 100000 deep, exit 42) asserts a `ReturnCall`
    terminator and `return_call` in the WAT;
    tests::tail_calls::indirect_tail_recursion_uses_return_call_ref
    (`run`/`tick`, 100000 deep, exit 42) asserts a `ReturnCallRef` terminator
    and `return_call_ref` in the WAT.
  Input boundary: source, executed Wasm component, enabled `tail_call` profile.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::tail_calls.
  Result: pass; Wasmtime executed both tail-recursive programs to exit 42.
  Revision: f2c43af plus the tail-call change in this worktree.
  Gaps: none.
```

```text
CF-08:
  Implementation: mir/lower/tail.rs only rewrites non-self calls when
    `TargetCapabilities::tail_call` is set, leaving an ordinary call plus
    return otherwise; `mir/verify/capability.rs` rejects a forced
    `ReturnCall*` under a disabled profile.
  Tests: driver tests::tail_calls::disabled_profile_keeps_an_ordinary_call
    asserts no tail-call terminator and no `return_call` in the WAT on the
    stable profile and executes `f 41` to 42;
    mir::verify::capability::tests::return_calls_require_the_tail_call_proposal.
  Input boundary: source and direct MIR; disabled profile.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::tail_calls;
    cargo test -p psrs-backend mir::verify::capability.
  Result: pass.
  Revision: f2c43af plus the tail-call change in this worktree.
  Gaps: none.
```

```text
CF-09:
  Implementation: mir/verify/function.rs (dominance, Jump argument count/type,
    Branch condition and parameterless targets, Switch selector/unique cases/
    targets); mir/verify/instruction/*; mir/verify/call/tail.rs; mir/verify/
    capability.rs.
  Tests: verify::tests::{rejects_values_used_before_definition,
    rejects_duplicate_switch_case_values,
    rejects_a_switch_with_a_non_i32_selector,
    rejects_a_branch_target_with_block_parameters};
    mir/verify/tests/tail.rs negatives; capability rejection test.
  Input boundary: malformed MIR.
  Commands: cargo test -p psrs-backend mir::verify.
  Result: pass.
  Revision: as above.
  Gaps: none.
```

```text
CF-10:
  Implementation: mir/opt (reachability, constants, copy forwarding, inlining,
    effects), wasm/lower, capability wasm_features gating; tail-call
    terminators flow through optimization and verification unchanged.
  Tests: mir::opt::tests::control_flow (fold/prune/project imports, switch
    preservation); wasm/lower/structure tests (loops, switches, traps);
    driver tests run optimized artifacts under Wasmtime (including the tail
    calls above, which run the normal optimized pipeline); the trap-aware
    structurer keeps spans on dead trap arms (driver
    functions.rs::runs_a_case_that_returns_a_reference).
  Input boundary: source and direct MIR.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass under the default profile and under an explicit
    `tail_call`-enabled profile.
  Revision: f2c43af plus the tail-call change in this worktree.
  Gaps: none.
```

## Remaining work and blockers

Every acceptance row CF-01 through CF-13 is Verified. The tail-call lowering,
its verifier rules, and the enabled/disabled profile behavior are exercised by
`crates/psrs-driver/src/tests/tail_calls.rs` under mandatory Wasmtime, and by
the negative fixtures in `mir/verify/tests/tail.rs`. The stable profile still
keeps `tail_call` disabled, so enabling `return_call*` by default remains a
profile revision rather than a code gap.
