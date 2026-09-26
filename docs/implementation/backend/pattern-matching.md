# Pattern Matching Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Pattern matching](../../design/backend/fp/pattern-matching.md)

**Progress:** PM-01..PM-13 Verified after a full design audit and the addition of
a first-match oracle, negative CC/MIR fixtures, and mandatory Wasmtime execution
evidence.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-05 and BE-06, with BE-08 and BE-09 for generic fields.

## Scope and dependencies

Complete the linked design's pattern matrix, decision DAG, coverage analysis,
and CC realization for patterns expressible by verified Typed Core. The design
is authoritative beyond this matrix. Typed pattern construction and source
syntax belong to the frontend; [data representation](data-representation.md)
owns constructor layouts; [generic aggregate erasure](generic-aggregate-erasure.md)
owns recovery of dependent aggregate fields; [control flow and tail calls](control-flow-and-tail-calls.md)
owns MIR/Wasm switch structuring. Track source and backend fixture evidence
separately. Literal, guard, view, tuple, array, as, and or patterns listed as
future work are not silently promoted to present source support.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. Each
Verified row needs exact implementation, test and execution evidence.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| PM-01 | A case evaluates each scrutinee exactly once and preserves left-to-right, top-to-bottom first-match order. | Source and verified Core cases with side effects or traps, duplicate constructor rows, and an irrefutable first row; inspect CC evaluation order and execute outcomes. | Verified |
| PM-02 | Matrix rows keep source indices, typed columns, residual slots, and constructor/record specialization rules. | Unit cases for wildcard, variable, constructor, record and nested combinations; assert specialized/default matrices and bindings. | Verified |
| PM-03 | Decision compilation chooses a useful column, specializes irrefutable rows correctly, and hashes equivalent normalized residual states. | Inspect DAG size/sharing for repeated subproblems and assert first-row irrefutable base case; compare against a simple first-match oracle. | Verified |
| PM-04 | Coverage and usefulness detect exhaustive, missing, and redundant rows with a valid nested witness and source span. | Exhaustive/non-exhaustive recursive ADTs, nested records, duplicate rows, and impossible branches; validate warning/error location and witness usefulness. | Verified |
| PM-05 | Decision DAG realizes constructor tests, tags, projections, variable binds, records, and newtype erasure through ordinary CC operations. | Inspect generated CC for each pattern family; execute field-bearing and nested cases, including failure edges and no redundant projection. | Verified |
| PM-06 | Nullary sums dispatch through CC TagSwitch, MIR Switch, and Wasm `br_table` when tags are unique; duplicates preserve source order. | Inspect all three stages, validate bytes, and execute dense/default/duplicate-tag cases with value-sensitive results. | Verified |
| PM-07 | Field-bearing sums and records perform tag/shape tests before projections and use typed branch results. | Malformed CC/MIR fixtures reject wrong tag, field, branch shape, or nondominating projection; valid nested cases execute. | Verified |
| PM-08 | Dependent erased scalar and aggregate fields recover according to their declared templates, never by unsafe nominal cast. | Generic ADT/record pattern cases over at least two instantiations, inspect conversion plans and execute recovered payload values; coordinate accepted aggregate cases. | Verified |
| PM-09 | An impossible missing edge becomes typed CC/MIR Unreachable and Wasm unreachable; real missing cases yield diagnostics. | Exhaustive and non-exhaustive fixtures inspect reachability, diagnostic spans, and expected runtime behavior. | Verified |
| PM-10 | Pattern compilation terminates on recursive types and preserves useful sharing without changing first-match semantics. | Recursive ADT and nested pattern stress fixtures with bounded compile time/DAG size and oracle comparison. | Verified |
| PM-11 | Optimization and Wasm lowering preserve warnings, source order, and selected-branch values. | Compare pre/post optimization execution on duplicate, nested, and generic cases; assert diagnostics remain source-associated. | Verified |
| PM-12 | Internal decision-DAG and realizer invariant violations are classified `InvalidCompilerIr`, not `UnsupportedSource`; genuine coverage failures stay source-associated. | Negative fixtures assert the `BackendErrorKind` of an unbound decision column and of structural DAG failures. | Verified |
| PM-13 | MIR verification rejects a variant projection read outside the dominance scope of its tag test. | A malformed MIR fixture reads a value defined in a sibling switch arm and must fail dominance. | Verified |

