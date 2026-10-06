# Linear Memory and Canonical ABI Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Linear memory and the canonical ABI boundary](../../design/backend/wasm/linear-memory-and-canonical-abi-boundary.md)
and [Canonical ABI and WIT](../../design/backend/wasm/canonical-abi-and-wit.md),
with [DEC-10](../../decision/DEC-10-canonical-abi-buffer-lifetime.md).

**Progress:** Re-baselined by
[DEC-16](../../decision/DEC-16-scalar-strings-and-utf8-storage.md). The landed
strings are GC `(array (mut i8))` values holding canonical UTF-8, each distinct
literal is a passive data segment materialized once with `array.new_data` and
interned in a lazily initialized module global, and the ABI adapter validates
text strictly instead of transcoding: a malformed sequence or an unpaired
surrogate traps. LM-02, LM-04, LM-05, ABI-01, ABI-03, ABI-06, and
ABI-07 are Verified. LM-01 is In progress because the profile still fixes one
wasm32 memory. ABI-02 is Verified, including `own`/`borrow` handle drop;
scalar, enum, flags, char, and GC string mapping already has tests. ABI-08's
non-byte `list<T>` / `Array` is lowered for scalars, `bool`, `char`,
and `string`/`list<u8>` elements, with an execution test for `list<string>`;
`option`/`result`/`variant` and tuples remain out because they are not compiler
source types
([DEC-11](../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md)).
The reclaiming allocator, transient buffer free, `post-return`, and buffer
ownership moved to
[canonical buffer allocation](canonical-buffer-allocation.md).
The ABI now runs on a single normalized canonical type: `abi::canonical`
resolves each WIT value once and derives flattening, layout, conformance, and
lowering from it; the descriptor types and per-shape plans are gone
([compositional canonical ABI lowering](../../design/backend/wasm/canonical-abi-compositional.md)).

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-11 and BE-17..BE-20.

## Scope and dependencies

Complete the one-memory canonical ABI boundary: string/data segments, the static
access-extent verification, and the canonical ABI lowering and WIT registry for
byte lists, non-byte lists of supported elements, and scalar shapes. The
reclaiming allocator, buffer free, and
`post-return` are owned by
[canonical buffer allocation](canonical-buffer-allocation.md). The thin
encoder/validator is owned by [Wasm encoding](wasm-encoding.md); WASI service
wiring by the [WASI platform](wasi-platform.md). This topic owns the bytes and
WIT adaptation.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| LM-01 | The target profile selects the pointer width and the ABI memory; the stable profile uses one wasm32 memory at index 0 with deterministic memory/data index order. | Encoded modules validate; driver components run; memory64/multi-memory remain. | In progress |
| LM-02 | Strings are GC byte sequences; each distinct literal is a passive data segment materialized once with `array.new_data` and interned in a lazily initialized module global, and the ABI linearizes strings transiently. | GC-string construction, crossing, literal interning, and execution tests. | Verified |
| LM-04 | Static access-extent verification covers the scratch and heap-state regions and requires `cabi_realloc` provenance for dynamic stores. | `wasm::lower::extent` accept/reject fixtures under the new regions. | Verified |
| LM-05 | Byte and width operations lower for the canonical ABI boundary. | `f64`/`f32`/`i64` adaptation tests and WIT scalar cases. | Verified |
| ABI-01 | WIT is vendored, parsed, name-resolved, and validated against source signatures. | Registry/validation tests and the signature-mismatch rejection. | Verified |
| ABI-02 | Scalars, enums, flags, chars, GC strings, and `own`/`borrow` handles map to canonical values and drop rules. | WIT scalar/enum/flags tests, string tests, and handle drop tests. | Verified |
| ABI-03 | `list<u8>` and direct (including nested) records flatten in WIT field order and recover into GC values: a `list<u8>` recovers into `Array Int`, not into text ([DEC-16](../../decision/DEC-16-scalar-strings-and-utf8-storage.md)). | WIT record/flags flattening tests, the indirect composite fixture, GC `list<u8>` recovery, and a driver execution test passing a recovered byte array to another import. | Verified |
| ABI-06 | Narrowed and unsigned WIT integers (`s8`/`u8`/`s16`/`u16`/`u32`) map to source `Int` with canonical masking and sign-extension. | Classification, validation, and lowering tests. | Verified |
| ABI-07 | The componentizer lifts the core module and prunes unused imports. | Component emission and execution tests. | Verified |
| ABI-08 | General aggregate results, `option`/`result`/`variant` payloads, non-byte lists, tuples, and export `post-return` release lower or are rejected with named diagnostics. | Non-byte `list<T>` of scalars, `bool`, `char`, `string`/`list<u8>`, nullary enums, flags, resource handles as parameters, and directly flattened records of scalar or string fields is classified, validated, and lowered; `list<string>` has a driver execution test and the record, flags, and handle elements have synthesized Wasm fixtures. `option`/`result`/`variant` are classified and validated against `Data.Maybe.Maybe`, `Data.Either.Either`, and a source data type, CC derives their variant representation and a concrete payload tree, and MIR branches on each tag and rebuilds the source value recursively for a scalar payload of any width (`s8`..`u64`, `f32`/`f64`), a byte or non-byte list, `flags`, a closed record, and a nested `option`/`result`/`variant`, recursing through record fields and a `list<record>`/`list<flags>` element ([DEC-13](../../decision/DEC-13-wit-to-source-type-mapping.md)); synthesized Wasm fixtures cover a `result`, `option`, `variant`, `option` parameter, a nested-variant error payload, nested record payloads, 64-bit/float payloads, a flags payload, a non-byte-list payload, a record with an aggregate field (both directions), `list<record>` payloads, and a large record result whose return area is allocated through `cabi_realloc`. An indirect parameter record carries a mapped aggregate as its discriminant and joined payload. Under [DEC-14](../../decision/DEC-14-resource-handle-ownership.md) the compiler no longer drops or releases a handle: a source-declared `[resource-drop]<resource>` import lowers to the canonical drop, a handle nested in a result aggregate is an ordinary value, and a `borrow<T>` result is rejected. `list<option<T>>`, `list<result<O, E>>`, and `list<variant>` elements, nested `list<list<T>>`, multi-word flags as list elements and inside aggregates, non-byte `list<T, N>`, and `list<own<T>>` results are now classified and lowered (recursively from the canonical element and its guest layout), and a unit-success `result<_, E>` maps to `Either E Unit` (the error on `Left`), sizes its return area from the error payload, and decodes the error into `Left`; the aggregate, nested, fixed-length, and owned-handle-list shapes have synthesized lower/encode/validate fixtures and the source-reachable aggregate list runs under Wasmtime. | In progress |

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names, input boundary,
commands, runtime, executed/skipped cases, revision, and gaps. Runtime cases use
`PSRS_REQUIRE_WASMTIME=1`. After Rust edits run `cargo fmt --all --check`,
`cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`.

