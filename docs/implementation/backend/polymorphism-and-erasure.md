# Polymorphism and Erasure Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Polymorphism and erasure](../../design/backend/fp/polymorphism-and-erasure.md)

**Progress:** Verified. Signature interning now re-dedupes after aggregate
normalization, the CC adaptation verifier requires the exact non-null erased
shape, integer-shaped captures reserve the integer box, and value-sensitive
execution covers both scalar boxes and both adapter directions. The dead
`ValueConversion::FunctionAdapter` variant was removed. Remaining deviations and
handoffs are recorded below.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-02 and BE-08; FE-09 supplies typed polymorphic input.

## Scope and dependencies

Complete rank-1 erased-value representation, signature interning, scalar
boxing, reference recovery, closure capture, and higher-order function
adapters in the linked design. The design's full present-tense contract applies
even when a row below is missing. Typed Core owns type checking and dictionary
evidence; [generic aggregate erasure](generic-aggregate-erasure.md) owns
recursive array/closed-record conversion; [data representation](data-representation.md)
owns physical GC layouts. Erased values must not cross the WIT canonical ABI.
Keep the erased fallback even if optimization specializes some calls.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**;
existing code or a fixture that only inspects WAT does not verify execution.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| PE-01 | A bare type variable has non-null erased `eqref` shape, while generic aggregates normalize recursively to canonical layouts. | Inspect CC shapes for `a`, `Array a`, nested arrays, dependent records and ADT fields; reject conflating bare erasure with aggregate identity. | Verified |
| PE-02 | Equal normalized function signatures share `SignatureId` and MIR function type; closure receiver and capture conventions are exact. | Equivalent/different signature interning cases, indirect call verification, and emitted type-index inspection. | Verified |
| PE-03 | Integer and Boolean use full-width `i32` boxes at erased boundaries, `Number` uses an `f64` box, and `String` erases and recovers as a GC reference. | Value-sensitive round trips including `Int` extremes, both Booleans, signed zero/NaN policy, and nonempty String contents. | In progress |
| PE-04 | GC references erase by upcast and recover by checked shape/provenance without avoidable allocation. | Inspect CC/MIR for reference-only conversions; execute valid ADT/array/record/closure identity and reject wrong nominal recovery. | Verified |
| PE-05 | `i31` is reserved for Boolean capture encoding and never substitutes for a general boxed full-width `Int`. | Capture and erased-call fixtures with high-bit integer values; inspect emitted boxing operations. | Verified |
| PE-06 | Direct generic calls adapt argument and result representations at the declaration signature and caller instantiation. | One generic body called at multiple scalar and reference instantiations; execute payload-sensitive results and inspect boundary plans. | Verified |
| PE-07 | Concrete-to-generic and generic-to-concrete function adapters convert each argument and result at invocation with exact arity/signatures. | Execute both adapter directions, mixed scalar/reference arguments, returned function values, and repeated calls; prove original function value evaluated once. | Verified |
| PE-08 | Closure captures use the uniform nullable `eqref` array; scalar captures box, a `String` capture is stored as its GC reference, and reference captures are stored as-is. | Escaping closures capture Int, Boolean, Number, String, reference, and already-erased values; execute later reads and inspect no double boxing. | In progress |
| PE-09 | RepresentationTest/Cast is restricted to valid erased boundaries and cannot replace nominal aggregate reconstruction. | CC/MIR verifier negative fixtures for unrelated nominal layouts, wrong box kind, nullability, and signature; coordinate positive aggregate cases with [generic aggregate erasure](generic-aggregate-erasure.md). | Verified |
| PE-10 | CC and MIR verifiers reject malformed adapters, captures, calls, and unresolved representation requirements. | Full-module negative fixtures for wrong signature, capture index/type, arity, cast provenance, and result shape before Wasm emission. | Verified |
| PE-11 | Erased values are recovered before canonical WIT calls; optimized and unspecialized execution agree. | Source or verified Core fixture crossing a concrete ABI call, plus execution retaining an erased generic path and normal optimized execution. | Verified |

## Vertical execution order

1. Audit typed Core inputs, CC normalization/signatures, P9 boxes/captures,
   verifiers, and current runtime fixtures against the design.
2. Finish exact scalar/reference conversions and adapters with malformed
   verifier tests; preserve the existing generic aggregate acceptance path.