PM-12 and PM-13 were discovered during the audit. The design's present-tense
contract (`Model`, `Design`, `Algorithms`, `Invariants and verification`, and
`Boundaries and interfaces`) imposes both: an internal DAG that references a
column before binding it, or a projection that escaped its tag test's dominance,
is a compiler defect; the verifier must reject it and the diagnostic kind must
say so.

## Vertical execution order

1. Audit typed pattern inputs, matrix/coverage/DAG code, CC realizer, MIR
   switch verifier, and Wasm encoder against every present-tense design rule.
2. Finish analysis and lowering as one vertical slice per pattern family;
   add negative verifier fixtures and source-order oracle cases.
3. Run optimized component execution for representative constructor, record,
   newtype, nested, recursive, and generic cases with mandatory Wasmtime.
4. Record evidence and update D-04. Frontend pattern syntax and official-suite
   landing remain independently tracked.

## Evidence record and completion rule

For every ID record code paths/functions, named tests/assertions, input
boundary, commands, Wasmtime version, executed/skipped cases, revision, and
gaps. Typed Core fixtures prove backend pattern handling only. Runtime cases
must execute with `PSRS_REQUIRE_WASMTIME=1`; skips leave rows unverified.
After Rust changes run `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus focused runtime
tests. Close only after all rows and the complete present-tense design pass.

Common commands (worktree root, `CARGO_TARGET_DIR` pointing at this worktree's
target directory):

```sh
cargo fmt --all --check
PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Runtime: `wasmtime 49.0.0 (17830bd3c 2026-09-21)`. All execution cases below ran;
nothing was skipped under `PSRS_REQUIRE_WASMTIME=1`.