## Recorded evidence

Revision: `81b2eee` plus the DEC-10 re-verification in this worktree.
Runtime: `wasmtime 49.0.1` under `PSRS_REQUIRE_WASMTIME=1`.

```text
LM-01:
  Implementation: crates/psrs-backend/src/wasm/lower/mod.rs and encode.rs
    (one memory, memory/data index order).
  Tests: psrs-driver tests::integration::emits_a_wasi_command_component;
    tests::wasi::runs_main_as_a_wasi_component_when_wasmtime_is_available;
    wasm::tests::rejects_a_data_index_that_does_not_match_module_order.
  Input boundary: verified MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass for one wasm32 memory at index 0. Encoded modules validate and
    driver components run.
  Gaps: the stable profile still fixes one wasm32 memory. Memory64 and
    multi-memory are not implemented, so this row stays In progress.
```

```text
LM-02:
  Implementation: the `$string` GC array in crates/psrs-backend/src/mir/layout/;
    `ArrayNewData` in mir/instruction.rs with the pool in mir/literals.rs;
    passive UTF-8 segments and the one lazy interning global per used literal in
    wasm/lower/runtime.rs; the `ref.is_null`/`global.set` guarded
    `array.new_data` in wasm/lower/structure/instructions.rs; the strict
    validation and byte codec in wasm/lower/codec/; the adapter in mir/wit/.
  Tests: psrs-driver tests::wasi::{lowers_string_log_to_wasi_stdout,
    prints_hello_world_when_wasmtime_is_available,
    prints_an_interned_literal_once_per_use_when_wasmtime_is_available,
    prints_an_empty_literal_when_wasmtime_is_available,
    passes_a_returned_wit_string_to_another_import,
    keeps_multiple_returned_wit_strings_in_distinct_allocations};
    wasm::tests::interns_repeated_string_literals_in_one_lazy_global;
    mir::layout::tests::maps_every_cc_value_shape_to_its_specified_mir_type;
    mir::indirect_tests::composite::
    indirect_composite_parameters_lower_to_the_canonical_layout.
  Input boundary: verified MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass; a static literal never enters linear memory, a repeated literal
    builds one GC string behind one global, and the ABI validates UTF-8 strictly:
    an invalid sequence or an unpaired surrogate traps instead of producing
    U+FFFD.
  Gaps: none.
```

