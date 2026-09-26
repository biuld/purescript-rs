# Linear Memory and Canonical ABI Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Linear memory and the canonical ABI boundary](../../design/backend/wasm/linear-memory-and-canonical-abi-boundary.md)
and [Canonical ABI and WIT](../../design/backend/wasm/canonical-abi-and-wit.md).

**Progress:** LM-01 through LM-05 and ABI-01 through ABI-07 Verified. ABI-08 is
Blocked: general aggregate results, `option`/`result`/`variant` payloads,
non-byte lists, tuples, and `own`/`borrow` have no source type or frontend
support yet. This is why the broader BE-19 row stays `Partial`.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-11 and BE-17..BE-20.

## Scope and dependencies

Complete the one-memory canonical ABI boundary, string/data segments, the bump
allocator, static access-extent verification, and the canonical ABI lowering and
WIT registry. The thin encoder/validator is owned by
[Wasm encoding](wasm-encoding.md); WASI service wiring by the
[WASI platform](wasi-platform.md). This topic owns the bytes, the allocator, and
WIT adaptation.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| LM-01 | One wasm32 memory at index 0, with deterministic memory/data index order and profile gating. | Encoded modules validate; driver components run. | Verified |
| LM-02 | Strings are length-prefixed UTF-8 data segments; literal regions are read-only. | String execution cases plus literal read-only extent rejection. | Verified |
| LM-03 | `cabi_realloc` checks alignment, address overflow, old range, growth, and copies preserved bytes. | Allocator unit test covering each failure and the copy path. | Verified |
| LM-04 | Static access-extent verification propagates known addresses and rejects unsound stores. | `wasm::lower::extent` accept/reject fixtures. | Verified |
| LM-05 | Byte and width operations lower for the canonical ABI boundary. | `f64`/`f32`/`i64` adaptation tests and WIT scalar cases. | Verified |
| ABI-01 | WIT is vendored, parsed, name-resolved, and validated against source signatures. | Registry/validation tests and the signature-mismatch rejection. | Verified |
| ABI-02 | Scalars, enums, flags, chars, and handles map directly to canonical values. | WIT scalar/enum/flags tests and driver WASI cases. | Verified |
| ABI-03 | Byte lists and direct (including nested) records flatten in WIT field order. | WIT record/flags flattening tests and the indirect composite fixture. | Verified |
| ABI-04 | Indirect parameter tuples are laid out and allocated through `cabi_realloc`. | Indirect composite parameter lowering/artifact tests. | Verified |
| ABI-05 | Unit-success, scalar, and byte-list results are recovered at the boundary. | Driver string/list result cases and multiple returned strings. | Verified |
| ABI-06 | Narrowed and unsigned WIT integers (`s8`/`u8`/`s16`/`u16`/`u32`) map to source `Int` with canonical masking and sign-extension. | Classification, validation, and lowering tests. | Verified |
| ABI-07 | The componentizer lifts the core module and prunes unused imports. | Component emission and execution tests. | Verified |
| ABI-08 | General aggregate results, `option`/`result`/`variant` payloads, non-byte lists, tuples, and `own`/`borrow` drop rules lower or are rejected with named diagnostics. | Partially covered: unsupported shapes are rejected with source diagnostics; the listed shapes cannot be produced. | Blocked |

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names, input boundary,
commands, runtime, executed/skipped cases, revision, and gaps. Runtime cases use
`PSRS_REQUIRE_WASMTIME=1`. After Rust edits run `cargo fmt --all --check`,
`cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`.

## Recorded evidence

Revision: `c4e65dd` plus the memory/ABI evidence changes in this worktree.
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
  Implementation: string data segments in crates/psrs-backend/src/wasm/lower/;
    literal read-only extents in wasm/lower/extent/.
  Tests: psrs-driver tests::wasi::{lowers_string_log_to_wasi_stdout,
    prints_hello_world_when_wasmtime_is_available};
    wasm::lower::extent::tests::{permits_loads_within_a_string_literal_segment,
    rejects_loads_that_cross_a_string_literal_segment,
    rejects_stores_to_string_literals_even_when_in_bounds}.
  Input boundary: verified MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass.
  Gaps: none.
```

```text
LM-03:
  Implementation: crates/psrs-backend/src/wasm/lower/realloc/.
  Tests: wasm::lower::realloc::tests::
    realloc_checks_alignment_overflow_and_growth_and_copies_old_bytes.
  Input boundary: synthesized core module.
  Commands: cargo test -p psrs-backend wasm::lower::realloc.
  Result: pass.
  Gaps: none.
```

```text
LM-04:
  Implementation: crates/psrs-backend/src/wasm/lower/extent/.
  Tests: wasm::lower::extent::tests::* (14 cases: widths, scratch
    read/write, literal bounds and read-only, wasm32 fixed/effective extents,
    wrapping arithmetic, block-parameter joins, unknown addresses,
    dynamic-read-allowed/dynamic-store-rejected).
  Input boundary: verified MIR with memory operations.
  Commands: cargo test -p psrs-backend wasm::lower::extent.
  Result: pass.
  Gaps: allocation provenance for dynamic pointers remains outside the
    verifier, recorded by design; dynamic reads rely on the Wasm bounds trap.
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
  Implementation: crates/psrs-backend/src/mir/wit/ (indirect parameter tuple
    layout and cabi_realloc allocation).
  Tests: mir::indirect_tests::composite::
    indirect_composite_parameters_lower_to_the_canonical_layout,
    mir::indirect_tests::indirect_canonical_parameters_lower_to_an_artifact.
  Input boundary: verified MIR and WIT signatures.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend mir::indirect_tests.
  Result: pass.
  Gaps: none.
```

```text
ABI-05:
  Implementation: crates/psrs-backend/src/mir/wit/ result recovery.
  Tests: psrs-driver tests::wasi::
    lowers_a_list_returning_import_with_an_allocator,
    keeps_multiple_returned_wit_strings_in_distinct_allocations,
    passes_a_returned_wit_string_to_another_import.
  Input boundary: source and executed components.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi.
  Result: pass.
  Gaps: indirect aggregate results remain part of ABI-06.
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
  Gaps: the listed shapes have no source type. Resumption: define source
    `Array` and `Maybe`/`Either`/tuple types (or equivalent canonical
    encodings), make the type checker accept record, array, and aggregate
    foreign signatures, and add result memory-layout computation and
    read-back. This keeps BE-19 `Partial`.
```

## Remaining work and blockers

ABI-08 is Blocked on source types and frontend support for aggregate foreign
signatures; its resumption condition is recorded above. Allocation provenance
and reclamation stay out of scope by DEC-09 and are recorded in the design.