```text
PM-01:
  Implementation: cc/lower/mod.rs `ExprKind::Case` lowers the scrutinee once and
    passes its ValueId to cc/case/mod.rs `lower_case`; cc/case/decision/compile
    builds rows in source order with the source `branch` index; realize/mod.rs
    `lower_decision_node` walks edges in order.
  Tests: pattern_matching_audit::case_evaluates_its_scrutinee_exactly_once
    executes a case whose scrutinee records logs on construction and asserts
    stdout `once\n` (exactly one evaluation);
    ::first_match_order_is_value_sensitive asserts reordered, duplicate,
    wildcard-before-constructor, and variable-before-constructor exits
    20/10/20/10; decision::compile::oracle_tests::
    decision_dag_matches_the_first_match_oracle_for_every_small_matrix checks
    all 258 small matrices against a first-match interpreter;
    adts::preserves_the_first_of_duplicate_constructor_patterns,
    ::preserves_an_earlier_wildcard_before_a_constructor_pattern,
    ::preserves_an_earlier_variable_before_a_constructor_pattern.
  Input boundary: source, executed Wasm, verified Core.
  Commands: common commands.
  Result: pass; the source cases executed under Wasmtime.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: none.

PM-02:
  Implementation: cc/case/decision/compile/mod.rs `Row` (patterns, bindings,
    branch, span) and `Column`/`ColumnKey`; compile/constructors.rs
    `compile_product`/`compile_sum`/`constructor_columns`; compile/matrix.rs
    `canonicalize`/`map_actions`/`available_inputs`.
  Tests: oracle_tests (constructor/Any specialization),
    oracle_record_tests::record_dag_matches_the_first_match_oracle_for_every_small_matrix
    (canonical field projections and partial records);
    decision::compile::tests::repeated_residual_matrix_is_shared_across_projection_paths
    asserts a canonical `Bind` action; ::nested_record_constructor_has_one_projection_and_keeps_spans
    asserts one projection and the pattern span.
  Input boundary: verified Core.
  Commands: common commands.
  Result: pass.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: none.

PM-03:
  Implementation: cc/case/decision/compile/matrix.rs `choose_column`;
    compile/mod.rs `compile_matrix` memo and `compile_uncached` first-row /
    all-irrefutable-column base cases; constructors.rs `specialize`/default.
  Tests: decision::compile::oracle_tests::
    decision_dag_matches_the_first_match_oracle_for_every_small_matrix (oracle),
    oracle_record_tests (record oracle);
    decision::compile::tests::repeated_residual_matrix_is_shared_across_projection_paths
    asserts equal residual matrices return the same node with no new nodes;
    ::duplicate_constructor_rows_keep_first_match_and_row_spans.
  Input boundary: verified Core.
  Commands: common commands.
  Result: pass; the oracle covers column choice, specialization, sharing, and
    first-match for every small sum and record matrix.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: none.

PM-04:
  Implementation: cc/case/coverage/mod.rs `analyze`, `useful`, `useful_with_active`
    (cycle-safe), `signature`, `specialize`, `default_matrix`, `render`;
    cc/case/mod.rs `require_exhaustive`/`report_redundant_branches`.
  Tests: coverage::tests::reports_the_missing_nullary_constructor,
    ::recognizes_exhaustive_nested_constructor_patterns,
    ::reports_a_nested_missing_constructor,
    ::record_products_are_covered_fieldwise_and_duplicate_rows_are_redundant,
    ::recursive_adt_wildcard_coverage_terminates,
    ::recursive_adt_analysis_finds_a_finite_uncovered_witness,
    ::recursive_coverage_agrees_with_a_bounded_first_match_oracle (all small
    recursive matrices against a bounded oracle);
    decision::compile::oracle_tests::coverage_agrees_with_the_first_match_oracle;
    adts::reports_a_missing_nested_constructor_as_a_coverage_witness checks the
    nested witness and exact case-expression span;
    ::exposes_redundant_case_alternatives_as_source_spanned_warnings checks the
    offending alternative range.
  Input boundary: verified Core and source.
  Commands: common commands.
  Result: pass.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: none.

PM-05:
  Implementation: cc/case/decision/realize/mod.rs `lower_decision`,
    `lower_decision_node`, `apply_action_list`; realize/switch.rs
    `lower_nullary_switch`/`lower_variant_switch`; compile/constructors.rs
    `needed_fields` projection selection.
  Tests: realize::tests::unbound_decision_column_is_invalid_compiler_ir;
    ::nested_tests::nested_sum_patterns_project_once_per_selected_constructor_and_trap_missing_tags
    asserts one VariantTag, two VariantGet, one TagSwitch, one Unreachable;
    ::product_tests::single_constructor_product_dispatch_projects_and_binds_first_row_once
    asserts one projection with the pattern span;
    ::record_tests::nested_record_patterns_share_one_product_projection_and_keep_source_spans;
    ::newtype_tests::newtype_constructor_erases_before_nested_enum_dispatch
    asserts no VariantGet/ProductGet for a newtype;
    pattern_matching_audit::product_pattern_projects_only_needed_fields asserts
    exactly `[1]` for a three-field product; ::field_bearing_sum_tests_the_tag_once_and_falls_through
    asserts one VariantTag and executes the fall-through.
  Input boundary: verified Core, source, malformed decision DAG.
  Commands: common commands.
  Result: pass; the source cases executed under Wasmtime.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: none.

PM-06:
  Implementation: realize/switch.rs `lower_nullary_switch` (CC TagSwitch);
    mir/lower/assignments.rs TagSwitch -> `Terminator::Switch`;
    wasm/lower/structure/region.rs switch -> `br_table`.
  Tests: pattern_matching_audit::nullary_sum_dispatch_lowers_through_all_three_stages
    asserts a CC TagSwitch, a MIR Switch, a Wasm `br_table`, and Wasm
    `unreachable`, then executes Blue -> 30;
    ::first_match_order_is_value_sensitive executes reordered and duplicate
    cases; adts::lowers_enum_case_to_mir_switch_and_wasm_br_table;
    adts::dispatches_reordered_enum_tags_and_the_default_arm_correctly;
    wasm/lower/structure/switch_tests::structures_sparse_negative_switch_tags_and_executes_the_default;
    mir/verify/tests/switch.rs.
  Input boundary: source, executed Wasm, MIR bytes.
  Commands: common commands.
  Result: pass; executed under Wasmtime.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: none.

PM-07:
  Implementation: realize/switch.rs `lower_variant_switch` reads the tag once
    then wraps each edge in an `If`; realize/mod.rs `variant_field_shape`;
    cc/verify/variant.rs; mir/verify/function.rs `value_available`.
  Tests: cc/verify/tests/variant.rs::rejects_a_variant_get_with_an_unknown_case,
    ::rejects_a_variant_get_with_an_unknown_field,
    ::rejects_a_variant_get_with_the_wrong_result_shape,
    ::rejects_a_tag_switch_case_with_the_wrong_branch_shape;
    cc/verify/tests/structure.rs::rejects_a_tag_switch_with_duplicate_tags;
    mir/verify/tests/switch.rs::rejects_a_projection_that_is_not_dominated_by_its_tag_test;
    pattern_matching_audit::field_bearing_sum_tests_the_tag_once_and_falls_through;
    adts::selects_between_nested_constructor_patterns,
    ::falls_back_when_a_nested_erased_pattern_does_not_match,
    ::runs_a_non_parameterized_field_constructor_case.
  Input boundary: malformed CC and MIR, source, executed Wasm.
  Commands: common commands.
  Result: pass; the source cases executed under Wasmtime.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: none.

PM-08:
  Implementation: cc/lower/conversion.rs `erased_field_recovery` and
    `emit_conversion`; realize/mod.rs `apply_action_list` recovery for
    VariantGet/ProductGet; generic aggregate erasure owns the conversion plans.
  Tests: realize::tests::parameterized_tests::
    parameterized_array_field_recovery_uses_the_canonical_generic_array,
    ::nested_parameterized_array_projection_recovers_each_canonical_boundary,
    ::generic_record_pattern_projects_its_canonical_array_field;
    pattern_matching_audit::dependent_erased_fields_recover_at_two_instantiations
    executes an erased scalar and an erased aggregate instantiation and asserts
    `ref.cast`; adts::runs_a_parameterized_adt_with_an_erased_scalar_field,
    ::runs_a_parameterized_adt_with_an_erased_number_field,
    ::runs_a_nested_pattern_on_an_erased_parameterized_field.
  Input boundary: verified Core and source.
  Commands: common commands. See also
    [generic aggregate erasure](generic-aggregate-erasure.md) GA-06/GA-14/GA-19.
  Result: pass; the source cases executed under Wasmtime.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: open-row patterns remain outside the conversion contract by design.

PM-09:
  Implementation: Decision::Fail realization -> `AssignmentKind::Unreachable`;
    mir/lower/assignments.rs Unreachable -> `Instruction::Unreachable`;
    wasm/lower/structure/mod.rs `block_traps`; coverage diagnostics.
  Tests: realize::tests::compiled_root_switch_resolves_the_realizer_root_slot
    asserts a trapping TagSwitch default and a MIR `Instruction::Unreachable`;
    ::nested_tests::nested_sum_patterns_project_once_per_selected_constructor_and_trap_missing_tags
    asserts one Unreachable; pattern_matching_audit::
    nullary_sum_dispatch_lowers_through_all_three_stages asserts Wasm
    `unreachable`; adts::reports_a_missing_nested_constructor_as_a_coverage_witness.
  Input boundary: verified Core, malformed tag path, source diagnostic.
  Commands: common commands.
  Result: pass. A malformed runtime tag is unreachable from a checked program;
    the trap path is proven structurally through CC, MIR, and validated Wasm.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: none.

PM-10:
  Implementation: coverage/mod.rs cycle-safe `useful_with_active`;
    compile/mod.rs memoization and the all-irrefutable-column base case.
  Tests: coverage::tests::recursive_adt_wildcard_coverage_terminates,
    ::recursive_adt_analysis_finds_a_finite_uncovered_witness,
    ::recursive_coverage_agrees_with_a_bounded_first_match_oracle;
    oracle_tests and oracle_record_tests (whole small matrices);
    pattern_matching_audit::recursive_pattern_compilation_terminates_and_stays_first_match
    executes a depth-four recursive ADT; adts::runs_nested_non_parameterized_gc_aggregates.
  Input boundary: verified Core and source.
  Commands: common commands.
  Result: pass; the recursive execution case ran under Wasmtime.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: none.

PM-11:
  Implementation: full pipeline in lib.rs `compile_with_target`; warnings
    carried on `Artifact`; mir/opt preserves Switch terminators and selectors.
  Tests: pattern_matching_audit::optimization_preserves_values_and_source_warnings
    asserts the optimized artifact keeps the source-spanned redundancy warning
    and executes the selected `v -> 20` branch; adts::
    lowers_enum_case_to_mir_switch_and_wasm_br_table asserts P10 preserves the
    selector and every MIR Switch successor; mir/opt/tests/control_flow.rs
    checks copy forwarding remaps the Switch selector.
  Input boundary: source and executed Wasm.
  Commands: common commands.
  Result: pass; executed under Wasmtime.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: none.

PM-12:
  Implementation: cc/case/mod.rs `ir_error` vs `source_error`;
    cc/case/decision/realize/mod.rs `case_error` uses `BackendError::invalid_ir`.
  Tests: realize::tests::unbound_decision_column_is_invalid_compiler_ir asserts
    every error kind is `InvalidCompilerIr`; the coverage diagnostics remain
    `UnsupportedSource` through `require_exhaustive`/`source_error` and the
    existing coverage tests.
  Input boundary: malformed decision DAG and internal class-layout/WIT shapes.
  Commands: common commands.
  Result: pass. The dictionary path (`cc/lower/dictionary/mod.rs`) and the
    internal WIT result arms (`mir/wit/mod.rs`) now classify inconsistent
    compiler evidence as `InvalidCompilerIr` rather than `UnsupportedSource`;
    genuine coverage and unsupported-representation diagnostics stay
    source-associated.
  Revision: f2c43af + the classification change in this worktree.
  Gaps: none. The remaining `BackendError::new` sites in `cc/lower` describe
    unsupported source shapes or source-span diagnostics, not internal
    invariants.
```