```text
LM-03:
  Moved to canonical buffer allocation (ALC-01..ALC-03).
```

```text
LM-04:
  Implementation: crates/psrs-backend/src/wasm/lower/extent/.
  Tests: wasm::lower::extent::tests::* (scratch/heap-state read/write and
    boundary, wasm32 fixed/effective extents, wrapping arithmetic,
    block-parameter joins, unknown addresses, and unproven dynamic stores
    rejected by allows_dynamic_reads_but_rejects_dynamic_stores_without_proof).
  Input boundary: verified MIR with memory operations.
  Commands: cargo test -p psrs-backend wasm::lower::extent.
  Result: pass. GC string literals are not MIR-addressable and define no
    region, matching the design, and an unproven dynamic store is rejected.
  Gaps: only the fixed `offset + width <= size` bound is proven for an
    allocator pointer.
```

```text
LM-05:
  Implementation: crates/psrs-backend/src/mir/instruction.rs
    (Load/Load8U/Store*/WrapI64/WidenI64) and mir/wit/.
  Tests: mir::tests::lowers_and_validates_f64_to_f32_abi_conversions;
    mir::wit::tests::scalar::*.
  Input boundary: verified MIR and WIT signatures.
  Commands: cargo test -p psrs-backend mir::.
  Result: pass.
  Gaps: none.
```

```text
ABI-01:
  Implementation: crates/psrs-backend/src/abi/ (registry, validation) and
    crates/psrs-backend/src/mir/wit/.
  Tests: psrs-backend bindings tests; psrs-driver
    tests::wasi::rejects_a_wit_import_when_the_declared_source_type_does_not_match.
  Input boundary: vendored WIT and source signatures.
  Commands: cargo test -p psrs-backend; PSRS_REQUIRE_WASMTIME=1
    cargo test -p psrs-driver --lib tests::wasi.
  Result: pass.
  Gaps: none.
```

```text
ABI-02:
  Implementation: crates/psrs-backend/src/abi/classification.rs and mir/wit/.
    `WasiParamKind::Handle` classifies `own<T>` and `borrow<T>` as one canonical
    `i32`. An owned import result is dropped with `[resource-drop]<T>` when the
    function does not return it; a borrow result is released when the call
    returns; an owned export result is released in `cabi_post_<name>`.
  Tests: mir::wit::tests::scalar::{scalar_f64_results_are_called_directly,
    char_arguments_and_results_use_direct_i32_values,
    enum_arguments_and_results_keep_the_validated_i32_tags,
    f32_arguments_and_results_are_adapted_to_source_numbers};
    mir::wit::tests::flags::{flags_arguments_pack_boolean_fields_in_wit_declaration_order,
    flags_arguments_split_after_thirty_two_bits};
    psrs-driver tests::wasi::{lowers_a_boolean_wit_result_with_a_boolean_source_type,
    lowers_a_source_foreign_import_with_a_wit_binding,
    lowers_string_log_to_wasi_stdout,
    prints_hello_world_when_wasmtime_is_available,
    prints_a_non_ascii_literal_when_wasmtime_is_available};
    mir::wit::tests::handles::{a_borrow_result_is_released_when_the_call_returns,
    the_verifier_rejects_an_owned_handle_dropped_twice,
    the_verifier_rejects_a_handle_used_after_its_borrow_scope};
    mir::binding_tests::p9_drops_an_owned_handle_that_the_function_does_not_return;
    wasm::lower::post_return::tests::post_return_drops_an_owned_export_handle.
  Input boundary: WIT signatures and source.
  Commands: cargo test -p psrs-backend --lib mir::wit::tests::scalar
    mir::wit::tests::flags; PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver
    --lib tests::wasi::lowers_string_log_to_wasi_stdout
    prints_hello_world_when_wasmtime_is_available
    prints_a_non_ascii_literal_when_wasmtime_is_available
    lowers_a_boolean_wit_result_with_a_boolean_source_type
    lowers_a_source_foreign_import_with_a_wit_binding.
  Result: pass for scalars, enums, flags, chars, GC strings, and handle
    drop. A string literal is a GC `(array (mut i8))` of canonical UTF-8
    linearized at the call. `resource.drop` is inserted for an owned handle the function does
    not return, a borrow result is released at the call, and the verifier
    rejects a second drop and a use after the borrow scope.
  Gaps: an owned handle returned as `Int` from a non-export function is not
    tracked in the caller. Handles nested in an unsupported aggregate are not
    dropped. The row is Verified for the scalar and handle mapping that the
    source ABI lowers.
```

