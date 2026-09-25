# Control Flow and Tail Calls Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Control flow and tail calls](../../design/backend/fp/control-flow-and-tail-calls.md)

**Progress:** Audited. CF-03, CF-04, CF-05, CF-12, and CF-13 (the merge-hint
removal) are Verified. CF-01, CF-02, CF-09, and CF-10 are In progress with the
gaps recorded below. CF-06, CF-07, CF-08, and CF-11 are Blocked on tail-call
lowering; see the precise remaining work.

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
| CF-01 | MIR terminators model Return, Jump, Branch, Switch, ReturnCall, and ReturnCallRef without a merge hint. | Inspect constructors and all producers/consumers; malformed old-style or missing-edge fixtures fail, and ordinary joins derive from CFG. | In progress |
| CF-02 | CFG analysis computes reachability, dominators, backedges, natural loops, nesting, and joins from edges. | Diamond, nested-loop, multiple-exit, dead-block, and invalid graph fixtures assert computed regions and diagnostic locations. | In progress |
| CF-03 | The structurer emits `block`/`loop`/`if` with valid depth-relative branches for every reducible CFG. | Validate and execute diamonds, loop-carried parameters, nested loops, early exits, and shared continuation cases; assert no dispatcher fallback on reducible input. | Verified |
| CF-04 | Irreducible CFGs use the specified dispatcher fallback with preserved block parameters and edge semantics. | Direct MIR irreducible fixtures validate and execute through Wasm; inspect selector dispatch and compare results with a CFG interpreter or equivalent oracle. | Verified |
| CF-05 | `Switch` preserves unique case tags, signed selector normalization, sparse/default behavior, and target block arguments. | Source and direct MIR fixtures cover dense nullary tags (`br_table`), sparse/negative tags, duplicate tags, defaults, and malformed target signatures. | Verified |
| CF-06 | Self-tail recursion evaluates new arguments first and becomes a loop with correctly carried parameters and constant stack use. | Source or verified Core recursion with permutation/side effects; inspect MIR/Wasm loop and execute deep recursion beyond ordinary call-stack depth. | Blocked |
| CF-07 | Other direct and reference tail calls become ReturnCall/ReturnCallRef when the target enables tail calls. | Inspect exact call signatures and emitted Wasm opcodes; execute mutual and indirect tail recursion with value-sensitive results. | Blocked |
| CF-08 | A target without tail-call support lowers the same semantics to ordinary call plus return and emits no tail-call opcode. | Compile equivalent fixtures under both profiles, validate bytes and execute shallow cases; assert disabled capability rejection only for unsupported forced operations. | Blocked |
| CF-09 | MIR verification checks tail position, callee signature, return type, branch target arguments, dominance, and switch uniqueness. | Full-module negative fixtures for bad tail signatures, non-dominating call operands, wrong edge values, duplicate cases, and invalid selector types. | In progress |
| CF-10 | P10 and Wasm lowering preserve CFG and tail-call semantics, traps, spans, and enabled feature profile. | Compare optimized/unoptimized results on loops, switches, recursive calls, and traps; validate emitted module/component with the selected profile. | In progress |
| CF-11 | In-scope source case/recursion paths reach structured Wasm; direct MIR fixtures remain identified as such. | Track each executable input boundary and test the normal component pipeline with mandatory Wasmtime, including a deep recursion case. | Blocked |
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
  Implementation: mir/mod.rs Terminator now has Return, Jump, Branch, Switch;
    ReturnCall and ReturnCallRef are still absent.
  Tests: returns/branches/switches covered by the CF-13 and CF-03/CF-05
    evidence; no test can reference a tail-call terminator yet.
  Input boundary: direct MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass for the terminators that exist; the tail-call terminators are
    not implemented.
  Revision: 775fafee5c755afab141e0e3265287aa0b7ec9e7 plus the listed working-tree
    changes.
  Gaps: ReturnCall/ReturnCallRef terminator variants, their verifier rules, and
    their emission are blocked on CF-06/CF-07; the merge-hint part is complete
    under CF-13.
```

```text
CF-13:
  Implementation: crates/psrs-backend/src/mir/mod.rs (Terminator::Branch has no
    merge_block); mir/lower/assignments.rs, mir/lower/aggregate/array.rs,
    mir/scalar_helpers.rs (producers no longer set it); mir/cfg.rs
    (common_join/join_blocks derive joins from CFG edges); mir/verify/function.rs
    (merge check removed, target-parameter check kept); mir/opt/constants.rs
    (join parameters preserved); wasm/lower/structure/legacy.rs (derives the
    diamond/switch join).
  Tests: mir/cfg.rs::tests::{derives_a_diamond_join_from_the_edges,
    derives_the_nearest_common_join,
    reports_no_join_when_arms_terminate_independently};
    wasm::lower::structure::tests::rejects_an_acyclic_branch_without_a_common_join
    (missing-edge fixture fails with "no common one-value join");
    driver functions.rs::runs_a_branch_with_equal_reference_arms executes a
    derived join under Wasmtime.
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
    lowers_nested_loops_with_multiple_exit_targets (nesting + multiple exits).
  Input boundary: direct MIR.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend.
  Result: pass.
  Revision: as above.
  Gaps: no fixture yet asserts a diagnostic span for an invalid graph whose
    retreating edges are not backedges; the reducible/dispatcher split is
    covered, the malformed-loop-nesting diagnostic is not.