```text
PM-13:
  Implementation: mir/verify/function.rs `value_available` and
    `compute_dominators`.
  Tests: mir/verify/tests/switch.rs::
    rejects_a_projection_that_is_not_dominated_by_its_tag_test reads a value
    defined in a sibling switch arm from the default arm and asserts the
    "outside its dominance scope" diagnostic.
  Input boundary: malformed MIR.
  Commands: common commands.
  Result: pass.
  Revision: 95aebe3 + uncommitted changes.
  Gaps: none.
```

## Discovered obligations and remaining work

- **PM-12 (typed internal failures).** The audit found that decision-DAG and
  realizer invariant violations were reported as `UnsupportedSource`. They are
  now `InvalidCompilerIr`; the coverage and unsupported-representation
  diagnostics stay source-associated. The dictionary class-layout path
  (`cc/lower/dictionary/mod.rs`) and the internal WIT result arms
  (`mir/wit/mod.rs`) were also reclassified, closing the cross-topic handoff.
- **PM-13 (projection dominance).** The audit added a MIR negative fixture for a
  projection that outlives its tag test's dominance, complementing the existing
  duplicate-tag and non-i32-selector fixtures.
- **Oracle coverage.** PM-03/PM-04 now have an exhaustive first-match oracle for
  all small sum and closed-record matrices plus a bounded recursive-ADT oracle.
  Opening the oracle to newtypes and multi-column records is straightforward
  future work; current source-level newtype and record cases execute instead.
- Excluded by design and not claimed as present support: literal, guard, view,
  tuple, array, as, and or patterns; open-row patterns.
- Frontend pattern syntax and official-suite landing remain independently
  tracked (FE-06/FE-12, M8-W).
