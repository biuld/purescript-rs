# Wasm Encoding, Validation, and Capability Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Wasm encoding and structuring](../../design/backend/wasm/encoding-and-structuring.md)
and the [capability profile](../../design/backend/wasm/capability-profile.md).

**Progress:** ENC-01 through ENC-11 Verified. The broader `BE-13`/`BE-14`/`BE-16`
rows retain their official-suite gates and the coverage gaps recorded in the
capability-profile implementation notes.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-13, BE-14, and BE-16; supporting BE-15.

## Scope and dependencies

Complete the thin structured Wasm representation, mechanical index allocation,
module skeleton, the thin-IR verifier, and the target capability profile. The
structuring algorithm and tail-call opcodes are owned by
[control flow and tail calls](../fp/control-flow-and-tail-calls.md); the
canonical ABI bytes are owned by
[linear memory and canonical ABI](linear-memory-and-canonical-abi.md). This
topic owns the encoding that consumes verified MIR and the validator/profile
contract.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| ENC-01 | The thin IR keeps distinct index domains and `Leaf`/`If`/`Block`/`Loop` regions; P10 makes no representation decision. | Encoding tests exercise structured regions and index domains; MIR values lower to locals. | Verified |
| ENC-02 | Defined types precede function types; import/function/entry/realloc, memory/global/data, and export indices follow the documented order. | Index-order and wrong-domain rejection fixtures. | Verified |
| ENC-03 | Every `ref.func` is declared in the element segment. | Closure execution tests emit `ref.func` and run the component. | Verified |
| ENC-04 | Data segments and the `cabi_realloc` allocator follow the boundary contract. | Data-index order rejection and allocator unit test. | Verified |
| ENC-05 | The command entry is synthesized and calls `exit-with-code` with the result on the CLI profile. | Entry synthesis is exercised by every executed component; a missing entry symbol is rejected. | Verified |
| ENC-06 | The thin-IR verifier checks local, function, type, global, data, and export indices, global initializer types, and branch depths. | Dedicated `wasm::tests` accept/reject fixtures. | Verified |
| ENC-07 | The encoded module is validated with a validator built from the same capability profile, then printed to WAT. | Backend harness validates before execution; profile/validator tests. | Verified |
| ENC-08 | Each capability field controls exactly its wasmparser feature and no other. | Independent-flag derivation test. | Verified |
| ENC-09 | Operations requiring a disabled capability are rejected before encoding with a source-associated diagnostic. | MIR capability fixtures and the driver WASI-capability diagnostic. | Verified |
| ENC-10 | Every backend execution case validates the artifact and asserts value-sensitive behavior. | `mir::gc_tests` and driver suites run validator plus Wasmtime. | Verified |
| ENC-11 | `return_call`/`return_call_ref` are encoded when the profile enables tail calls. | Driver tail-call tests assert the opcodes in WAT and execute. | Verified |

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names, input boundary,
commands, runtime, executed/skipped cases, revision, and gaps. Runtime cases use
`PSRS_REQUIRE_WASMTIME=1`. After Rust edits run `cargo fmt --all --check`,
`cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`.

## Recorded evidence

Revision: `3795492` plus the encoding evidence changes in this worktree.
Runtime: `wasmtime 49.0.0` under `PSRS_REQUIRE_WASMTIME=1`.

```text
ENC-01:
  Implementation: crates/psrs-backend/src/wasm/mod.rs (Module, Op,
    index domains); crates/psrs-backend/src/wasm/lower/.
  Tests: wasm::tests::defined_types_precede_function_types;
    mir::tests::defined_types_flow_into_the_wasm_type_section;
    the structure_tests/reducible_tests exercise If/Block/Loop regions.
  Input boundary: verified MIR.
  Commands: cargo test -p psrs-backend wasm::.
  Result: pass.
  Gaps: none.
```

```text
ENC-02:
  Implementation: crates/psrs-backend/src/wasm/encode.rs (index assignment,
    section order); crates/psrs-backend/src/wasm/lower/mod.rs.
  Tests: wasm::tests::{defined_types_precede_function_types,
    rejects_a_data_index_that_does_not_match_module_order,
    rejects_an_export_with_the_wrong_index_domain,
    interns_repeated_string_literals_in_one_lazy_global}.
  Input boundary: verified MIR.
  Commands: cargo test -p psrs-backend wasm::.
  Result: pass.
  Gaps: none.
```

```text
ENC-03:
  Implementation: crates/psrs-backend/src/wasm/lower/structure/closure.rs and
    the declared element segment in encode.rs.
  Tests: wasm::tests::encodes_and_runs_a_gc_struct; driver closure execution
    tests (tests::functions capturing-closure cases).
  Input boundary: verified MIR and executed components.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend wasm::;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass.
  Gaps: none.
```