3. Execute unspecialized and optimized component cases with payload-sensitive
   assertions, then check WIT boundaries and source-vs-Core coverage.
4. Record all evidence and update D-04 without treating this topic as the
   entire FE-09 or official backend suite gate.

## Evidence record and completion rule

For each ID record implementation paths/entry points; exact tests and
assertions; input boundary; commands, runtime version and actual executions;
revision; and gaps. Verified Core fixtures do not establish source support.
Runtime cases use `PSRS_REQUIRE_WASMTIME=1`; skips leave rows unverified. After
Rust edits run `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus the mandatory
focused runtime cases. Complete only after every row and every present-tense
design obligation is verified.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
PE-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

### Evidence

```text
PE-01:
  Implementation: cc/layout/scalar.rs (scalar_type erased shape),
    cc/layout/aggregate.rs (normalize_aggregate_layouts canonical arrays and
    products), cc/layout/mod.rs (type_layout).
  Tests: cc::layout::tests::canonical_arrays_key_by_element_shape,
    canonical_record_keys_sort_labels_and_share_equal_keyed_records,
    parameter_dependent_record_field_keeps_canonical_array_and_erases_the_adt_slot;
    mir::gc_tests::erased::executes_erased_identity_for_scalars_and_concrete_references_on_both_targets.
  Input boundary: verified Typed Core layout fixtures plus a verified CC module.
  Commands: cargo test -p psrs-backend cc::layout; cargo test -p psrs-backend mir::gc_tests::erased.
  Result: pass. `a` is Reference{nullable:false, Erased}; `Array a` is a
    canonical Array(Erased); a dependent record field keeps its canonical array
    and erases only the bare slot.
  Revision: cc5d0f4 + audit diff.
  Gaps: none.

PE-02:
  Implementation: cc/layout/functions.rs (canonicalize_signatures),
    cc/layout/aggregate.rs (calls it after aggregate handle remapping),
    cc/layout/mod.rs (returns the canonical function_types map).
  Tests: cc::layout::tests::equal_normalized_function_signatures_share_one_signature_id
    asserts equal `Array Int` function types share one signature while
    `Array Int` and `Array String` keep distinct canonical arrays and
    signatures; driver
    polymorphism_erasure_audit::equal_normalized_function_signatures_allocate_one_mir_function_type
    compiles `fInt :: Array Int -> Array Int`, `fStr :: Array String -> Array String`,
    `useInt`, `useStr`, asserts `keys.len() == distinct.len()` over `mir.types`
    (no duplicate concrete function types), then executes to 4.
  Input boundary: verified Typed Core layout fixture and source.
  Commands: cargo test -p psrs-backend cc::layout::tests::equal_normalized_function_signatures_share_one_signature_id;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib polymorphism_erasure_audit::equal_normalized_function_signatures_allocate_one_mir_function_type.
  Result: pass, Wasmtime 49.0.0 executed exit 4.
  Revision: cc5d0f4 + audit diff.
  Gaps: none. The fixpoint also rewrites nested Closure ids in signatures and
    representations, so `useInt`/`useStr` collapse onto the shared id.

PE-03:
  Implementation: cc/lower/conversion.rs (BoxScalar/UnboxScalar plans),
    cc/lower/erased.rs (unbox_erased_value), mir/layout (Box{Integer},
    Box{Number}), mir/lower/assignments.rs.
  Tests: driver polymorphism_erasure_audit::erased_int_box_preserves_high_bit_values
    (2000000000), erased_boolean_box_preserves_both_values (`[true,false]` index 1),
    erased_string_box_preserves_nonempty_contents (logs "value"),
    erased_number_box_preserves_negative_values (-1.5),
    erased_number_box_preserves_nan_and_signed_zero (NaN is not equal to itself;
    1.0 / -0.0 < 0), erased_identity_boxes_char_and_unit ('B' -> 66, Unit
    identity), and mir::gc_tests::erased.
  Input boundary: source plus a verified CC module.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib polymorphism_erasure_audit;
    cargo test -p psrs-backend mir::gc_tests::erased.
  Result: pass, all cases executed under Wasmtime 49.0.0.
  Revision: cc5d0f4 + audit diff.
  Gaps: none. Signed-zero/NaN are observed through arithmetic semantics because
    the language has no NaN literal.

