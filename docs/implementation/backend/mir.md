# MIR Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [MIR](../../design/backend/fp/mir.md)

**Progress:** All rows MIR-01 through MIR-12 Verified. The tail-call terminators
(`ReturnCall`, `ReturnCallRef`) exist and are exercised by the records below;
see [control flow and tail calls](control-flow-and-tail-calls.md). The `String`
value type is re-baselined to a GC reference by
[DEC-10](../../decision/DEC-10-canonical-abi-buffer-lifetime.md); the affected
layout rows are tracked in
[data representation](data-representation.md) and
[scalars and primitives](scalars-and-primitives.md).

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-03, with BE-13 and BE-15 at the output boundary.

## Scope and dependencies

Complete MIR as the lowest long-lived, typed SSA/CFG representation, including
P9 lowering, representation planning, verification, and its P10 handoff. The
linked design's present-tense requirements remain authoritative beyond this
matrix. CC supplies target-neutral values; [data representation](data-representation.md)
owns physical GC layouts; [control flow and tail calls](control-flow-and-tail-calls.md)
owns general structuring and tail calls; WIT and linear memory own their ABI
rules. MIR must express and verify their required interfaces. Keep spans where
diagnostics or debugging need them and use the design's Code map as the target.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. A
Verified row requires named implementation and reproducible test evidence.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| MIR-01 | P9 consumes verified CC through an explicit conversion and creates one typed MIR module, not CC nodes with target fields. | Trace a CC fixture into MIR and inspect types, spans, imports, and absence of unresolved CC representation handles. | Verified |
| MIR-02 | Functions contain typed basic blocks, block parameters, SSA instructions, and explicit terminators with unique IDs. | Positive CFG fixtures and negative duplicate/missing ID, missing terminator, and cross-function value tests. | Verified |
| MIR-03 | Reachable `ReprId`s reserve and resolve to exact defined types in one recursion group without merging distinct nominal identities. | Recursive variant/array/record/closure layout fixtures; inspect one-to-one reachable mapping and valid Wasm type section. | Verified |
| MIR-04 | P9 lowers scalar, product, variant, array, closure, erased, and canonical ABI shapes to exact MIR value types. | Table-driven positive and mismatch tests across every shape; verify concrete and canonical aggregate layouts separately. | Verified |
| MIR-05 | MIR carries the required instruction families with exact input/output and effect contracts. | Check numeric, call/ref, GC, array, memory, import and conversion instructions; malformed modules reject invalid operand and result types. | Verified |
| MIR-06 | CFG verification proves definition dominance, block-argument arity/types, Boolean branch selectors, switch tags/targets, and return types. | Full-module valid joins/loops and malformed non-dominating uses, bad edges, duplicate tags, and wrong return/signature fixtures. | Verified |
| MIR-07 | Defined-type and reference subtyping obey Wasm mutability, finality, function variance, and nullability rules. | Positive/negative struct, array, function-ref, subtype and cast fixtures; include nullable load into non-null destination rejection. | Verified |
| MIR-08 | Aggregate reconstruction uses typed helpers and private defaultable array allocation; every slot is initialized before exposure. | Inspect lowered maps, verify malformed early read/escape/incomplete-loop fixtures, and execute empty/nested conversions; coordinate evidence with [generic aggregate erasure](generic-aggregate-erasure.md). | Verified |
| MIR-09 | External calls use checked import signatures and the selected canonical ABI boundary; unsupported capabilities fail before encoding. | Positive component import call and negative import/type/capability fixtures with source-associated failures where input has a span. | Verified |
| MIR-10 | P10 consumes and returns verified MIR without changing its representation contract. | Verify before and after each enabled pass; differential execution on traps, calls, mutable arrays, imports, and aggregate reconstruction. | Verified |
| MIR-11 | Wasm emission mechanically consumes verified MIR and does not make new layout decisions. | Validate emitted core modules/components, inspect representative GC/reference operations, and execute value-sensitive fixtures. | Verified |
| MIR-12 | MIR diagnostics distinguish invalid compiler IR from unsupported valid source input. | Assert failure stage, diagnostic kind, and source span on representative malformed MIR and unsupported source fixtures. | Verified |

## Vertical execution order

1. Audit the MIR model, P9 planner, instruction set, verifier, P10 boundary,
   and encoder consumers against every design section; assign evidence gaps.