```text
ENC-04:
  Implementation: crates/psrs-backend/src/wasm/lower/realloc/ (synthesized
    cabi_realloc); data segment emission in encode.rs.
  Tests: wasm::lower::realloc::tests::
    realloc_checks_alignment_overflow_and_growth_and_copies_old_bytes;
    mir::wit::tests::records::record_arguments_flatten_in_wit_field_order.
  Input boundary: verified MIR and executed components.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend wasm::.
  Result: pass.
  Gaps: none.
```

```text
ENC-05:
  Implementation: crates/psrs-backend/src/wasm/lower/mod.rs (entry synthesis
    and exit-with-code call).
  Tests: mir::tests::wasm_lowering_requires_an_explicit_entry_symbol;
    every executed component (mir::tests::runs_a_mir_gc_struct_under_wasmtime,
    mir::gc_tests) runs through the synthesized entry.
  Input boundary: verified MIR and executed components.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend.
  Result: pass.
  Gaps: none.
```

```text
ENC-06:
  Implementation: crates/psrs-backend/src/wasm/verify.rs.
  Tests: wasm::tests::{accepts_a_branch_to_the_function_label_from_a_nested_block,
    accepts_branches_to_the_function_label_through_raw_wasm_labels,
    rejects_a_branch_depth_beyond_the_raw_labels,
    rejects_a_branch_depth_beyond_the_structured_labels,
    rejects_a_branch_depth_outside_its_enclosing_labels,
    interns_repeated_string_literals_in_one_lazy_global,
    rejects_a_data_index_that_does_not_match_module_order,
    rejects_an_export_with_the_wrong_index_domain}.
  Input boundary: malformed and valid thin Wasm IR.
  Commands: cargo test -p psrs-backend wasm::.
  Result: pass.
  Gaps: none.
```

```text
ENC-07:
  Implementation: crates/psrs-backend/src/lib.rs (validator_for);
    crates/psrs-backend/src/wasm/verify.rs; WAT via wasmprinter.
  Tests: capability::tests::target_validator_rejects_simd_when_its_gate_is_disabled;
    every backend execution harness validates before running
    (mir::gc_tests::run_gc_output validates with validator_for);
    driver compile validates the component.
  Input boundary: encoded core module and component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass.
  Gaps: none.
```

```text
ENC-08:
  Implementation: crates/psrs-backend/src/capability.rs (wasm_features).
  Tests: capability::tests::wasm_feature_flags_follow_independent_profile_fields,
    stable_profile_matches_the_documented_wasi_0_2_target.
  Input boundary: target profile.
  Commands: cargo test -p psrs-backend capability.
  Result: pass; each field controls only its matching wasmparser feature.
  Gaps: none; `component_implements` has no wasmparser flag in the pinned
    release and is recorded as such.
```

```text
ENC-09:
  Implementation: crates/psrs-backend/src/mir/verify/capability.rs;
    crates/psrs-backend/src/cc/verify capability paths;
    driver diagnostic mapping.
  Tests: mir::verify::capability::* (source-attributed gate failures);
    psrs-driver tests::wasi::
    rejects_a_wasi_interface_outside_the_component_capability_profile.
  Input boundary: verified MIR with reduced profiles and source diagnostics.
  Commands: cargo test -p psrs-backend capability;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass.
  Gaps: none.
```

```text
ENC-10:
  Implementation: test harnesses validate then execute under Wasmtime.
  Tests: mir::gc_tests::* (each validates with validator_for then runs);
    wasm::tests::encodes_and_runs_a_gc_struct; driver execution suites.
  Input boundary: verified MIR and source.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass, nothing skipped.
  Gaps: none.
```

```text
ENC-11:
  Implementation: wasm/lower/structure/region.rs and dispatcher.rs encode
    ReturnCall/ReturnCallRef.
  Tests: psrs-driver tests::tail_calls::
    non_self_tail_calls_use_return_call_when_enabled,
    indirect_tail_recursion_uses_return_call_ref (assert WAT opcodes and run).
  Input boundary: source with an enabled `tail_call` profile.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::tail_calls.
  Result: pass.
  Gaps: none.
```

## Remaining work and blockers

Topic obligations are Verified. The broader BE-13/BE-14/BE-16 rows stay
`Partial`/`Planned` for remaining MIR instruction/control-flow forms, official
`L6/M7` gates, multi-value, bulk memory, and other optional proposal families
(all recorded in the capability-profile implementation notes and D-04).