```text
ABI-03:
  Implementation: crates/psrs-backend/src/mir/wit/records.rs,
    mir/wit/mod.rs (a list result calls `bytes_to_string`), and
    abi/classification.rs.
  Tests: mir::wit::tests::records::{record_arguments_flatten_in_wit_field_order,
    nested_records_flatten_byte_lists_in_wit_field_order};
    mir::indirect_tests::composite::
    indirect_composite_parameters_lower_to_the_canonical_layout;
    mir::wit::tests::buffers::list_results_free_the_import_buffer_after_decoding;
    psrs-driver tests::wasi::passes_a_byte_array_to_another_import.
  Input boundary: WIT records, verified MIR, and executed components.
  Commands: cargo test -p psrs-backend --lib
    record_arguments_flatten_in_wit_field_order
    nested_records_flatten_byte_lists_in_wit_field_order
    indirect_composite_parameters_lower_to_the_canonical_layout
    list_results_free_the_import_buffer_after_decoding;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
    passes_a_byte_array_to_another_import.
  Result: pass. Direct and nested records flatten in WIT field order. A
    returned WIT `string` decodes into a GC string and a returned `list<u8>`
    into `Array Int`, each of which another import can consume, because the two
    WIT types no longer share a source type under
    [DEC-16](../../decision/DEC-16-scalar-strings-and-utf8-storage.md).
    Re-executed under Wasmtime 49.0.1.
  Gaps: source-level record signatures are rejected by the type checker, so
    record parameters are reachable through backend IR paths only.
```

```text
ABI-04:
  Moved to canonical buffer allocation (ALC-04); the indirect parameter layout
  remains covered by ABI-03 and the canonical ABI/WIT topic.
```

```text
ABI-05:
  Moved to canonical buffer allocation (ALC-05).
```

```text
ABI-06:
  Implementation: crates/psrs-backend/src/abi/classification.rs
    (`param_kind`, `result_kind` for U8/U16/U32/S8/S16),
    crates/psrs-backend/src/abi/link.rs (`validate_import_signature`,
    `WasiParamKind::IntegerNarrow`, `WasiResultKind::IntegerNarrow`), and
    crates/psrs-backend/src/mir/wit/parameters/mod.rs (`narrow_integer` masks
    and sign-extends).
  Tests: abi::tests::integers::classifies_narrow_and_unsigned_wit_integers;
    mir::wit::parameters::tests::unsigned_narrow_parameter_only_masks,
    ::signed_narrow_parameter_masks_and_sign_extends.
  Input boundary: WIT classification, signatures, and MIR lowering.
  Commands: cargo test -p psrs-backend abi::tests::classifies_narrow;
    cargo test -p psrs-backend mir::wit::parameters.
  Result: pass; every WIT integer maps to source `Int`, narrow parameters are
    masked (and sign-extended when signed), and narrow results use the
    canonical `i32` directly.
  Gaps: none.
```

```text
ABI-07:
  Implementation: crates/psrs-linker/src/compose.rs (wit-component lift).
  Tests: psrs-driver tests::integration::emits_a_wasi_command_component;
    tests::wasi::runs_main_as_a_wasi_component_when_wasmtime_is_available.
  Input boundary: encoded core module.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass.
  Gaps: none.
```