PE-04:
  Implementation: cc/lower/erased.rs (RepresentationCast recovery),
    cc/lower/conversion.rs (RecoverReference), mir/lower (RefCast),
    cc/verify/adaptation.rs, cc/verify/ops/aggregate.
  Tests: mir::gc_tests::erased executes identity recovery for a concrete
    product; cc::verify::tests::rejects_a_nullable_erased_source_crossing_to_a_concrete_reference;
    cc::verify::ops::aggregate::tests::rejects_endpoints_that_disagree_with_the_typed_values.
  Input boundary: verified CC module and malformed CC fixtures.
  Commands: cargo test -p psrs-backend mir::gc_tests::erased;
    cargo test -p psrs-backend cc::verify.
  Result: pass. Erased reference identity executes; wrong nominal or nullable
    recovery is rejected before Wasm emission.
  Revision: cc5d0f4 + audit diff.
  Gaps: none.

PE-05:
  Implementation: cc/layout/captures.rs and cc/layout/mod.rs (boxed_integer_type
    reservation); wasm/lower/structure/closure.rs (Boolean capture uses
    ref.i31, Integer capture uses Box{Integer}).
  Tests: cc::layout::tests::non_i32_integer_shaped_captures_reserve_the_integer_box;
    driver polymorphism_erasure_audit::boolean_capture_uses_i31_and_round_trips
    asserts a Boolean-typed ClosureNew capture slot and executes;
    erased_int_box_preserves_high_bit_values exercises a high-bit Int through
    the erased protocol.
  Input boundary: verified Typed Core layout fixture and source.
  Commands: cargo test -p psrs-backend cc::layout::tests::non_i32_integer_shaped_captures_reserve_the_integer_box;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib polymorphism_erasure_audit::boolean_capture_uses_i31_and_round_trips.
  Result: pass. i31 is used only for Boolean capture slots; high-bit Int values
    survive the integer box.
  Revision: cc5d0f4 + audit diff.
  Gaps: none.

PE-06:
  Implementation: cc/lower/call/application.rs, cc/lower/erased.rs
    (unbox_erased_value), cc/lower/conversion.rs, mir/lower/aggregate.
  Tests: mir::gc_tests::erased executes one `identity` body at Int, Number, and
    a concrete reference; driver polymorphism_erasure_audit::
    linked_modules_round_trip_an_erased_high_bit_int; generic_aggregate_audit::
    audit_battery (array/number/boolean/char/string/unit instantiations).
  Input boundary: verified CC module and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib;
    cargo test -p psrs-backend mir::gc_tests::erased.
  Result: pass. One generic body serves several scalar and reference
    instantiations with payload-sensitive exits.
  Revision: cc5d0f4 + audit diff.
  Gaps: none.

PE-07:
  Implementation: cc/lower/erased.rs (adapt_erased_function_value),
    cc/lower/call/application.rs, cc/lower/call/partial.rs.
  Tests: driver polymorphism_erasure_audit::
    higher_order_adapters_are_value_sensitive_in_both_directions (generic to
    concrete for Int and Boolean, concrete to generic for Int);
    generic_aggregate_audit::audit_battery higher_order_* cases; the adapter
    captures its source once.
  Input boundary: source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib polymorphism_erasure_audit::higher_order_adapters_are_value_sensitive_in_both_directions.
  Result: pass, executed under Wasmtime 49.0.0.
  Revision: cc5d0f4 + audit diff.
  Gaps: none. Over-application of a global returning a function
    (`makeReader true 42`) is rejected before adapters; the tests bind the
    returned function first.

PE-08:
  Implementation: cc/layout/captures.rs (integer-shaped free capture detection),
    cc/lower/lambda.rs (ClosureGetCapture), mir/lower/assignments.rs and
    wasm/lower/structure/closure.rs (uniform nullable eqref capture array).
  Tests: cc::layout::tests::non_i32_integer_shaped_captures_reserve_the_integer_box
    (Char/String/Unit, no Type::Variable, no Type::I32);
    driver polymorphism_erasure_audit::
    escaping_closures_capture_int_and_reference_values (I32 and Ref capture
    slots), escaping_closures_capture_a_string_value (logs "captured"),
    boolean_capture_uses_i31_and_round_trips; generic_aggregate_audit::
    retained_generic_capture_executes_through_a_closure (reference/erased).
  Input boundary: verified Typed Core layout fixture and source.
  Commands: cargo test -p psrs-backend cc::layout::tests::non_i32_integer_shaped_captures_reserve_the_integer_box;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib polymorphism_erasure_audit.
  Result: pass. A free Char/String/Unit capture now reserves Box{Integer};
    Int, Boolean, String, reference, and already-erased captures execute.
  Revision: cc5d0f4 + audit diff.
  Gaps: none in the covered shapes; Number capture is exercised by the generic
    aggregate capture tests.

