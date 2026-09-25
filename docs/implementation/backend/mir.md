# MIR Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [MIR](../../design/backend/fp/mir.md)

**Progress:** Unverified; audit the current implementation before changing a row.

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
| MIR-01 | P9 consumes verified CC through an explicit conversion and creates one typed MIR module, not CC nodes with target fields. | Trace a CC fixture into MIR and inspect types, spans, imports, and absence of unresolved CC representation handles. | Unverified |
| MIR-02 | Functions contain typed basic blocks, block parameters, SSA instructions, and explicit terminators with unique IDs. | Positive CFG fixtures and negative duplicate/missing ID, missing terminator, and cross-function value tests. | Unverified |
| MIR-03 | Reachable `ReprId`s reserve and resolve to exact defined types in one recursion group without merging distinct nominal identities. | Recursive variant/array/record/closure layout fixtures; inspect one-to-one reachable mapping and valid Wasm type section. | Unverified |
| MIR-04 | P9 lowers scalar, product, variant, array, closure, erased, and canonical ABI shapes to exact MIR value types. | Table-driven positive and mismatch tests across every shape; verify concrete and canonical aggregate layouts separately. | In progress |
| MIR-05 | MIR carries the required instruction families with exact input/output and effect contracts. | Check numeric, call/ref, GC, array, memory, import and conversion instructions; malformed modules reject invalid operand and result types. | Unverified |
| MIR-06 | CFG verification proves definition dominance, block-argument arity/types, Boolean branch selectors, switch tags/targets, and return types. | Full-module valid joins/loops and malformed non-dominating uses, bad edges, duplicate tags, and wrong return/signature fixtures. | In progress |
| MIR-07 | Defined-type and reference subtyping obey Wasm mutability, finality, function variance, and nullability rules. | Positive/negative struct, array, function-ref, subtype and cast fixtures; include nullable load into non-null destination rejection. | In progress |
| MIR-08 | Aggregate reconstruction uses typed helpers and private defaultable array allocation; every slot is initialized before exposure. | Inspect lowered maps, verify malformed early read/escape/incomplete-loop fixtures, and execute empty/nested conversions; coordinate evidence with [generic aggregate erasure](generic-aggregate-erasure.md). | In progress |
| MIR-09 | External calls use checked import signatures and the selected canonical ABI boundary; unsupported capabilities fail before encoding. | Positive component import call and negative import/type/capability fixtures with source-associated failures where input has a span. | Unverified |
| MIR-10 | P10 consumes and returns verified MIR without changing its representation contract. | Verify before and after each enabled pass; differential execution on traps, calls, mutable arrays, imports, and aggregate reconstruction. | In progress |
| MIR-11 | Wasm emission mechanically consumes verified MIR and does not make new layout decisions. | Validate emitted core modules/components, inspect representative GC/reference operations, and execute value-sensitive fixtures. | In progress |
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

## Remaining work and blockers

The rows below record the audit and the defects fixed in this pass. Rows still
marked Unverified have not been audited against their full obligation.

