# Linear Memory and Canonical ABI Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Linear memory and the canonical ABI boundary](../../design/backend/wasm/linear-memory-and-canonical-abi-boundary.md)
and [Canonical ABI and WIT](../../design/backend/wasm/canonical-abi-and-wit.md),
with [DEC-10](../../decision/DEC-10-canonical-abi-buffer-lifetime.md).

**Progress:** Re-baselined by
[DEC-10](../../decision/DEC-10-canonical-abi-buffer-lifetime.md). Strings are
now GC `(array (mut i16))` values, each distinct literal is a passive data
segment materialized once with `array.new_data` and interned in a lazily
initialized module global, and the ABI adapter transcodes UTF-16 to
and from the component's UTF-8. LM-02, LM-04, LM-05, ABI-01, ABI-06, and ABI-07
are Verified. LM-01 implements that representation and is In progress because
the profile still fixes one wasm32 memory. ABI-02 and ABI-03 are In progress for
GC byte lists and handles. ABI-08 stays Blocked for non-byte `list<T>` /
`SourceType::Array`; `option`/`result`/`variant` and tuples are not compiler
source types ([DEC-11](../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md)).
The reclaiming allocator, transient buffer free, `post-return`, and buffer
ownership moved to
[canonical buffer allocation](canonical-buffer-allocation.md).

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-11 and BE-17..BE-20.

## Scope and dependencies

Complete the one-memory canonical ABI boundary: string/data segments, the static
access-extent verification, and the canonical ABI lowering and WIT registry for
byte lists and scalar shapes. The reclaiming allocator, buffer free, and
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
| ABI-02 | Scalars, enums, flags, chars, GC strings, and `own`/`borrow` handles map to canonical values and drop rules. | WIT scalar/enum/flags tests, string tests, and handle drop tests. | In progress |
| ABI-03 | Byte lists and direct (including nested) records flatten in WIT field order and recover into GC values. | WIT record/flags flattening tests, the indirect composite fixture, and GC byte-list recovery. | In progress |
| ABI-06 | Narrowed and unsigned WIT integers (`s8`/`u8`/`s16`/`u16`/`u32`) map to source `Int` with canonical masking and sign-extension. | Classification, validation, and lowering tests. | Verified |
| ABI-07 | The componentizer lifts the core module and prunes unused imports. | Component emission and execution tests. | Verified |
| ABI-08 | General aggregate results, `option`/`result`/`variant` payloads, non-byte lists, tuples, and export `post-return` release lower or are rejected with named diagnostics. | Partially covered: unsupported shapes are rejected with source diagnostics; the listed shapes cannot be produced. | Blocked |

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names, input boundary,
commands, runtime, executed/skipped cases, revision, and gaps. Runtime cases use
`PSRS_REQUIRE_WASMTIME=1`. After Rust edits run `cargo fmt --all --check`,
`cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`.

## Recorded evidence

Revision: the `backend/gc-string` worktree on top of `cc8cf2c`.
Runtime: `wasmtime 49.0.0` under `PSRS_REQUIRE_WASMTIME=1`.

```text
LM-01:
  Implementation: crates/psrs-backend/src/wasm/lower/mod.rs and encode.rs
    (one memory, memory/data index order).
  Tests: psrs-driver tests::integration::emits_a_wasi_command_component;
    tests::wasi::runs_main_as_a_wasi_component_when_wasmtime_is_available;
    wasm::tests::rejects_a_data_index_that_does_not_match_module_order.
  Input boundary: verified MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass.
  Gaps: none.
```