```text
ABI-08:
  Implementation: canonical WIT classification and flattening in
    crates/psrs-backend/src/abi/canonical/ (CanonicalType, resolve, flatten,
    leaves, size_align); memory layout in crates/psrs-backend/src/abi/layout.rs;
    MIR lowering in crates/psrs-backend/src/mir/wit/ (parameters, aggregate,
    lists, free); Wasm loops in crates/psrs-backend/src/wasm/lower/structure/lists/
    (element.rs, aggregate.rs, nested.rs, record.rs). A non-byte list element is
    copied recursively from its canonical type and guest layout: scalars,
    `string`/`list<u8>`, records, flags (including multi-word), `option`/`result`/
    `variant`, nested lists, and owned-handle lists as parameters and results. A
    shape the source cannot express is rejected at binding time; the guest half
    is CC's RepresentationTable through guest_layout.
  Tests: psrs-backend abi::tests::lists::
    interns_an_array_of_supported_elements_and_rejects_nested_arrays,
    classifies_scalar_and_string_lists_and_rejects_aggregates,
    admits_a_non_byte_fixed_length_list;
    abi::canonical::tests::result_area_sizes_a_unit_success_from_its_error_payload;
    mir::wit::tests::lists::
    a_borrow_list_result_is_rejected_with_a_named_diagnostic;
    mir::indirect_tests::record_list::tests::* (aggregate, nested, fixed, and
    multi-word-flags elements, lower/encode/validate);
    psrs-driver tests::wasi::{
    reads_environment_variables_as_records_when_wasmtime_is_available,
    lowers_a_list_of_strings_to_an_array,
    lowers_the_environment_arguments_wrapper_to_an_array,
    reads_environment_arguments_when_wasmtime_is_available,
    rejects_a_non_byte_wit_list_before_lowering_it_as_a_string,
    rejects_a_string_declaration_for_a_list_of_strings,
    rejects_a_wit_import_when_the_declared_source_type_does_not_match};
    psrs-driver tests::wasi::filesystem::
    reports_a_unit_success_result_as_left_when_wasmtime_is_available
    (a unit-success `result<_, E>` failure observes `Right`, no trap).
  Input boundary: WIT signatures, source, and executed component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi;
    cargo test -p psrs-backend --lib abi::tests::lists;
    cargo test -p psrs-backend --lib mir::wit::tests::lists;
    cargo test -p psrs-backend --lib mir::indirect_tests::record_list.
  Result: pass under Wasmtime 49.0.1. `list<string>` and an aggregate list
    result are copied into a GC array; `option`/`result`/`variant`, nested lists,
    multi-word flags, and non-byte fixed-length lists are classified and lowered;
    a unit-success `result<_, E>` maps to `Either E Unit` (the error on `Left`), sizes its return area from the error payload, and decodes the error into `Left`;
    a `list<own<T>>` result is an ordinary index array and a `list<borrow<T>>`
    result is rejected by name.
  Gaps: `option`, `result`, and non-unit `variant` are classified and validated
    against `Data.Maybe.Maybe`, `Data.Either.Either`, and a source data type,
    CC derives their variant representation and a concrete payload tree, and MIR
    branches on each tag and rebuilds the source value recursively for a scalar
    payload of any width (`s8`..`u64`, `f32`/`f64`), a byte or non-byte list,
    `flags`, a closed record, and a nested `option`/`result`/`variant`, recursing
    through record fields and a `list<record>`/`list<flags>` element
    ([DEC-13](../../decision/DEC-13-wit-to-source-type-mapping.md)). An indirect
    parameter record carries a mapped aggregate as its discriminant and joined
    payload, and a large aggregate return area is allocated through
    `cabi_realloc`. Under [DEC-14](../../decision/DEC-14-resource-handle-ownership.md)
    a handle in a result is an ordinary value, the compiler exposes
    `resource.drop` to source, and a borrow result is rejected. Aggregate,
    nested, fixed-length, and `list<own<T>>` shapes are not source-reachable, so
    they have synthesized lower/encode/validate evidence rather than a Wasmtime
    run; the source-reachable aggregate list (`list<tuple<string, string>>`) runs
    under Wasmtime. This keeps BE-19 `Partial`.
```

## Remaining work and blockers

LM-02, LM-04, ABI-02, and ABI-03 are verified on the GC-string representation
and the `own`/`borrow` drop rules. LM-01 stays In progress because the profile
fixes one wasm32 memory. An owned handle returned as `Int` from a non-export
function is not tracked in the caller. The reclaiming allocator,
buffer free, and `post-return` are tracked by
[canonical buffer allocation](canonical-buffer-allocation.md). ABI-08's
`option`/`result`/`variant` forms are recognized on the resolved-type path
([DEC-13](../../decision/DEC-13-wit-to-source-type-mapping.md)): classification,
Core conformance validation, CC's variant representation and payload tree, and
MIR recursive tag-branch lowering for a scalar payload of any width, a byte or
non-byte list, `flags`, a closed record, and a nested `option`/`result`/`variant`
are implemented, recursing through record fields and a `list<record>`/
`list<flags>` element; a large aggregate return area is allocated through
`cabi_realloc`. Under [DEC-14](../../decision/DEC-14-resource-handle-ownership.md)
the compiler drops no handle on its own and exposes `resource.drop` to source.
The non-byte `list<T>` /
`Array` part of ABI-08 is lowered for supported elements and now also for a
`list<record>`/`list<flags>` aggregate payload. These are tracked on BE-11
and BE-17..BE-20 in [D-04](../../design/D-04-suite-roadmap.md).