2. Complete missing types, instructions, lowering, and verifier rules together.
   Add negative fixtures for every new verifier invariant.
3. Exercise each representation family through Wasm validation and mandatory
   component execution, including optimizer paths and malformed inputs.
4. Update evidence and D-04; the broader BE-03, BE-13, and BE-15 gates retain
   their own official-suite and capability scope.

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names/assertions, input
boundary, commands and runtime version, executed/skipped cases, tested revision,
and gaps. A direct MIR fixture proves the MIR contract, not source coverage;
record missing source paths separately. Runtime evidence uses
`PSRS_REQUIRE_WASMTIME=1` and cannot be skipped. After Rust edits run
`cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus focused mandatory
Wasmtime tests. Verify all rows and re-audit the full present-tense design
before closing this topic.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
MIR-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Recorded evidence

Revision: `f2c43af` plus the MIR audit changes in this worktree. Runtime:
`wasmtime 49.0.0` under `PSRS_REQUIRE_WASMTIME=1`; nothing skipped. Root
commands: `cargo fmt --all --check`, `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`.

```text
MIR-01:
  Implementation: mir/mod.rs (lower_module, lower_module_with_bindings,
    lower_module_after_binding_validation); mir/planner.rs, mir/layout/,
    mir/reachable.rs produce the typed module; no CC representation handle
    survives into MIR.
  Tests: mir::planner::tests::gc_planner_consumes_cc_requirements;
    mir::layout::tests::module_planner_omits_unreachable_requirements;
    mir::tests::{defined_types_flow_into_the_wasm_type_section,
    lowers_an_imported_call}; cc_ir_audit cases trace CC into MIR and inspect
    spans and types.
  Input boundary: verified CC.
  Commands: cargo test -p psrs-backend mir::; PSRS_REQUIRE_WASMTIME=1
    cargo test -p psrs-driver --lib.
  Result: pass.
  Revision: f2c43af plus this worktree.
  Gaps: none.
```

```text
MIR-02:
  Implementation: mir/mod.rs (Function, BasicBlock, ValueDecl, Terminator);
    mir/verify/function.rs (unique IDs, one definition per value, entry and
    terminator presence, dominance).
  Tests: mir::verify::tests::structure::
           rejects_duplicate_value_ids, rejects_duplicate_block_ids,
           rejects_a_block_without_a_terminator,
           rejects_a_value_defined_by_two_instructions;
         mir::verify::tests::dominance::
           accepts_a_diamond_join_that_receives_a_dominating_value,
           accepts_a_loop_with_a_carried_block_parameter,
           rejects_a_value_defined_in_one_branch_and_used_at_the_join.
  Input boundary: malformed and valid full MIR.
  Commands: cargo test -p psrs-backend mir::verify.
  Result: pass.
  Revision: f2c43af plus this worktree.
  Gaps: none.
```

```text
MIR-03:
  Implementation: mir/reachable.rs (sorted worklist), mir/layout/mod.rs
    (one RecGroup, repr_indices), mir/layout/validate.rs.
  Tests: mir::layout::tests::
           plans_mutually_recursive_requirements_in_one_recursion_group,
           module_planner_omits_unreachable_requirements,
           planner_owns_the_gc_closure_and_capture_layouts,
           planner_rejects_a_dangling_closure_signature;
         mir::tests::defined_types_flow_into_the_wasm_type_section validates
         the emitted type section.
  Input boundary: CC RepresentationTable and verified MIR.
  Commands: cargo test -p psrs-backend mir::layout.
  Result: pass; every reachable requirement maps to one defined type and
    distinct nominal identities stay distinct.
  Revision: f2c43af plus this worktree.
  Gaps: none.
```

```text
MIR-04:
  Implementation: mir/layout/mod.rs (value_type, storage_type,
    array_storage_type, reference); mir/lower/.
  Tests: mir::layout::tests::maps_every_cc_value_shape_to_its_specified_mir_type
           (Integer->I32, Boolean->Boolean, Number->F64, String->I32,
            references with copied nullability, product fields),
           plans_field_bearing_variants_as_tag_carrying_supertypes,
           plans_the_uniform_closure_and_capture_array_layout;
         mir::gc_tests::{lowers_a_cc_product_through_the_gc_planner,
           variant::lowers_different_variant_cases_through_both_planners,
           array::executes_a_pure_array_clone_and_update,
           erased::executes_erased_identity_for_scalars_and_concrete_references_on_both_targets};
         mir::tests::{lowers_and_validates_f64_to_f32_abi_conversions,
           lowers_an_imported_call}; mir::wit::tests::*.
  Input boundary: CC shapes, verified MIR, executed components.
  Commands: cargo test -p psrs-backend mir::.
  Result: pass; concrete and canonical aggregate layouts are verified
    separately.
  Revision: f2c43af plus this worktree.
  Gaps: none.
