# Data Representation Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Data representation](../../design/backend/fp/data-representation.md)

**Progress:** Verified for DR-01 through DR-12 on the audited revision. The
runtime evidence executed under Wasmtime 49.0.0 with `PSRS_REQUIRE_WASMTIME=1`;
open questions and cross-topic handoffs are recorded below.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-05 through BE-10 and BE-15.

## Scope and dependencies

Complete the physical Wasm GC layouts and P9 operation lowering in the linked
design. Its present-tense Model, Algorithms, Code map, Invariants, and
Boundaries are authoritative beyond this matrix. CC provides abstract shapes;
[MIR](mir.md) owns the typed IR and verifier; [generic aggregate erasure](generic-aggregate-erasure.md)
owns conversions between nominal aggregate layouts; [polymorphism and erasure](polymorphism-and-erasure.md)
owns erased values. This topic must implement the concrete interfaces they
require. Linear memory holds ABI bytes, not language heap values. Keep distinct
nominal identities and use the design's Code map as the organization target.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. A
Verified row needs named implementation, negative verification, and execution
evidence where behavior is observable.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| DR-01 | P9 maps every CC scalar, string, reference, erased, and closure shape to the specified MIR value type. | Table-driven layout tests for `Int`, `Number`, `Boolean`, `Char`, `Unit`, `String`, products, variants, arrays, closures, and erased values; reject mismatches. | Verified |
| DR-02 | Planner reserves all reachable nominal types in one recursion group and maps each `ReprId` to one `DefinedTypeId`. | Recursive and mutually referring record/array/variant/closure fixtures inspect type identities, forward references, and valid Wasm type section. | Verified |
| DR-03 | Products and closed records are structs with canonical field order and correct mutability; pure update creates a new value. | Construct, project, pattern match, and update mixed fields; retain the old value and execute assertions on both. | Verified |
| DR-04 | Field-bearing sums use an abstract tag-carrying supertype and final case subtypes; all-nullary sums use immediate `i32` tags. | Inspect type hierarchy and tag positions; execute nullary, single/multiple-field and nested ADT cases; reject wrong tag/field selection. | Verified |
| DR-05 | Newtype representation erases the wrapper exactly where the design specifies. | Inspect CC/MIR for absence of wrapper allocation and execute construction/matching through nested uses. | Verified |
| DR-06 | Arrays use mutable GC element storage but source `ArraySet` is a pure clone/update; canonical and concrete layouts remain distinct. | Execute empty, singleton, nested, and aliased update/read cases; inspect `array.new`, `get`, `set`, clone and type indices. | Verified |
| DR-07 | Closures use a code reference and uniform nullable `eqref` capture array with exact capture boxing. | Inspect layout, capture ordering and code signature; execute escaping closures capturing each scalar class and GC references. | Verified |
| DR-08 | Scalar boxes and erased/reference recovery obey exact nullability and nominal provenance rules. | Positive and negative `ref.test`/`ref.cast`, box/unbox, i31 Boolean capture, full-width integer, and Number paths; no nominal cast substitutes for aggregate reconstruction. | Verified |
| DR-09 | Product, variant, array, closure, and conversion operations lower to exact typed MIR instructions. | Full-module verifier rejects wrong operand, field/index, mutability, arity, nullability, and layout; valid cases validate as Wasm. | Verified |
| DR-10 | Private defaultable aggregate allocation cannot escape before full initialization. | Malformed MIR early-read/return/branch fixtures and nested conversion execution; coordinate proof with [generic aggregate erasure](generic-aggregate-erasure.md). | Verified |
| DR-11 | Capability flags reject operations requiring disabled GC, references, or typed function references before emission. | Compile representative operations with each feature disabled; assert named failure and no invalid artifact. | Verified |
| DR-12 | Emitted core module and component execute representative reachable layouts without relying on WAT text alone. | Mandatory Wasmtime cases for products, sums, arrays, closures, erased fields and conversion paths, with value-sensitive assertions. | Verified |

## Vertical execution order

1. Audit CC shapes, P9 planner, MIR operations/verifier, target profile and
   executable fixtures against every design section.
2. Complete each layout and corresponding operation/verifier rules together;
   add malformed fixtures before accepting a representation family.
3. Validate and execute artifacts for each reachable family, including
   aliases, nested values, capability failures, and generic/concrete paths.
4. Record evidence and update D-04. Broader source syntax, open rows, WIT
   aggregates, and official-suite gates retain their own owners.

## Evidence record and completion rule