```text
MIR-04:
  Implementation: crates/psrs-backend/src/mir/scalar_helpers.rs
    (contains_operation, lower_scalar_helpers, ScalarHelpers::binary_instruction)
  Tests: mir::gc_tests::div_mod::
           generates_helpers_for_division_and_modulo_inside_tag_switch_arms
           (asserts __psrs_euclidean_int_div/_mod are generated for a case and a
            default arm, then executes the component and expects exit 2)
         mir::gc_tests::div_mod::
           lowers_euclidean_integer_division_and_modulo_on_both_targets
         psrs-driver tests::scalars::runs_division_and_modulo_inside_a_case_arm
           (asserts both helper names in the MIR dump and exit code 42)
  Input boundary: CC fixture and source
  Commands: cargo test -p psrs-backend
            PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
  Result: pass; wasmtime installed, execution cases ran
  Revision: cc5d0f4 plus uncommitted changes
  Gaps: product/variant/array/closure/ABI shape coverage beyond scalar helpers

MIR-06:
  Implementation: crates/psrs-backend/src/mir/verify/function.rs
    (verify_terminator requires Branch targets to be parameterless)
    crates/psrs-backend/src/mir/opt/mod.rs (prune unreachable after simplifying
    a terminator, before verifying)
  Tests: mir::verify::tests::rejects_a_branch_target_with_block_parameters
         mir::verify::tests::rejects_values_used_before_definition
         mir::verify::tests::rejects_duplicate_switch_case_values
         mir::verify::tests::rejects_ref_func_with_a_different_target_signature
         psrs-driver tests::functions::runs_a_recursive_polymorphic_reference_identity
           and runs_a_recursive_polymorphic_identity_at_integer_types
  Input boundary: malformed full MIR and source
  Commands: cargo test -p psrs-backend
            PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
  Result: pass
  Revision: cc5d0f4 plus uncommitted changes
  Gaps: dedicated valid-join and loop dominance fixtures remain optional

MIR-07:
  Implementation: crates/psrs-backend/src/mir/verify/instruction/mod.rs
    (ref.test/ref.cast heap relation), mir/verify/subtype.rs (heap_related)
  Tests: mir::verify::tests::gaps::rejects_ref_cast_between_unrelated_heaps
         mir::verify::tests::gaps::accepts_an_erased_upcast_to_eqref
  Input boundary: malformed and valid full MIR
  Commands: cargo test -p psrs-backend
  Result: pass
  Revision: cc5d0f4 plus uncommitted changes
  Gaps: more nullability and function-variance fixtures

MIR-08:
  Implementation: crates/psrs-backend/src/mir/verify/array_map/mod.rs
    (verify_conversion_helpers rejects a cast in place of a nominal rebuild)
  Tests: mir::verify::tests::gaps::
           rejects_a_conversion_helper_that_casts_instead_of_rebuilding
         mir::verify::tests::gaps::
           accepts_a_conversion_helper_that_rebuilds_the_aggregate
         existing mir::verify::arrays and generic aggregate audit execution
  Input boundary: malformed and valid full MIR
  Commands: cargo test -p psrs-backend
  Result: pass
  Revision: cc5d0f4 plus uncommitted changes
  Gaps: exact input/output layout matching needs conversion-plan metadata
    beyond the generated helper name

MIR-10:
  Implementation: crates/psrs-backend/src/mir/opt/mod.rs (prune after simplify),
    mir/opt/constants.rs (keep the Branch merge parameter), and the trap-aware
    Wasm structurer in wasm/lower/structure/{cfg,region,mod}.rs
  Tests: psrs-driver tests::functions::runs_a_recursive_polymorphic_reference_identity,
           runs_a_recursive_polymorphic_identity_at_integer_types,
           runs_a_branch_with_equal_reference_arms, runs_a_case_that_returns_a_reference
           (each asserts exit code 42)
  Input boundary: source, via P7 Core optimization then P9/P10
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
  Result: pass; all four optimizer regressions execute
  Revision: cc5d0f4 plus uncommitted changes
  Gaps: differential execution across traps, mutable arrays, imports, and
    aggregate reconstruction is not yet a single matrix

MIR-11:
  Implementation: wasm/lower/structure/region.rs and mod.rs (block_traps skips
    a value read after a trap); design implementation note
    records why Boolean i31 boxing stays in the encoder
  Tests: psrs-driver tests::functions::runs_a_case_that_returns_a_reference;
         existing wasm::lower::structure and closure execution tests
  Input boundary: valid MIR and source
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
            cargo test -p psrs-backend
  Result: pass
  Revision: cc5d0f4 plus uncommitted changes
  Gaps: Boolean capture i31 stays an encoder rule by documented design

MIR-12:
  Implementation: psrs-backend/src/lib.rs (BackendErrorKind, BackendError::invalid_ir),
    mir/verify/util.rs, wasm/verify.rs, wasm/lower/mod.rs, mir/lower/*,
    psrs-driver/src/lib.rs (Diagnostic.kind)
  Tests: mir::verify::tests::
           backend_error_kinds_distinguish_invalid_ir_from_unsupported_source
         mir::verify::tests::rejects_a_branch_target_with_block_parameters
           (asserts InvalidCompilerIr and the failing span)
         psrs-driver tests::wasi::
           rejects_a_wasi_interface_outside_the_component_capability_profile
           (asserts UnsupportedSource and the "Int" source span)
  Input boundary: malformed MIR and unsupported source
  Commands: cargo test -p psrs-backend
            PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
  Result: pass
  Revision: cc5d0f4 plus uncommitted changes
  Gaps: none for the stated obligation
```