```text
LM-02:
  Implementation: the `$string` GC array in crates/psrs-backend/src/mir/layout/;
    `ArrayNewData` in mir/instruction.rs with the pool in mir/literals.rs;
    passive UTF-16 segments and the one lazy interning global per used literal in
    wasm/lower/runtime.rs; the `ref.is_null`/`global.set` guarded
    `array.new_data` in wasm/lower/structure/instructions.rs; the UTF-16<->UTF-8
    codec in wasm/lower/codec/; the adapter in mir/wit/.
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
    builds one GC string behind one global, and the ABI transcodes UTF-16 to
    UTF-8 (invalid sequences and unpaired surrogates become U+FFFD).
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
    block-parameter joins, unknown addresses, allocator-proven dynamic store
    accepted and unproven dynamic store rejected).
  Input boundary: verified MIR with memory operations.
  Commands: cargo test -p psrs-backend wasm::lower::extent.
  Result: pass. GC string literals are not MIR-addressable and define no
    region, matching the design; a dynamic store through a `cabi_realloc`
    pointer is accepted and an unproven dynamic store is rejected.
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
  Tests: mir::wit::tests::scalar::*; mir::wit::tests::flags::*;
    psrs-driver tests::wasi::{lowers_a_boolean_wit_result_with_a_boolean_source_type,
    lowers_a_source_foreign_import_with_a_wit_binding}.
  Input boundary: WIT signatures and source.
  Commands: cargo test -p psrs-backend mir::wit;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi.
  Result: pass.
  Gaps: none.
```

```text
ABI-03:
  Implementation: crates/psrs-backend/src/mir/wit/records.rs and
    abi/classification.rs.
  Tests: mir::wit::tests::records::{record_arguments_flatten_in_wit_field_order,
    nested_records_flatten_byte_lists_in_wit_field_order};
    mir::indirect_tests::composite::
    indirect_composite_parameters_lower_to_the_canonical_layout.
  Input boundary: WIT records and verified MIR.
  Commands: cargo test -p psrs-backend mir::.
  Result: pass.
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
    crates/psrs-backend/src/abi/validation.rs (`source_parameter_matches`),
    crates/psrs-backend/src/abi/mod.rs (`validate_signature`,
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
  Implementation: crates/psrs-backend/src/component.rs (wit-component lift).
  Tests: psrs-driver tests::integration::emits_a_wasi_command_component;
    tests::wasi::runs_main_as_a_wasi_component_when_wasmtime_is_available.
  Input boundary: encoded core module.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass.
  Gaps: none.
```

```text
ABI-08:
  Implementation: none for the listed shapes; unsupported shapes are rejected
    at classification/lowering.
  Tests: psrs-driver tests::wasi::
    rejects_a_non_byte_wit_list_before_lowering_it_as_a_string,
    rejects_a_wit_import_when_the_declared_source_type_does_not_match.
  Input boundary: WIT signatures and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi.
  Result: blocked; the unsupported-shape diagnostics pass, but the shapes are
    not lowered.
  Gaps: non-byte `list<T>` still needs `SourceType::Array` and its lowering
    (copy in, read back) for elements that map. That list part of ABI-08 is
    unchanged. `option`, `result`, non-unit `variant`, and tuple are not
    compiler source types
    ([DEC-11](../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md),
    [primitive FFI and the standard library](../../design/backend/wasm/primitive-ffi-and-stdlib.md)):
    a wrapper may pass primitive arguments whose flattening matches, and
    multi-value returns stay unsupported. Do not define `Maybe`/`Either`/tuple
    source types. This keeps BE-19 `Partial`.
```

## Remaining work and blockers

Migration to the DEC-10 target: GC string representation and `array.new_data`
literals (LM-02), the re-scoped extent regions (LM-04), GC byte-list recovery
(ABI-02/ABI-03), and `own`/`borrow` handles (ABI-02). The reclaiming allocator,
buffer free, and `post-return` are tracked by
[canonical buffer allocation](canonical-buffer-allocation.md). ABI-08's
`option`/`result`/`variant` and tuple forms are not compiler source types
([DEC-11](../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md),
[primitive FFI and the standard library](../../design/backend/wasm/primitive-ffi-and-stdlib.md)):
a wrapper may pass primitive arguments whose flattening matches, and
multi-value returns stay unsupported. Non-byte `list<T>` / `SourceType::Array`
is unchanged and is still the list part of ABI-08. These are tracked on BE-11
and BE-17..BE-20 in [D-04](../../design/D-04-suite-roadmap.md).