For every ID record code paths/functions, exact tests/assertions, input
boundary, commands, Wasmtime version, executed/skipped cases, tested revision,
and gaps. Typed Core or direct MIR fixtures establish only their entry boundary.
Runtime cases require `PSRS_REQUIRE_WASMTIME=1`; skipped cases are unverified.
After Rust edits run `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus focused mandatory
runtime cases. Close only when every row and every present-tense design rule
has evidence.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
DR-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Evidence records

Revision: `775fafe` plus the uncommitted audit changes in this worktree.
Wasmtime: 49.0.0. All commands ran with the worktree's unique target directory.

```text
DR-01:
  Implementation: mir/layout/mod.rs (PlannedLayout::plan_selected, value_type,
    storage_type, array_storage_type, reference)
  Tests: mir/layout/tests.rs::maps_every_cc_value_shape_to_its_specified_mir_type
    (Integer->I32, Boolean->Boolean, Number->F64, String->I32,
    Repr/Aggregate/Closure/Erased references with copied nullability; product
    fields I32,I32,F64 immutable);
    mir/layout/tests.rs::plans_the_uniform_closure_and_capture_array_layout
  Input boundary: CC RepresentationTable
  Commands: cargo test -p psrs-backend mir::layout
  Result: pass, 8 executed, 0 skipped
  Gaps: none
DR-02:
  Implementation: mir/reachable.rs (sorted worklist), mir/layout/mod.rs
    (one RecGroup, repr_indices)
  Tests: mir/layout/tests.rs::plans_mutually_recursive_requirements_in_one_recursion_group
    (one RecGroup, distinct ReprId->DefinedTypeId, forward references in range,
    case subtype after its non-final supertype);
    crates/psrs-driver/tests/data_representation_execution.rs compiles and runs
    the recursive generic-array fixture through a valid Wasm type section
  Input boundary: CC RepresentationTable
  Commands: cargo test -p psrs-backend mir::layout; PSRS_REQUIRE_WASMTIME=1
    cargo test -p psrs-driver --test data_representation_execution
  Result: pass, executed under Wasmtime
  Gaps: none
DR-03:
  Implementation: mir/layout/mod.rs (Product -> immutable struct, in order);
    cc/lower/record/mod.rs lower_record_update (read unchanged fields + ProductNew)
  Tests: maps_every_cc_value_shape_to_its_specified_mir_value_type asserts
    canonical field order and immutable fields; driver records.rs
    runs_a_record_update_through_a_gc_struct, preserves aliases (arrays.rs
    preserves_the_original_gc_array_when_updating_a_copy), and the mandatory
    product/record case in data_representation_execution.rs
  Input boundary: source and CC
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
  Result: pass, value-sensitive exit codes
  Gaps: none
DR-04:
  Implementation: mir/layout/mod.rs (variant supertype non-final {i32} + final
    case subtypes {i32, fields}); cc/lower/constructor.rs (nullary -> Constant tag)
  Tests: plans_field_bearing_variants_as_tag_carrying_supertypes (hierarchy, tag
    at field 0, case fields at 1..); mandatory field-bearing and all-nullary
    cases in data_representation_execution.rs
  Input boundary: CC and source
  Commands: cargo test -p psrs-backend mir::layout; PSRS_REQUIRE_WASMTIME=1
    cargo test -p psrs-driver --test data_representation_execution
  Result: pass
  Gaps: none
DR-05:
  Implementation: cc/layout/mod.rs (newtype_ids excluded from enum/aggregate
    sets), cc/layout/scalar.rs newtype_field_type, cc/lower/constructor.rs
  Tests: driver adts.rs erases_a_newtype_constructor_and_pattern_at_runtime
    (asserts no wrapper representation and no struct.new), and
    selects_nested_patterns_through_an_erased_newtype
  Input boundary: source
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
  Result: pass
  Gaps: none
DR-06:
  Implementation: mir/layout/mod.rs array_storage_type (mutable, nullable
    reference storage); mir/lower/assignments.rs ArrayClone/ArraySet;
    wasm/lower/structure/arrays.rs emit_array_clone; cc/lower/array.rs
    lower_array_update (clone then set)
  Tests: mir/gc_tests/array.rs::executes_a_pure_array_clone_and_update (updated
    clone element 42 + preserved source element 20 + length 2 = 64 under
    Wasmtime); mir/verify/tests/arrays.rs
    rejects_array_clone_with_non_defaultable_element_storage and
    rejects_array_clone_of_an_immutable_array; driver arrays.rs alias cases
  Input boundary: CC and malformed MIR
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend
  Result: pass, negative checks reject the previously accepted malformed clones
  Gaps: none