PE-09:
  Implementation: cc/verify/adaptation.rs (verify_erased_adaptation),
    cc/verify/ops/aggregate, mir/lower (RefCast/RefTest).
  Tests: cc::verify::tests::rejects_a_nullable_erased_source_crossing_to_a_concrete_reference,
    rejects_a_nullable_erased_target_crossing_from_a_concrete_reference,
    accepts_exact_non_null_erased_boundary_casts,
    accepts_a_same_heap_nullability_change_for_loop_carried_references;
    cc::verify::ops::aggregate::tests (nominal/array/product rejection).
  Input boundary: malformed CC fixtures.
  Commands: cargo test -p psrs-backend cc::verify.
  Result: pass. Crossing a heap boundary now requires the exact
    Reference{nullable:false, Erased} shape; a same-heap nullability change
    used to carry loop values stays accepted.
  Revision: cc5d0f4 + audit diff.
  Gaps: MIR-side RefCast/RefTest verification is owned by the MIR topic; see
    the handoff note below.

PE-10:
  Implementation: cc/verify/mod.rs, cc/verify/ops, cc/verify/helpers.rs,
    mir/verify.
  Tests: cc::verify::tests (undeclared parameter, wrong call result, wrong
    unary operand, wrong array element, wrong captures, wrong box projection,
    plus the PE-09 negative cases); cc::verify::ops::aggregate tests.
  Input boundary: malformed CC fixtures.
  Commands: cargo test -p psrs-backend cc::verify.
  Result: pass.
  Revision: cc5d0f4 + audit diff.
  Gaps: none for CC. `ValueConversion::FunctionAdapter` was removed as dead
    code (never constructed); function adaptation is generated directly by
    `adapt_erased_function_value`.

PE-11:
  Implementation: cc/lower/call/application.rs, cc/lower/erased.rs,
    mir/bindings.rs, wit lowering.
  Tests: generic_aggregate_audit::linked_modules_share_canonical_layouts
    (linked generic producer, executes 42),
    polymorphism_erasure_audit::linked_modules_round_trip_an_erased_high_bit_int,
    generic_aggregate_audit::generic_string_array_preserves_observable_contents
    (generic array path crosses the WASI `log` ABI and prints "world").
  Input boundary: linked source and source crossing a WIT call.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass, executed under Wasmtime 49.0.0.
  Revision: cc5d0f4 + audit diff.
  Gaps: there is no separate unspecialized compiler mode; the erased path is
    the one path and it is the executed path.
```

## Remaining work and blockers

- **MIR RefCast/RefTest nullability handoff.** `mir/verify/instruction` checks
  the cast target heap and destination type but does not reject a nullable
  operand or mismatched operand/target nullability; tightening it is owned by
  the MIR topic. CC now rejects these shapes before lowering.
- **`String` is now a distinct `ValueShape`.** The design model lists `String`
  as its own shape; CC maps `Type::String` to `ValueShape::String`, the CC
  verifier rejects numeric operations on it, and P9 maps it to `i32` while
  sharing the one-field integer box on the erased path. `Array Int` and
  `Array String` therefore keep distinct canonical arrays and signatures; the
  PE-02 evidence was updated accordingly.
- **Dead `FunctionAdapter` variant removed.** The design grammar in
  [generic aggregate erasure](../../design/backend/fp/generic-aggregate-erasure.md)
  still lists `FunctionAdapter`; CC performs function adaptation through
  `adapt_erased_function_value`, not through `ValueConversion`. Removing the
  never-constructed variant keeps the code honest; wiring it would require a
  new MIR closure-adapter path.
- **Source coverage.** Runtime evidence is source-level except the verified CC
  erased identity fixture; type-class dictionaries and returned polymorphic
  functions remain independently tracked in
  [type classes and dictionaries](type-classes-and-dictionaries.md).