```

```text
CF-03:
  Implementation: wasm/lower/structure/region.rs, cfg/mod.rs; acyclic diamonds
    use the derived join and a result-typed `if` in legacy.rs; the dispatcher is
    selected only when cfg::has_irreducible_component is true.
  Tests: structure::tests::
    lowers_a_natural_loop_with_a_preheader_and_loop_carried_values asserts
    lowered.locals.len() == values.len() - parameters.len() (no dispatcher state
    local) and executes to 15 under Wasmtime;
    lowers_nested_loops_with_multiple_exit_targets asserts two loops;
    switch_tests::structures_sparse_negative_switch_tags_and_executes_the_default
    executes a reducible diamond/switch; driver functions.rs runs source
    diamonds and cases.
  Input boundary: direct MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend structure;
    PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass; Wasmtime executed the loop, nested-loop, sparse-switch, and
    source diamond cases.
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
  Implementation: legacy.rs::switch_index and region.rs::switch_index (compare
    each sparse signed tag, map to a dense index); Switch verification in
    mir/verify/function.rs; Jump argument/type checks.
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
CF-09:
  Implementation: mir/verify/function.rs (dominance, Jump argument count/type,
    Branch condition and parameterless targets, Switch selector/unique cases/
    targets); mir/verify/instruction/* for instruction typing.
  Tests: verify::tests::{rejects_values_used_before_definition,
    rejects_duplicate_switch_case_values, rejects_a_switch_with_a_non_i32_selector,
    rejects_a_branch_target_with_block_parameters};
    mir/verify tests for wrong jump argument counts are exercised by the
    optimizer/inline suites.
  Input boundary: malformed MIR.
  Commands: cargo test -p psrs-backend mir::verify.
  Result: pass.
  Revision: as above.
  Gaps: tail-position, callee-signature, and return-type checks for
    ReturnCall/ReturnCallRef do not exist because the terminators do not exist
    (blocked on CF-06/CF-07).
```

```text
CF-10:
  Implementation: mir/opt (reachability, constants, copy forwarding, inlining,
    effects), wasm/lower, capability wasm_features gating.
  Tests: mir::opt::tests::control_flow (fold/prune/project imports, switch
    preservation); wasm/lower/structure tests (loops, switches, traps);
    driver tests run optimized artifacts under Wasmtime; the trap-aware
    structurer keeps spans on dead trap arms (driver
    functions.rs::runs_a_case_that_returns_a_reference).
  Input boundary: source and direct MIR.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass under the default capability profile.
  Revision: as above.
  Gaps: tail-call semantics and an enabled `tail_call` profile cannot be
    compared because tail-call lowering is blocked.
```

## Remaining work and blockers

CF-13 (merge-hint removal) is complete and verified. CF-03, CF-04, CF-05, and
CF-12 are verified. CF-01, CF-02, CF-09, and CF-10 are verified only for the
parts that exist today; their remaining gaps are listed above.

CF-06, CF-07, CF-08, and CF-11 remain blocked on the tail-call lowering that the
design specifies but that is not implemented. The precise remaining work is:

1. Add `ReturnCall { function: SymbolId, arguments, span }` and
   `ReturnCallRef { function: ValueId, type_index, arguments, span }` to
   `mir::Terminator`, and extend `mir/cfg.rs::successors`, the verifier's
   operand/type checks, the optimizer's successors, and the structurer's
   terminator emission (`region.rs`, `legacy.rs`, `dispatcher.rs`).
2. Implement `mir/lower/tail.rs::mark_tail(function, target)`, invoked from
   `mir/lower` before verification. Tail position must be computed over the
   real lowered CFG, not a single block: the CC lowerer emits a recursive call
   in an `if` arm that passes its result through a one-parameter join that then
   `Return`s (see the `sum` MIR dump), so a call is in tail position when its
   destination follows a chain of single-argument `Jump`s through one-parameter
   blocks to the function's `Return`. That chain is what the design's
   single-block pseudocode omits.
3. Self-tail loopification: add a fresh preheader, add block parameters to the
   function entry that mirror the function parameters, rewrite uses of the
   function parameters to the new block parameters (needs a new
   `Instruction::map_operands`), and replace each self tail call with a `Jump`
   to the header carrying the new arguments. This makes deep self-recursion
   constant-stack on the default, tail-call-disabled profile.
4. Non-self direct and reference tail calls become `ReturnCall`/`ReturnCallRef`
   only when `TargetCapabilities::tail_call` is set; otherwise leave the
   ordinary call plus return. The verifier must reject a hand-written
   `ReturnCall*` when the profile disables the proposal (forced operation), and
   `wasm/lower/structure` must encode `return_call`/`return_call_ref`.
5. Add Wasmtime tests with `PSRS_REQUIRE_WASMTIME=1`: a deep self-recursion
   fixture (beyond the ordinary call-stack depth, e.g. `sum 100000 0`), a direct
   mutual tail-recursion pair, and an indirect tail recursion through a typed
   function reference, plus a disabled-profile fixture that asserts the emitted
   WAT contains no `return_call*` opcode.