DR-07:
  Implementation: mir/layout/mod.rs closure/capture layout; mir/lower/assignments.rs
    ClosureNew/ClosureCall/ClosureGetCapture; wasm/lower/structure/closure.rs
  Tests: plans_the_uniform_closure_and_capture_array_layout (closure struct
    {(ref func), (ref capture)}, capture array (mut (ref null eq)));
    mir/verify/tests/gaps.rs rejects_a_closure_capture_projection_with_a_non_eq_reference_result
    and ..._with_an_unrepresentable_result; driver functions.rs capturing-closure
    cases; mandatory closure case in data_representation_execution.rs
  Input boundary: CC and malformed MIR
  Commands: cargo test -p psrs-backend; PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
  Result: pass
  Gaps: capture count is dynamic, so index bounds remain a runtime (Wasm) check
DR-08:
  Implementation: mir/lower/aggregate/mod.rs BoxScalar/UnboxScalar/RecoverReference;
    mir/verify/array_map/mod.rs verify_conversion_helpers
  Tests: gaps.rs rejects_a_conversion_helper_that_casts_even_with_an_unrelated_rebuild
    and accepts_a_conversion_helper_that_rebuilds_the_aggregate; existing
    erased-field/box driver tests; mandatory erased-field case in
    data_representation_execution.rs
  Input boundary: malformed MIR and source
  Commands: cargo test -p psrs-backend; PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
  Result: pass
  Gaps: none
DR-09:
  Implementation: mir/verify/instruction/** and mir/verify/call.rs
  Tests: the negative array, closure-capture, conversion-helper, and capability
    fixtures above; valid modules validate and execute
  Input boundary: malformed MIR
  Commands: cargo test -p psrs-backend; PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
  Result: pass; newly rejected: non-defaultable/immutable array.clone,
    non-eq/unrepresentable capture projection
  Gaps: none
DR-10:
  Implementation: mir/verify/array_map/mod.rs (verify_array_maps,
    path_escapes_without_store)
  Tests: mir/verify/array_map/tests.rs (early read/return/branch, missing store,
    non-unit increment, store outside loop); mir/gc_tests/array.rs executes a
    completed map
  Input boundary: malformed MIR
  Commands: cargo test -p psrs-backend
  Result: pass
  Gaps: none
DR-11:
  Implementation: mir/layout/mod.rs plan_selected (UnsupportedGcTarget /
    UnsupportedClosureTarget); mir/verify/capability.rs validate_target_capabilities
  Tests: mir/layout/tests.rs gc_planner_rejects_an_mvp_only_target;
    mir/verify/capability.rs closure_operations_require_typed_function_references
    and i31_operations_require_reference_types, plus the existing GC/reference/
    multi-value gates
  Input boundary: CC and malformed MIR with reduced profiles
  Commands: cargo test -p psrs-backend capability; cargo test -p psrs-backend mir::layout
  Result: pass
  Gaps: none
DR-12:
  Implementation: mir/gc_tests/mod.rs run_gc now honors PSRS_REQUIRE_WASMTIME;
    psrs-driver/tests/data_representation_execution.rs
  Tests: executes_representative_reachable_layouts (products, field-bearing and
    nullary sums, arrays, closures, erased fields, generic aggregate conversion)
    with value-sensitive exit codes; mir/gc_tests/array.rs and variant.rs run
    componentized artifacts; psrs-driver/tests/wasmtime_required.rs baseline
  Input boundary: source and CC
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
  Result: pass, all runtime cases executed under Wasmtime 49.0.0, 0 skipped
  Gaps: none
```

## Discovered obligations

The audit tightened three checks that the present-tense design requires but the
verifier did not enforce:

- `ArrayClone` lowers to `array.new_default` + `array.copy`, so the verifier now
  rejects an immutable array type and non-defaultable element storage
  (`mir/verify/instruction/arrays.rs`). Wasm validation rejects both, so the
  previous acceptance could have emitted an invalid module.
- `ClosureGetCapture` now rejects a projection whose result is a non-`eq`
  reference or an unrepresentable scalar, matching the closure capture boxing
  contract (`mir/verify/call.rs`).
- Capability inference now marks closure operations as requiring typed function
  references and `i31` operations as requiring reference types
  (`mir/verify/capability.rs`).

## Remaining work and blockers

The physical layout families are verified on this revision. Open items outside
this topic:

- Open record rows and unknown foreign aggregate layouts remain unsupported
  conversions, owned by [generic aggregate erasure](../../design/backend/fp/generic-aggregate-erasure.md).
- The `i31` optimization for nullary cases of mixed sums and GC strings remain
  future work in the design.
- The capture-array element mutability bit is part of the planned layout but is
  not re-checked by the verifier, because the closure operations only read the
  array; no lowering can observe the difference.
- Official-suite execution gates retain their own owner
  ([D-04](../../design/D-04-suite-roadmap.md#backend-feature-matrix)).