```

```text
MIR-05:
  Implementation: mir/instruction.rs (destination/operands/span),
    mir/numeric.rs, mir/scalar_helpers.rs, mir/verify/instruction/*.
  Tests: mir::gc_tests::binary_matrix::
           verifies_and_executes_every_cc_binary_scalar_variant_on_both_targets;
         mir::gc_tests::unary::
           lowers_unary_and_conversion_operations_on_both_targets;
         mir::gc_tests::{lowers_a_cc_product_through_the_gc_planner,
           array::executes_a_pure_array_clone_and_update,
           variant::lowers_different_variant_cases_through_both_planners};
         mir::tests::{lowers_an_imported_call,
           lowers_and_validates_f64_to_f32_abi_conversions,
           rejects_a_struct_new_with_a_mistyped_field};
         mir::verify::tests::scalar::* and instruction negatives.
  Input boundary: CC fixtures, malformed MIR, executed components.
  Commands: cargo test -p psrs-backend mir::.
  Result: pass; malformed operands and results are rejected.
  Revision: f2c43af plus this worktree.
  Gaps: none.
```

```text
MIR-06:
  Implementation: mir/verify/function.rs (dominance, block-argument
    arity/types, Branch selectors, Switch tags/targets, Return type);
    mir/opt/mod.rs prunes unreachable blocks before verifying.
  Tests: mir::verify::tests::dominance (valid diamond join, loop-carried
    parameter, non-dominating branch value);
    mir::verify::tests::{rejects_a_branch_target_with_block_parameters,
    rejects_values_used_before_definition, rejects_duplicate_switch_case_values,
    rejects_ref_func_with_a_different_target_signature};
    mir::verify::tests::switch::{rejects_a_switch_with_a_non_i32_selector,
    rejects_a_projection_that_is_not_dominated_by_its_tag_test};
    mir::verify::tests::tail::*.
  Input boundary: malformed and valid full MIR.
  Commands: cargo test -p psrs-backend mir::verify.
  Result: pass.
  Revision: f2c43af plus this worktree.
  Gaps: none.
```

```text
MIR-07:
  Implementation: mir/verify/subtype.rs (finality, mutability, width/depth,
    contravariant parameters and covariant results, nullability);
    mir/verify/instruction/mod.rs (ref.test/ref.cast heap relation);
    mir/verify/mod.rs now returns the collected module-level errors instead
    of discarding them.
  Tests: mir::verify::tests::subtype::
           accepts_a_valid_struct_case_subtype,
           rejects_a_final_declared_supertype,
           rejects_a_subtype_that_changes_field_mutability,
           rejects_a_subtype_that_widens_field_nullability,
           accepts_contravariant_parameters_and_covariant_results,
           rejects_a_function_subtype_that_widens_its_result;
         mir::verify::tests::gaps::{rejects_ref_cast_between_unrelated_heaps,
           accepts_an_erased_upcast_to_eqref};
         mir::verify::tests::arrays::rejects_nullable_array_load_declared_as_nonnullable.
  Input boundary: malformed and valid full MIR.
  Commands: cargo test -p psrs-backend mir::verify.
  Result: pass. The `verify_module` defect fix is what makes the subtyping and
    module-level checks effective; before it, `verify_defined_types` errors
    were collected and dropped.
  Revision: f2c43af plus this worktree.
  Gaps: none.
```

```text
MIR-08:
  Implementation: mir/lower/aggregate/{mod,array}.rs (ProductMap/ArrayMap,
    private defaultable destination, RefCast at exit); mir/lower/
    conversion_helpers.rs; mir/verify/array_map/mod.rs;
    mir/verify/instruction/arrays.rs.
  Tests: mir::verify::array_map::tests::* (missing store, early escape,
    non-unit advance, store outside the loop, reading before the loop);
    mir::verify::tests::arrays::
      rejects_array_new_default_with_non_defaultable_element_storage,
      rejects_array_clone_with_non_defaultable_element_storage,
      rejects_array_clone_of_an_immutable_array;
    mir::lower::conversion_helpers::tests::
      shares_equal_full_plans_but_not_different_nested_plans;
    generic_aggregate_audit::empty_array_reconstruction_executes and the
    GA-16/GA-17 records in
    [generic aggregate erasure](generic-aggregate-erasure.md).
  Input boundary: malformed and valid full MIR, source/Core fixtures.
  Commands: cargo test -p psrs-backend mir::verify::array_map;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit.
  Result: pass; every nullable slot is initialized before exposure.
  Revision: f2c43af plus this worktree.
  Gaps: none.
```

```text
MIR-09:
  Implementation: mir/mod.rs (import projection from referenced calls);
    mir/bindings.rs and mir/lower/wit.rs (canonical ABI adaptation);
    mir/verify/capability.rs (unsupported capabilities rejected before
    encoding); mir/verify/instruction/memory.rs.
  Tests: mir::tests::{lowers_an_imported_call,
    lowers_and_validates_f64_to_f32_abi_conversions}; mir::binding_tests::*;
    mir::wit::tests::*; mir::verify::capability::* (source-attributed gate
    failures); psrs-driver tests::wasi::
    rejects_a_wasi_interface_outside_the_component_capability_profile.
  Input boundary: verified CC/bindings, malformed MIR with reduced profiles,
    source diagnostics.
  Commands: cargo test -p psrs-backend mir::;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass.
  Revision: f2c43af plus this worktree.
  Gaps: none.
```

```text
MIR-10:
  Implementation: mir/opt/mod.rs (verify before and after every enabled pass),
    mir/opt/{cfg,constants,values,inline,imports,effects}.rs.
  Tests: mir::opt::tests::control_flow::
    folds_branch_merges_prunes_blocks_and_projects_imports,
    preserves_switch_selectors_and_all_successors_through_p10;
    mir::opt::tests::constants::
    folds_total_arithmetic_but_keeps_an_unused_trapping_operation;
    mir::opt::tests::{copy,copy_forwarding_updates_the_structurer_result_value,
    inline::inlines_small_direct_functions_and_folds_the_result};
    driver execution on traps, calls, mutable arrays, imports, and aggregate
    reconstruction (tests::tail_calls, tests::generic_aggregate_audit,
    tests::polymorphism_erasure_audit) runs the optimized pipeline.
  Input boundary: verified MIR and source.
  Commands: cargo test -p psrs-backend mir::opt;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass; optimization preserves the type table and observable results.
  Revision: f2c43af plus this worktree.
  Gaps: none. Differential execution is covered across the named suites
    rather than one combined matrix.
```

```text
MIR-11:
  Implementation: wasm/lower/ encodes verified MIR mechanically;
    wasm/lower/structure/closure.rs documents the one encoder-owned rule
    (Boolean capture i31).
  Tests: mir::gc_tests::{lowers_a_cc_product_through_the_gc_planner,
    array::executes_a_pure_array_clone_and_update,
    variant::lowers_different_variant_cases_through_both_planners,
    erased::...}; mir::tests::{defined_types_flow_into_the_wasm_type_section,
    runs_a_mir_gc_struct_under_wasmtime}; driver execution suites validate and
    run the emitted component.
  Input boundary: verified MIR then Wasm core/component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass.
  Revision: f2c43af plus this worktree.
  Gaps: none; Boolean capture i31 stays an encoder rule by documented design.
```

```text
MIR-12:
  Implementation: psrs-backend/src/lib.rs (BackendErrorKind,
    BackendError::invalid_ir), mir/verify/util.rs, wasm/verify.rs,
    wasm/lower/mod.rs, mir/lower/*, psrs-driver/src/lib.rs (Diagnostic.kind).
  Tests: mir::verify::tests::
           backend_error_kinds_distinguish_invalid_ir_from_unsupported_source;
         psrs-driver tests::wasi::
           rejects_a_wasi_interface_outside_the_component_capability_profile.
  Input boundary: malformed MIR and unsupported source.
  Commands: cargo test -p psrs-backend;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass.
  Revision: f2c43af plus this worktree.
  Gaps: none.
```
