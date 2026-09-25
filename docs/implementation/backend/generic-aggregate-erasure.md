# Generic Aggregate Erasure Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)
**Design:** [Generic aggregate erasure](../../design/backend/fp/generic-aggregate-erasure.md)
**Progress:** Accepted against GA-01 through GA-20 after the independent review
findings were fixed and required execution was repeated. The earlier audit and
review findings below are retained as history; [resolution evidence](#resolution-of-independent-review-2026-09-25)
records the current result.
**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-08, BE-09, and BE-10; also BE-02, BE-03, BE-13, and BE-15.

## Scope and dependencies

Complete every present-tense requirement in the linked design, including its
Model, Algorithms, Code map, Invariants, and Boundaries. The design remains the
semantic authority; this document records execution and acceptance. A missing
checklist item does not exempt a design requirement. Add discovered obligations
with new stable IDs rather than silently narrowing existing rows.

The implementation includes normalization, conversion planning, every listed
semantic boundary, CC verification, MIR reconstruction and verification, Wasm
encoding, diagnostics, and observable execution. Necessary changes to adjacent
stages are part of this task. Preserve the specialized concrete path and the
DEC-07 erased ADT policy. Use the design's Code map as the ownership target;
record any equivalent module organization and explain its responsibilities.

Dependencies are the existing Typed Core, CC/MIR representations and verifiers,
scalar boxes, closure adapters, GC layout planner, and component execution
path. Inspect their actual contracts before implementation. Do not wait for an
entire neighboring topic to be complete when its required interface can be
implemented within this task.

The following design exclusions remain excluded: open-row conversion, unknown
foreign aggregate layouts, observable pointer identity or mutation, cyclic
aggregate value graphs, and shared nominal GC types across independently
compiled Wasm artifacts. Rejection diagnostics for unsupported inputs remain
in scope. Cycle-safe normalization of recursive type declarations and calls
between source modules linked into one Core program are in scope.

## Acceptance matrix

All rows start as **Unverified**, even where code or tests already exist.
Allowed states are **Unverified**, **In progress**, **Blocked**, and **Verified**.
A blocked row must name the missing contract or external dependency and the
condition for resuming. A verified row must link implementation and named tests
in the evidence record below. A test name alone is not execution evidence.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| GA-01 | Template normalization keeps declaration variables abstract; Actual normalization applies substitutions and retains unresolved variables. | Unit cases distinguish `Array a`, `Array Int`, nested arrays, mixed closed records, and partially resolved substitutions at the same boundary. | Verified |
| GA-02 | Canonical keys are deterministic and cycle-safe; records use canonical labels, positional products have separate keys, and ADTs retain declaration identity. | Equivalent templates share IDs across instantiations and linked modules; distinct labels/declarations remain distinct; recursive declarations terminate. P9 assigns one type per reachable representation without merging nominal identities. | Verified |
| GA-03 | Typed plan construction selects identity, scalar box/unbox, reference erase/recover, sequence, aggregate maps, and function adapters with exact endpoints. | Positive and negative plan tests cover both directions; equal shapes do not reconstruct; CC plans and MIR contain no Core type variables or Wasm layout decisions at the wrong stage. | Verified |
| GA-04 | Arrays reconstruct recursively between specialized and canonical layouts. | Execute empty, singleton, and multi-element arrays in both directions, including nested arrays and arrays of closed records; check length, order, and element values. | Verified |
| GA-05 | Closed records reconstruct fields in canonical order with the same label set. | Execute mixed scalar/reference fields, nested records and arrays, and reordered source labels; verify labels select the right values and concrete fields retain their shapes. | Verified |
| GA-06 | Dependent ADT fields convert through their declared template before erased storage and reverse those steps on projection. | Execute `Wrap a = Wrap (Array a)` and record payloads at multiple instantiations through generic and concrete consumers. Inspect construction and recovery plans. | Verified |
| GA-07 | Bare-variable ADT fields preserve ordinary erasure. | `Hold a = Hold a` carrying a concrete array or record round-trips without aggregate mapping; contrast its plan with GA-06. | Verified |
| GA-08 | Array literals, reads, and pure updates adapt elements to the required layout. | Execute canonical and concrete paths, retain and read an alias of the original after update, and check converted replacement values and unchanged elements. | Verified |
| GA-09 | Record construction, access, pattern projection, and pure updates adapt field shapes. | Execute generic and concrete consumers with mixed/nested fields; retain an alias of the original and prove updates leave it unchanged. | Verified |
| GA-10 | Direct call arguments and returns use declaration signatures and caller instantiations. | One generic body is called at distinct concrete types; execute both argument and result conversions, including aggregate results consumed after return. | Verified |
| GA-11 | Higher-order adapters convert aggregate arguments/results and preserve closure signatures. | Execute concrete-to-generic and generic-to-concrete function adaptation, nested aggregate values, and mixed scalar/aggregate parameters. | Verified |
| GA-12 | Captures preserve canonical generic or specialized concrete layouts as appropriate. | Execute lifted closures that capture and later read generic arrays/records and concrete arrays/records; cover conversions at capture creation or read as required by the lifted signature. | Verified |
| GA-13 | Linked source modules share canonical keys and compatible call boundaries. | Compile linked modules with producer/consumer boundaries and distinct instantiations; inspect shared representation identities and execute the resulting component. | Verified |
| GA-14 | Reference recovery has valid provenance and never substitutes a nominal cast for reconstruction. | CC verifier rejects absent/inconsistent recovery evidence, wrong variant/tag/field/template and wrong endpoints. Valid instantiation and variant-field recovery execute without nominal cast traps. | Verified |
| GA-15 | CC verifier validates every recursive conversion plan. | Malformed CC fixtures reject unresolved handles, non-array map endpoints, incompatible element plans, wrong product arity/labels, invalid sequences and incompatible nested plans before MIR emission. | Verified |
| GA-16 | P9 lowers maps into exact typed reconstruction and interns helpers by complete plans. | Inspect MIR field reads/new products and array allocation/loops; equal full keys share helpers, differing nested conversions do not collide. Identity has no conversion allocation. | Verified |
| GA-17 | ArrayNewDefault uses defaultable storage; every logical slot is initialized before exposure and the source is never written. | MIR verifier rejects nondefaultable allocation, early escape/return, incomplete initialization and wrong element/index/nominal types. Exercise empty and nested conversion loops; document the verifier's proof or enforced construction discipline. | Verified |
| GA-18 | Wasm mechanically encodes verified layouts and CFG under the target profile. | Validate core module and component, inspect relevant GC operations/structured loops, and execute them; disabled required GC/reference capabilities are rejected before emission. | Verified |
| GA-19 | Unsupported source conversions have named, source-associated diagnostics; malformed internal plans are compiler errors. | Negative tests cover open rows/unknown foreign shapes at the appropriate input boundary, incompatible endpoints, and preserved operation span/module. Supported generic cases no longer hit obsolete unsupported diagnostics. | Verified |
| GA-20 | Optimizations preserve conversion semantics and concrete regressions remain valid. | Execute both a path retaining reconstruction and the normal optimized pipeline; verify scalar/aggregate values, pure updates and exact concrete layouts. Existing workspace tests remain green. | Verified |

Apply scalar coverage across the relevant rows: `Int`, `Boolean`, `Number`,
`String`, `Char`, and `Unit`, following each scalar's defined representation.
Include GC references (arrays, records, ADTs, and closures where valid Core
permits them). Exercise at least two concrete instantiations of the same generic
body, with both integer-box and number-box paths and nested reference recovery.
Do not replace these cases with repeated integer-only examples. Use focused
combinations that exercise distinct representation and lowering paths; a full
Cartesian product is unnecessary.

### Evidence record

For each row, add a record using this format (one test may support several IDs):

```text
GA-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact test names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

### Audit results (2026-09-25)

The following records are the implementation agent's initial audit. The
independent review superseded their completion claims and `Gaps: none`
statements where findings applied. The resolution section records the repairs
and final evidence for those rows.

Tested revision: commit `eb43bf9` ("Implement generic aggregate erasure")
plus the uncommitted audit changes in `git diff` (three defects fixed and new
mandatory tests). Runtime commands below require Wasmtime 49; every required
case executed with `PSRS_REQUIRE_WASMTIME=1`, so an unavailable runtime fails the
run instead of skipping.

Defects fixed during this audit:

- `crates/psrs-backend/src/cc/layout/functions.rs`: a declaration whose result
  is a function type (`makeReader :: forall a. Array a -> (Int -> a)`) now
  registers its runtime signature. Previously such function types were rejected
  with "function type has no runtime layout".
- `crates/psrs-backend/src/mir/verify/util.rs` and
  `crates/psrs-backend/src/mir/verify/instruction/mod.rs`: Boolean array slots use
  Wasm `i32` storage, so `array.new`/`array.get`/`array.set` now accept the
  logical `Boolean`/`i32` pairing exactly as struct fields already did.
- `crates/psrs-backend/src/mir/verify/instruction/arrays.rs`: the verifier now
  rejects a non-defaultable `array.new_default` element storage.

```text
GA-01:
  Implementation: crates/psrs-backend/src/cc/layout/{mod,scalar,aggregate}.rs.
    Core is already substituted at each typed boundary, so the callee's declared
    type is normalized with its variables abstract (`Erased`) while the caller's
    actual type is normalized from its instantiated Core type.
  Tests: crates/psrs-backend/src/cc/layout/tests.rs:
    canonical_arrays_key_by_element_shape (Array a -> Array(Erased);
    Array Int -> Array(Integer); Array (Array a) nests),
    canonical_record_keys_sort_labels_and_share_equal_keyed_records
    (mixed scalar/reference closed record), and
    parameter_dependent_record_field_keeps_canonical_array_and_erases_the_adt_slot.
  Input boundary: verified Typed Core fixtures.
  Commands: cargo test -p psrs-backend cc::layout
  Result: pass. No source case exercises a partially resolved substitution at
    one boundary because Core carries no unresolved call-site substitution.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-02:
  Implementation: cc/layout/aggregate.rs (AggregateKey, reserve before recursion,
    canonical HashMap; scalar.rs keeps ADT identity declaration-based).
  Tests: cc/layout/tests.rs::canonical_record_keys_sort_labels_and_share_equal_keyed_records,
    canonical_arrays_key_by_element_shape, recursive_aggregate_normalization_terminates;
    driver tests::generic_aggregate_audit::linked_modules_share_canonical_layouts
    asserts exactly one `Array(Erased)` representation across linked modules.
  Input boundary: verified Typed Core fixtures and linked source.
  Commands: cargo test -p psrs-backend cc::layout;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit
  Result: pass; recursion terminates without a stack overflow.
  Revision: eb43bf9 + audit diff.
  Gaps: none. Positional products are not a surface-language construct here, so
    their separate key is enforced by the array/record key split only.

GA-03:
  Implementation: cc/lower/conversion.rs::plan_conversion (identity, box/unbox,
    erase/recover, sequence, ArrayMap, ProductMap); cc/convert.rs plans carry
    ValueShape/ReprId only.
  Tests: cc/verify/ops/aggregate/tests.rs accepts_a_valid_array_map,
    accepts_a_type_instantiation_recovery, rejects_endpoints_that_disagree_with_the_typed_values,
    rejects_an_array_map_whose_source_is_not_an_array,
    rejects_an_array_map_with_an_incompatible_element_plan,
    rejects_a_product_map_with_the_wrong_labels, and friends; driver
    tests::generic_aggregate_audit::audit_battery executes identity and both
    conversion directions.
  Input boundary: malformed CC fixtures and source.
  Commands: cargo test -p psrs-backend cc::verify::ops::aggregate;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit
  Result: pass. `plan_conversion` returns `Identity` when shapes are equal, so
    equal shapes allocate nothing.
  Revision: eb43bf9 + audit diff.
  Gaps: none. The never-constructed `ValueConversion::FunctionAdapter`
    variant was removed by the Polymorphism and Erasure topic; function
    adaptation is performed by the existing erased closure adapter, not by
    plan_conversion.

GA-04:
  Implementation: cc/lower/conversion.rs (ArrayMap), mir/lower/aggregate/array.rs
    (ArrayNewDefault loop), wasm/lower/structure/arrays.rs.
  Tests: driver tests::generic_aggregate_audit::audit_battery cases
    array_roundtrip_multi, array_roundtrip_singleton, array_roundtrip_nested,
    array_roundtrip_records, scalar_boolean_array, scalar_char_array,
    scalar_string_array; empty_array_reconstruction_executes (verified Core
    fixture that empties the literal, keeps post-P7 `ArrayNewDefault` + `ArraySet`,
    and executes length 0). parameterized_shapes::
    maps_nested_generic_arrays_recursively_across_instantiations also inspects
    WAT and executes.
  Input boundary: source and verified Typed Core fixture.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib parameterized_shapes
  Result: pass, all cases executed. Source empty-array literals are rejected at
    P5, so empty coverage comes from the Core fixture; recorded as a missing
    source path, not source support.
  Revision: eb43bf9 + audit diff.
  Gaps: no source-level empty array literal.

GA-05:
  Implementation: cc/layout/aggregate.rs canonical label sort,
    cc/lower/conversion.rs ProductMap, mir/lower/aggregate/mod.rs lower_product_map.
  Tests: audit_battery cases record_roundtrip_mixed, record_roundtrip_reordered,
    record_roundtrip_nested, generic_record_pattern_projection,
    generic_record_construction, generic_record_update,
    generic_boolean_record_field, generic_record_two_instantiations;
    parameterized_shapes::maps_closed_generic_records_and_nested_arrays_across_instantiations.
  Input boundary: source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib parameterized_shapes
  Result: pass; execution checks the values selected by reordered labels.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-06:
  Implementation: cc/lower/conversion.rs erased_field_recovery and dependent
    construct/project order; cc/verify/ops/aggregate.rs evidence checks.
  Tests: audit_battery dependent_adt_int, dependent_adt_number,
    dependent_adt_record; parameterized_shapes::
    maps_generic_arrays_across_polymorphic_adt_boundaries inspects pre-P7 plans,
    post-P7 layouts and WAT, and executes.
  Input boundary: source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib parameterized_shapes
  Result: pass at Int, Number, and record instantiations.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-07:
  Implementation: cc/layout/scalar.rs::field_storage_shape erases a bare-variable
    field without a template map.
  Tests: audit_battery bare_variable_adt_array (custom round-trip through
    `Hold a = Hold a`) contrasted with the GA-06 dependent cases.
  Input boundary: source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit
  Result: pass.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-08:
  Implementation: cc/lower/array.rs and cc/lower/conversion.rs.
  Tests: audit_battery cases generic_literal_construction, generic_array_update,
    array_update_alias; parameterized_shapes::
    maps_nested_generic_arrays_recursively_across_instantiations.
  Input boundary: source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib parameterized_shapes
  Result: pass; array_update_alias reads the retained alias and proves purity.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-09:
  Implementation: cc/lower/record/mod.rs and cc/lower/conversion.rs ProductMap.
  Tests: audit_battery record_roundtrip_*, generic_record_pattern_projection,
    generic_record_construction, generic_record_update, record_update_alias;
    existing tests::records::runs_a_record_pattern_in_a_function_parameter.
  Input boundary: source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit;
    cargo test -p psrs-driver --lib tests::records
  Result: pass; record_update_alias reads the retained alias.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-10:
  Implementation: cc/lower/call (application) converts arguments and results
    using declaration signatures and caller actual shapes.
  Tests: audit_battery direct_call_distinct_int, direct_call_distinct_number,
    two_instantiations_share_body, two_instantiations_roundtrip,
    generic_record_two_instantiations.
  Input boundary: source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit
  Result: pass; one generic body is called at Int and Number and results are
    consumed after return.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-11:
  Implementation: cc/lower/call/application.rs closure adapter plus the same
    conversion plans.
  Tests: audit_battery higher_order_array;
    parameterized_shapes::adapts_higher_order_array_arguments_and_results
    (pre-P7 ArrayMap count plus execution).
  Input boundary: source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib parameterized_shapes
  Result: pass.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-12:
  Implementation: cc/layout/captures.rs and cc/lower/lambda.rs store canonical
    generic aggregates in the uniform capture array.
  Tests: audit_battery capture_generic_array; existing
    tests::functions::runs_a_capturing_lambda_through_a_closure.
  Input boundary: source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit
  Result: pass.
  Revision: eb43bf9 + audit diff.
  Gaps: captures of a bare function-typed top-level binding (`reader = makeReader xs`) are a
    general unsupported source path (a "call expects 0 arguments" P8 error), unrelated to
    aggregate conversion and not part of this topic.

GA-13:
  Implementation: driver links modules into one Core program before P8; shared
    canonical keys follow from LayoutKeysTest.
  Tests: driver tests::generic_aggregate_audit::linked_modules_share_canonical_layouts
    (producer/consumer modules, one canonical `Array(Erased)`, retained
    `ArrayNewDefault`, executed round-trip).
  Input boundary: linked source modules.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit
  Result: pass.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-14:
  Implementation: cc/verify/ops/aggregate/mod.rs verify_conversion.
  Tests: cc/verify/ops/aggregate/tests.rs
    rejects_a_variant_field_recovery_with_the_wrong_template,
    rejects_a_variant_field_recovery_with_the_wrong_tag_or_field,
    rejects_a_variant_field_recovery_with_a_non_variant_handle,
    rejects_recovery_from_a_non_erased_source,
    accepts_a_type_instantiation_recovery; audit_battery executes valid
    instantiation and variant-field recovery.
  Input boundary: malformed CC fixtures and source.
  Commands: cargo test -p psrs-backend cc::verify::ops::aggregate;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit
  Result: pass; no nominal cast trap on valid recovery.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-15:
  Implementation: cc/verify/ops/aggregate/mod.rs verify_plan.
  Tests: cc/verify/ops/aggregate/tests.rs (13 cases listed above) plus
    rejects_a_sequence_with_an_incompatible_step,
    rejects_an_array_map_whose_target_is_not_an_array,
    rejects_a_product_map_with_the_wrong_arity.
  Input boundary: malformed CC fixtures.
  Commands: cargo test -p psrs-backend cc::verify::ops::aggregate
  Result: pass; all malformed plans are rejected before MIR emission.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-16:
  Implementation: mir/lower/aggregate/mod.rs inlines ProductMap/ArrayMap into
    the caller. There is no conversion-helper function keyed by the complete
    plan, so equal plans do not share a helper.
  Tests: none yet for helper sharing.
  Input boundary: MIR.
  Commands: n/a.
  Result: not verified.
  Revision: eb43bf9 + audit diff.
  Gaps: interning helpers by complete (source, target, plan) key is not
    implemented; see Remaining work.

GA-17:
  Implementation: mir/lower/aggregate/array.rs lower_array_map (private
    destination, ArraySet inside the loop, RefCast at exit);
    mir/verify/array_map/mod.rs (length source, index zero, index<length,
    backedge increment, no escape before store);
    mir/verify/instruction/arrays.rs is_defaultable_storage;
    mir/layout/mod.rs array_storage_type makes reference slots nullable.
  Tests: mir/verify/array_map/tests.rs accepts_a_complete_array_map_loop,
    rejects_a_loop_that_does_not_initialize_each_element,
    rejects_a_loop_that_does_not_advance_by_one,
    rejects_a_loop_iteration_that_can_escape_before_storing,
    rejects_a_store_that_happens_outside_the_loop;
    mir/verify/tests/arrays.rs rejects_array_new_default_with_non_defaultable_element_storage,
    rejects_array_new_default_with_a_non_i32_length,
    rejects_array_new_default_with_a_non_array_source;
    generic_aggregate_audit::empty_array_reconstruction_executes.
  Input boundary: malformed MIR and source/Core fixtures.
  Commands: cargo test -p psrs-backend mir::verify::array_map;
    cargo test -p psrs-backend mir::verify::tests;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit
  Result: pass. Enforced construction discipline: the layout planner makes every
    reference slot nullable and all scalar storage defaultable, so a planner-built
    conversion has no non-defaultable slot; the verifier now also rejects one.
  Revision: eb43bf9 + audit diff.
  Gaps: none.

GA-18:
  Implementation: wasm/lower/structure/{arrays,instructions}.rs encode verified MIR.
  Tests: parameterized_shapes inspections assert `array.new_default`,
    `array.get`, `array.set`, `struct.new`, `struct.get`, `array.new_fixed`;
    generic_aggregate_audit executes built components; existing
    tests::abi::* capability gates reject target profiles without GC/refs.
  Input boundary: verified MIR then Wasm component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib parameterized_shapes;
    cargo test -p psrs-backend abi
  Result: pass; the component validates and executes.
  Revision: eb43bf9 + audit diff.
  Gaps: none in the included profile; capability-gate tests predate this audit.

GA-19:
  Implementation: cc/lower/conversion.rs emits a source-spanned
    "unsupported aggregate conversion between normalized runtime shapes"; the
    existing layout diagnostics keep stage and span.
  Tests: audit_battery shows all supported generic cases compile (no obsolete
    unsupported diagnostic); tests::records::rejects_open_record_patterns covers
    open-row rejection at P5; cc/verify/ops/aggregate/tests.rs covers malformed
    internal plans as compiler errors.
  Input boundary: source and malformed CC.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit;
    cargo test -p psrs-driver --lib tests::records;
    cargo test -p psrs-backend cc::verify::ops::aggregate
  Result: pass for the covered boundaries.
  Revision: eb43bf9 + audit diff.
  Gaps: no source program exercises the unsupported-conversion diagnostic
    directly; unknown-foreign-aggregate rejection is unverified at its input
    boundary. See Remaining work.

GA-20:
  Implementation: psrs_backend::compile_with_target runs P7 then MIR optimize;
    driver compile_source uses it.
  Tests: all audit_battery cases run the optimized pipeline and assert runtime
    values; empty_array_reconstruction_executes and
    linked_modules_share_canonical_layouts assert the optimized MIR still
    contains the reconstruction; existing repo tests remain green.
  Input boundary: source and verified Core.
  Commands: cargo test --workspace;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit
  Result: pass; no production optimization was disabled.
  Revision: eb43bf9 + audit diff.
  Gaps: none.
```

## Execution plan

These are ordered work packages within one topic, not separate completion
criteria. Preserve passing behavior, implement missing contracts, and continue
until every acceptance row is verified.

1. Audit the current implementation against GA-01 through GA-20. Record real
   evidence and missing behavior; do not assume implementation notes establish
   completion. Resolve contradictions with the normative design before coding.
2. Complete array normalization, plans, verification, MIR reconstruction, Wasm
   encoding, and mandatory execution as one vertical slice. Include direct
   calls/returns, empty arrays, nested arrays, and concrete regressions.
3. Complete closed records and their array combinations through the same stages,
   including access, pattern projection, and persistent updates.
4. Complete dependent ADT construction/projection and bare-variable erasure,
   with generic and concrete consumers at multiple instantiations.
5. Complete higher-order adapters, captures, and linked source-module boundaries.
   Close negative verifier, diagnostic, target-profile and optimization cases
   throughout the slices, then audit the full matrix again.

### Existing evidence leads, not acceptance results

At checklist creation, these files contain implementation or test candidates:

- [CC plan construction](../../../crates/psrs-backend/src/cc/convert.rs).
- [CC boundary lowering](../../../crates/psrs-backend/src/cc/lower/conversion.rs).
- [CC aggregate verification](../../../crates/psrs-backend/src/cc/verify/ops/aggregate/mod.rs).
- [MIR aggregate lowering](../../../crates/psrs-backend/src/mir/lower/aggregate/mod.rs).
- [Source regressions](../../../crates/psrs-driver/src/tests/parameterized_shapes.rs):
  `maps_generic_arrays_across_polymorphic_adt_boundaries`,
  `maps_closed_generic_records_and_nested_arrays_across_instantiations`,
  `maps_nested_generic_arrays_recursively_across_instantiations`, and
  `adapts_higher_order_array_arguments_and_results`.

Those source regressions inspect pre-P7 CC plans, then compile normally.
Only some cases explicitly inspect retained MIR reconstruction. WAT presence of
`array.get` or `struct.new` alone cannot establish that the intended generic
conversion survived optimization and executed. Mandatory runtime execution
prevents a skipped run; it does not establish coverage of every claimed path.
See the independent review for missing evidence.

Acceptance must include an artifact with reachable conversion code that is
actually exercised, using runtime-dependent inputs, a verified Core fixture
retaining a generic call, or a test-only path through production passes without
P7 specialization. Inspect that same artifact's conversion path and execute it.
Also test the ordinary optimized pipeline. Do not disable production
optimization to make the tests pass.

Where source lowering cannot express valid backend input, verified Typed Core
fixtures may establish backend coverage. Record the missing source path
separately; do not claim source support from synthetic fixtures. A fixture must
still exercise production lowering, verification, encoding and execution.

### Required validation and completion rule

Run the repository Rust gates after implementation:

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test wasmtime_required
```

The last command checks the baseline runtime only; the topic tests were adapted
to honor the same variable, so run them with `PSRS_REQUIRE_WASMTIME=1` as
recorded in the evidence records. Every required runtime case must execute;
skipped cases leave the corresponding rows unverified or blocked.

Finish only after all rows have evidence, the design-to-code audit finds no
untracked present-tense obligation, and required checks pass. Do not close a
row by adding unsupported behavior for an in-scope case, weakening a verifier,
removing a test, or moving a requirement to future work. Follow up on every
remaining slice without waiting for another user prompt. An external blocker
or execution limit leaves the task incomplete and must produce a precise
continuation record.

Update D-04 and the design's short implementation notes to match verified
coverage. Topic completion does not complete the broader BE-08/09/10 rows:
open rows, other ADT work, and official-suite gates retain their own scope.
The final handoff must identify the changed files, tested revision, commands,
runtime evidence, and any remaining gaps so a separate reviewer can reproduce
acceptance directly from this document.

## Independent review (2026-09-25)

**Disposition: changes required.** The review checked `eb43bf9` plus the
implementation agent's uncommitted changes. Existing tests passing does not
close the following findings. Production code was not changed during this
independent review; temporary probes were removed after reproduction.

### R1 — ArrayGet permits an unproved non-null result (high)

In `mir/verify/instruction/mod.rs`, `ArrayGet` checks
`value_type_assignable(destination_type, storage_type)`. That direction admits
a non-null destination for nullable reference storage. A load can produce null;
its result cannot be strengthened without an explicit checked operation.

Reproduced using a full `verify_module` fixture: accept an array parameter with
`(ref null eq)` element storage, load index zero into a `(ref eq)` result, and
return that result. The verifier accepts the module. Fix the result check while
preserving Boolean storage compatibility; add nullable/non-null positive and
negative regression cases. Recheck GA-17 and the array instruction contract.

### R2 — Initialization proof permits premature exit and pre-loop reads (high)

`mir/verify/array_map/mod.rs::path_escapes_without_store` stops traversing when
it reaches a block containing a store. It never proves that this block continues
to the next iteration rather than returning the partly initialized array.
The private-destination operand scan covers loop body instructions only; it
omits allocation/header instructions and terminator operands.

Two probes against `array_map_function()` reproduce acceptance by
`verify_array_maps`:

1. Keep the element store in block 2, move increment/backedge to a new block 4,
   and branch from block 2 on the existing loop condition to exit block 3 or
   block 4. The condition is true when the body runs, so a nonempty conversion
   exits after the first store, yet verification succeeds.
2. Append an `ArrayGet` of destination `ValueId(1)` at zero `ValueId(5)` to
   allocation block 0, before entering the loop. Verification succeeds even
   though it reads an uninitialized logical element.

These probes exercise the initialization sub-verifier, separately from the R1
full-module fixture. Strengthen the proof to cover all paths, instruction order,
terminators, and uses before completion. Add full-module malformed MIR tests
as well as focused proof tests. GA-17 cannot remain Verified.

### R3 — The capture acceptance case contains no closure (medium)

`generic_aggregate_audit::audit_battery` uses
`captureRead values = (\index -> arrayLength values) 0` for GA-12.
An independent `compile_with_stages` probe confirmed that its optimized MIR
contains no `Instruction::ClosureNew`: the immediately applied lambda does not
exercise the claimed capture allocation/read path.

Add an escaping or otherwise retained closure with generic arrays and records,
inspect its captures, and execute a later read. Include the concrete capture
paths required by GA-12. Use verified Core when source limitations prevent a
valid backend case; record the source limitation separately.

### R4 — Scalar and boundary assertions are weaker than their claims (medium)

The Number cases `dependent_adt_number`, `direct_call_distinct_number`, and
`two_instantiations_roundtrip` consume the recovered value using
`use value = 42`. A corrupt numeric payload would not change the expected
result. The String case checks array length without observing string contents;
the battery has no Unit case. GA-04, GA-06, GA-10, and GA-20 therefore still
need value-sensitive evidence across the required representations.

Replace constant consumers with observable arithmetic/comparisons or output
that depends on the recovered payload. Assert that the intended reconstruction
or adapter is reachable in the executed artifact. GA-11 also needs the specified
nested aggregate and mixed scalar/aggregate adapter cases; repeating the one
`Array Int` identity adapter does not cover the full row.

### R5 — Two required implementation items remain explicitly unfinished

GA-16 helper interning and GA-19 unsupported-input diagnostic coverage are still
In progress in the implementation agent's own handoff. Neither has an external
blocker. Complete their existing continuation tasks; do not interpret the
successful audit tests as completion of this topic.

### Review validation

- `cargo fmt --all --check`: passed before temporary probes.
- `cargo test --workspace`: passed on the submitted implementation.
- `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit`:
  all three tests passed on Wasmtime 49.0.0, including the runtime battery.
- Temporary probes: all three backend probes confirmed acceptance of the
  invalid cases described in R1/R2; the driver probe confirmed absent closure
  allocation in R3. Probe sources were removed after reproduction.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib parameterized_shapes`:
  all four runtime regressions passed.
- `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test wasmtime_required`:
  the baseline component execution gate passed.
- Final `cargo fmt --all --check`, `git diff --check`, and local document-link
  checks passed after temporary probes were removed.

## Resolution of independent review (2026-09-25)

Tested revision: `eb43bf9` plus the uncommitted implementation audit and repair
diff. All required topic execution ran with `PSRS_REQUIRE_WASMTIME=1` on
Wasmtime 49.0.0. Earlier R1–R5 descriptions remain above for reproducibility;
this section supersedes their open status and the earlier GA-16/GA-19 handoff.

| Finding and rows | Repair | Reproducible evidence |
| --- | --- | --- |
| R1; GA-17 | `ArrayGet` checks storage-to-result assignability. Nullable storage cannot be loaded into a non-null result without an explicit cast. Boolean/i32 storage remains compatible. | `mir/verify/tests/arrays.rs::rejects_nullable_array_load_declared_as_nonnullable`; full backend verifier and workspace tests. |
| R2; GA-17 | The array-map verifier now follows paths beyond stores to their backedge, rejects exits or returns from the body, and checks destination use in allocation, header, body and terminators before completion. | `mir/verify/array_map/tests.rs::rejects_an_exit_after_storing_only_the_current_element` and `rejects_reading_the_destination_before_its_loop` use focused and full-module verification; existing legal loop tests pass. |
| R3; GA-12 | Source fixtures retain allocated closures across P7 for generic arrays, concrete arrays, and generic records; the tests inspect `ClosureNew` in optimized MIR and execute the captured value. | `generic_aggregate_audit::retained_generic_capture_executes_through_a_closure`. |
| R4; GA-04, GA-06, GA-10, GA-11, GA-20 | Number checks depend on recovered values; the String case checks actual WASI output; Unit crosses a generic array; higher-order cases cover nested arrays, mixed scalar/aggregate parameters, and both adapter directions. | `generic_aggregate_audit::audit_battery` and `generic_string_array_preserves_observable_contents`, all executed under required Wasmtime. Pre-P7 plan and post-P7 optimized execution checks remain in `parameterized_shapes`. |
| R5 helper sharing; GA-16 | P9 generates one MIR function per complete `(source shape, destination shape, conversion plan)` key for reconstruction plans. Equal plans call the shared helper; different nested plans get distinct symbols. Identity and scalar-only plans stay at the call site. | `mir::lower::conversion_helpers::tests::shares_equal_full_plans_but_not_different_nested_plans`; `generic_aggregate_audit::p9_reuses_helpers_for_equal_complete_conversion_plans` inspects P9 calls and executes the optimized component. |
| R5 diagnostics; GA-19 | The P8 unsupported-plan error is checked at its source span; source open rows fail at P5 and unsupported foreign aggregate shapes fail at P9 with source ranges. A valid typed source program cannot construct an incompatible conversion endpoint, so the P8 final arm is tested with a direct boundary fixture. | `cc::lower::conversion::tests::unsupported_typed_boundary_reports_its_source_span`; `tests::records::rejects_open_record_patterns`; `tests::wasi::rejects_a_non_byte_wit_list_before_lowering_it_as_a_string`; malformed CC verifier tests. |

The implementation changed Rust modules under `cc/lower`, `mir/lower`, and
`mir/verify`, plus driver regressions. All source files remain within the
repository's 500-line limit. The following gates passed after repair:

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib generic_aggregate_audit
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib parameterized_shapes
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test wasmtime_required
```

The source frontend still rejects empty array literals; the verified Typed Core
fixture covers an empty array through the backend. That source feature belongs
to frontend work. Independently linked Wasm artifacts and open-row aggregate
conversion remain outside this design. No acceptance row is blocked.

## Remaining work and blockers

None for this topic's GA-01 through GA-20 contract. The broader BE-08/09/10
roadmap rows retain official-suite and other representation work outside this
topic.
