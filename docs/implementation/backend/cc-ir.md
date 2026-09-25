# CC IR Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [CC IR](../../design/backend/fp/cc-ir.md)

**Progress:** CC-01, CC-02, CC-04, CC-06..CC-13 Verified; CC-03 Blocked on the
distinct `ValueShape::String` gap; CC-05 In progress for the same reason.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-01 and BE-02.

## Scope and dependencies

Complete the linked design's present-tense CC contract from verified Typed Core
through P8 and CC verification. The design's Model, Algorithms, Code map,
Invariants, and Boundaries remain authoritative if a row below misses an
obligation. This topic owns target-neutral ANF, closure conversion, abstract
representation requirements, external bindings, and their verifier. Concrete
GC layouts, MIR CFGs, WIT adaptation, and the pattern decision algorithm have
their own topics; exercise their interfaces here where CC must supply input.
Keep source spans and Core's left-to-right evaluation order. Use the design's
Code map as the organization target, recording justified equivalent ownership.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. Every
Verified row needs the evidence record below; existing code alone is not
acceptance. A Blocked row names the dependency and resumption condition.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| CC-01 | P8 consumes verified Typed Core and emits a separate CC module with stable function symbols, parameters, ordered assignments, and source spans. | Trace a Core fixture through P8; reject missing symbols, duplicate definitions, or misplaced parameters in CC verification. | Verified |
| CC-02 | ANF names intermediate computations in Core evaluation order, including callee before arguments, strict `let`, and branch-local work. | Side-effecting or trapping operands expose order in source-to-component execution; inspect CC assignment order and branch placement. | Verified |
| CC-03 | CC values use target-neutral `ValueShape`, `ReprId`, and `SignatureId`; strings retain their semantic shape and nominal identities stay distinct. | Inspect interning and representation tables for equal/different keys, recursive reservations, and absence of Wasm heap indices or ABI pointer details in CC. | Blocked |
| CC-04 | Function signatures and representation requirements are complete before P9 and stable across linked source modules. | Compile mutually referring modules and recursive declarations; check one canonical handle per equivalent requirement and no unresolved handle. | Verified |
| CC-05 | The full CC operation vocabulary has exact operand/result shapes and preserved spans. | Exercise constants, primitives, calls, products, variants, arrays, reference tests/casts, conversions, `If`, and unreachable/switch forms that the design permits; malformed fixtures reject wrong shapes. | In progress |
| CC-06 | Lambda lifting computes ordered, deduplicated free captures and binds lifted parameters before body assignments. | Nested and escaping closures capture scalars, references, and functions; inspect capture order and execute later reads. | Verified |
| CC-07 | Global function values, partial applications, and recursive function groups have callable wrappers and valid capture environments. | Execute direct and indirect calls, underapplication and overapplication, mutually recursive closures, and an escaping recursive function; check one evaluation of supplied operands. | Verified |
| CC-08 | Erased/concrete function adapters use exact signatures and capture the adapted value once. | Inspect CC adapters in both directions and execute value-sensitive calls through the same P8-to-Wasm path; coordinate aggregate cases with [generic aggregate erasure](generic-aggregate-erasure.md). | Verified |
| CC-09 | External bindings are validated against Core and CC, then projected without dropping valid unused declarations too early. | Positive imported-call and unused-binding fixtures; malformed names, signatures, duplicate bindings, and missing references fail at the correct boundary. | Verified |
| CC-10 | CC verification checks every definition/use, assignment order, captures, calls, branches, representation operations, and aggregate plans. | Full-module negative fixtures for undefined/non-dominating values, wrong capture index/type, call arity/signature, branch shape, and invalid representation evidence. | Verified |
| CC-11 | CC-to-P9 is explicit and preserves source ranges and one evaluation of each computation. | Inspect P9's input contract; run an executable component whose observable outputs distinguish repeated or reordered evaluation. | Verified |
| CC-12 | In-scope failures produce source-associated diagnostics; malformed internal CC is rejected before MIR emission. | Assert diagnostic kind and span for unsupported input and verify internal-error paths on intentionally malformed CC. | Verified |
| CC-13 | A global function value's wrapper exposes the declaration's full flattened callable arity, and generated partial/adapted callables use the shared stable symbol allocator. | Execute an over-applied alias, an under-then-applied partial, and a declaration that returns a closure; compile partial applications in two linked source modules with identical layouts. | Verified |

## Vertical execution order

1. Audit Core-to-P8 contracts, existing CC operations, verifier rules, and
   tests; record each row's real gap. Implement the model and interning first.
2. Finish ANF, closure and adapter lowering, external bindings, and all CC
   operations needed by the design. Extend verifier checks with negative tests.
3. Compile through P9, Wasm validation, and component execution. Include
   value-sensitive order, capture, recursion, and cross-module cases. Run the
   normal optimizer; inspect pre-optimization CC when it can erase the evidence.
4. Re-audit every present-tense design rule and update the evidence record and
   D-04 status. Do not treat completion of this topic as completion of BE-01/02.

## Evidence record and completion rule

For each ID record implementation paths and entry points; exact test paths,
names, and assertions; input boundary (source, verified Typed Core, or malformed
CC); exact command, runtime version and executed/skipped cases; tested revision;
and remaining gaps. A Typed Core fixture proves backend behavior only: keep any
missing source path explicit. Runtime rows must execute with
`PSRS_REQUIRE_WASMTIME=1`; skipped tests leave those rows unverified. After Rust
changes run `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus the focused
mandatory Wasmtime cases. Finish only when every row is Verified and the design
audit finds no untracked present-tense requirement.

Common commands (worktree root, `CARGO_TARGET_DIR` pointing at this worktree's
target directory):

```sh
cargo fmt --all --check
PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Runtime: `wasmtime 49.0.0 (17830bd3 2026-09-21)`. All execution cases below ran;
nothing was skipped under `PSRS_REQUIRE_WASMTIME=1`.

```text
CC-01:
  Implementation: cc/mod.rs `lower_module`, `lower_module_with_bindings`;
    cc/lower/mod.rs `lower_function` and `GeneratedSymbolAllocator`;
    cc/verify/mod.rs `verify_module`, `verify_function_inner`.
  Tests: cc_ir_audit::cc_declarations_are_well_formed_and_capture_once asserts
    unique CC symbols, parameters that lead the value declarations, and
    well-formed assignment spans;
    cc::verify::tests::structure::rejects_parameters_that_are_not_the_first_value_declarations,
    ::rejects_a_function_symbol_defined_twice,
    ::rejects_an_external_that_conflicts_with_a_function_symbol;
    cc/lower/letrec/tests.rs::mutually_recursive_local_functions_lower_to_explicit_closures
    and ::recursive_function_captures_an_earlier_same_let_value_and_escapes
    assert exact source ranges on generated assignments.
  Input boundary: source and malformed CC.
  Commands: common commands above.
  Result: pass; 205 psrs-backend and 186 psrs-driver library tests executed;
    source-level cases ran under Wasmtime.
  Revision: 775fafe + uncommitted changes.
  Gaps: none.
```

```text
CC-02:
  Implementation: cc/lower/mod.rs `lower_value_inner` (operands in source
    order), cc/lower/call/application.rs (callee before arguments),
    cc/lower/call/partial.rs, cc/lower/mod.rs `If`/`Case` branch-local lists.
  Tests: cc_ir_audit::effects_observe_callee_and_argument_order executes
    `first (runEffect (log "a")) (runEffect (log "b"))` and asserts stdout
    `a\nb\n`, so the callee and both arguments run once, left to right;
    psrs-driver tests/effects.rs::running_effects_preserves_source_order
    asserts `first\nsecond\n`; cc_ir_audit
    ::cc_declarations_are_well_formed_and_capture_once inspects assignment
    order. letrec/tests.rs checks ordered recursive assignments.
  Input boundary: source and executed Wasm.
  Commands: common commands above.
  Result: pass; the ordering cases executed under Wasmtime.
  Revision: 775fafe + uncommitted changes.
  Gaps: none.
```

```text
CC-03:
  Implementation: cc/representation.rs (`ValueShape`, `RefShape`, `ReprId`,
    `SignatureId`, `RepresentationTable::reserve`/`set`/`add_signature`);
    cc/layout/aggregate.rs canonical keys; cc/layout/functions.rs interning and
    `canonicalize_signatures`. `StringConstant` still lowers to
    `ValueShape::Integer`.
  Tests: cc/layout/tests.rs
    ::canonical_record_keys_sort_labels_and_share_equal_keyed_records,
    ::canonical_arrays_key_by_element_shape,
    ::recursive_aggregate_normalization_terminates,
    ::equal_normalized_function_signatures_share_one_signature_id; the CC type
    model names no Wasm value/type/heap or ABI pointer type.
  Input boundary: verified Typed Core and malformed CC.
  Commands: common commands above.
  Result: pass for the implemented part; the String distinction is not
    executed because it does not exist.
  Revision: 775fafe + uncommitted changes.
  Gaps: the design requires a distinct `ValueShape::String`; `cc/representation.rs`
    has none, `cc/layout/scalar.rs` and `cc/mod.rs` map `Type::String`/
    `SourceType::String` to `ValueShape::Integer`, and
    cc/verify/ops/mod.rs requires `StringConstant` to produce `Integer`. This is
    the SP-02 blocker recorded in
    [scalars and primitives](scalars-and-primitives.md) and the deviation
    recorded in [polymorphism and erasure](polymorphism-and-erasure.md):
    adding the variant changes the shared representation model and the erased
    protocol plus the P9 value-type mapping (`mir/layout`), so it needs the
    data-representation topic. Resumption: add the variant and its MIR layout,
    then reject scalar primitives on it.
```

```text
CC-04:
  Implementation: cc/layout/functions.rs `append_function_types`,
    `canonicalize_signatures`; cc/layout/aggregate.rs
    `reserve_aggregate_layouts`/`normalize_aggregate_layouts`; P8 runs
    `verify_module` before returning a `BackendInput`; P9's `PlannedLayout`
    rejects unresolved handles.
  Tests: cc_ir_audit::partial_applications_in_linked_modules_get_distinct_symbols
    compiles two linked source modules with partial applications at the same
    source offset and executes the result; cc/layout/tests.rs
    ::recursive_aggregate_normalization_terminates and
    ::equal_normalized_function_signatures_share_one_signature_id;
    psrs-driver tests/generic_aggregate_audit.rs and
    tests/polymorphism_erasure_audit.rs exercise canonical handles end to end.
  Input boundary: source, verified Typed Core, and malformed CC.
  Commands: common commands above.
  Result: pass; the linked-module case executed under Wasmtime.
  Revision: 775fafe + uncommitted changes.
  Gaps: none.
```

```text
CC-05:
  Implementation: cc/mod.rs `AssignmentKind`; cc/verify/ops/mod.rs,
    ops/table.rs, ops/tag_switch.rs; cc/verify/variant.rs;
    cc/verify/ops/aggregate/mod.rs; cc/case/decision/realize.
  Tests: cc/verify/tests/mod.rs (constants, primitives, arrays, boxes,
    function references, conditionals); ops/aggregate/tests.rs (conversion
    plans); tests/adaptation.rs (representation adapters);
    structure.rs::rejects_a_tag_switch_with_duplicate_tags,
    ::rejects_if_branches_with_different_result_shapes,
    ::rejects_a_direct_call_with_the_wrong_arity;
    cc_ir_audit::erased_function_adapter_executes and
    ::partial_applications_in_linked_modules_get_distinct_symbols execute the
    variant/array/conversion paths. cc/lower/record/tests.rs and
    cc/lower/dictionary/tests.rs cover products and records.
  Input boundary: source, verified Typed Core, and malformed CC.
  Commands: common commands above.
  Result: pass for every implemented operation; `StringConstant` is checked as
    `Integer`.
  Revision: 775fafe + uncommitted changes.
  Gaps: the `StringConstant` result shape follows CC-03; until
    `ValueShape::String` exists the constant's operand/result shapes cannot
    match the design. Resumption follows CC-03.
```

```text
CC-06:
  Implementation: cc/lower/lambda/mod.rs `lower_lambda`,
    cc/lower/lambda/captures.rs `lambda_captures`/`collect_captures` (first-use
    order, deduplicated); `lower_lambda` peels the
    whole lambda chain and eta-expands a lambda that returns a closure; the
    lifted function binds its parameters before body assignments.
    cc/lower/letrec/mod.rs uses the same collector for recursive groups.
  Tests: cc_ir_audit::cc_declarations_are_well_formed_and_capture_once lowers
    `\x -> a + b + x + a`, finds the single lifted closure, and asserts its
    capture indices are exactly `[0, 1]` (ordered, deduplicated on first use);
    cc_ir_audit::nested_lambda_arities_execute executes `\x -> \y -> x + y` and a
    lambda returning a closure;
    psrs-driver tests/functions.rs::runs_a_capturing_lambda_through_a_closure,
    ::preserves_full_width_ints_in_capturing_lambdas, and
    ::runs_a_record_capturing_lambda_through_a_closure execute later reads;
    letrec/tests.rs covers escaping closures.
  Input boundary: source and verified Typed Core.
  Commands: common commands above.
  Result: pass; the capturing cases executed under Wasmtime.
  Revision: 775fafe + uncommitted changes.
  Gaps: none.
```

```text
CC-07:
  Implementation: cc/lower/lambda/mod.rs (`lower_lambda`),
    cc/lower/lambda/wrapper.rs (`make_wrapper`, eta-expanded wrappers);
    cc/lower/global.rs;
    cc/lower/call/application.rs (`lower_partial_global_application`,
    `lower_indirect_application`, over-application fallback);
    cc/lower/call/partial.rs (shared `GeneratedSymbolAllocator`); cc/lower/letrec.
  Tests: cc_ir_audit::partial_application_value_executes (underapplication),
    ::over_application_of_a_global_value_executes (over-application),
    ::global_value_with_partial_lambda_prefix_executes (a declaration returning
    a closure, eta-expanded wrapper),
    ::partial_applications_in_linked_modules_get_distinct_symbols (cross-module
    stable symbols), ::nested_lambda_arities_execute (multi-argument and
    closure-returning nested lambdas), all executed under Wasmtime;
    psrs-driver tests/functions.rs::runs_a_top_level_function_value_through_call_ref,
    ::runs_a_non_capturing_local_lambda_through_call_ref,
    ::invokes_a_polymorphic_global_function_value_at_a_concrete_type;
    letrec/tests.rs::mutually_recursive_local_functions_lower_to_explicit_closures
    and ::recursive_function_captures_an_earlier_same_let_value_and_escapes.
  Input boundary: source and verified Typed Core.
  Commands: common commands above.
  Result: pass; all listed execution cases ran under Wasmtime.
  Revision: 775fafe + uncommitted changes. The shared-allocator fix replaced a
    span-derived symbol that made equal-layout partial applications in two
    linked modules collide; the over-application fallback and eta-expanded
    wrapper make valid curried value uses compile instead of failing arity
    checking.
  Gaps: none.
```

```text
CC-08:
  Implementation: cc/lower/erased.rs `adapt_erased_function_value`
    (captures the adapted value once), `unbox_erased_value`;
    cc/verify/adaptation.rs `verify_erased_adaptation`.
  Tests: cc_ir_audit::erased_function_adapter_executes executes
    `identity (\value -> value)` applied to `42`; psrs-driver
    tests/functions.rs::adapts_a_concrete_lambda_to_a_partially_erased_function,
    ::invokes_a_concrete_function_returned_from_a_polymorphic_call,
    ::runs_a_polymorphic_higher_order_call;
    cc/verify/tests/adaptation.rs negative and positive adapter shapes;
    tests/polymorphism_erasure_audit.rs covers both adapter directions.
  Input boundary: source, verified Typed Core, and malformed CC.
  Commands: common commands above.
  Result: pass; the adapter cases executed under Wasmtime.
  Revision: 775fafe + uncommitted changes.
  Gaps: none.
```

```text
CC-09:
  Implementation: bindings/mod.rs `ExternalBindings::from_core`,
    `validate_core`, `validate_cc`; cc/mod.rs `abstract_signature`,
    `signature_matches_source`; P9 resolves every binding then projects the
    imports referenced by lowered MIR calls.
  Tests: bindings/tests.rs::accepts_a_complete_wit_projection,
    ::rejects_a_missing_wit_binding, ::rejects_a_duplicate_binding,
    ::rejects_a_binding_for_a_non_source_import;
    mir/binding_tests.rs::p9_does_not_emit_unreachable_external_bindings,
    ::p9_emits_a_referenced_external_binding,
    ::p9_rejects_a_binding_that_disagrees_with_cc;
    psrs-driver tests/wasi.rs::rejects_a_wasi_interface_outside_the_component_capability_profile
    asserts the diagnostic kind and span at the P9 boundary.
  Input boundary: verified Typed Core, CC fixtures, and malformed bindings.
  Commands: common commands above.
  Result: pass.
  Revision: 775fafe + uncommitted changes.
  Gaps: none.
```

```text
CC-10:
  Implementation: cc/verify/mod.rs, cc/verify/helpers.rs,
    cc/verify/ops/{mod.rs,table.rs,tag_switch.rs},
    cc/verify/{variant.rs,scalar.rs,adaptation.rs},
    cc/verify/ops/aggregate/mod.rs.
  Tests: structure.rs negatives for misplaced parameters, a value used before
    its assignment, direct-call arity, branch result shapes, non-contiguous
    capture indices, duplicate tag-switch tags, duplicate function symbols, and
    an external/function symbol conflict, each asserting
    `BackendErrorKind::InvalidCompilerIr`; cc/verify/tests/mod.rs and
    tests/adaptation.rs cover value shapes, array/box shapes, captures, erased
    adaptation; ops/aggregate/tests.rs covers conversion evidence;
    ::rejects_an_undeclared_parameter and
    ::rejects_integer_arithmetic_on_number_operands.
  Input boundary: malformed CC.
  Commands: common commands above.
  Result: pass.
  Revision: 775fafe + uncommitted changes.
  Gaps: none.
```

```text
CC-11:
  Implementation: mir/mod.rs `lower_module_with_bindings` (explicit CC input,
    validate_cc first); mir/lower/assignments.rs maps each CC assignment to
    exactly one MIR instruction sequence; mir/lower/mod.rs keeps `source.span`
    on generated instructions and blocks.
  Tests: cc_ir_audit::effects_observe_callee_and_argument_order observes each
    `log` exactly once in order; ::cc_declarations_are_well_formed_and_capture_once
    asserts every CC assignment keeps a well-formed range;
    mir/gc_tests, mir/indirect_tests and mir/tests run the lowered component;
    psrs-driver tests/effects.rs::a_stored_effect_runs_each_time_it_is_explicitly_run.
  Input boundary: source, verified Typed Core, and executed Wasm.
  Commands: common commands above.
  Result: pass; execution cases ran under Wasmtime.
  Revision: 775fafe + uncommitted changes.
  Gaps: none.
```

```text
CC-12:
  Implementation: cc/verify errors now use `BackendError::invalid_ir`;
    cc/mod.rs propagates verifier errors before building the `BackendInput`;
    mir/mod.rs runs `validate_cc` before layout; driver maps every backend
    error to a `Diagnostic` with stage, span, and `kind`.
  Tests: structure.rs::rejects_parameters_that_are_not_the_first_value_declarations
    asserts `BackendErrorKind::InvalidCompilerIr`; bindings/tests.rs asserts the
    P8/P9 boundary messages; psrs-driver
    tests/integration.rs::attributes_backend_errors_to_their_declaring_module
    asserts the failing module and stage;
    tests/wasi.rs::rejects_a_wasi_interface_outside_the_component_capability_profile
    asserts `UnsupportedSource` and the exact source span;
    mir/binding_tests.rs::p9_rejects_a_binding_that_disagrees_with_cc rejects
    malformed CC input before MIR emission.
  Input boundary: source, malformed CC, and malformed bindings.
  Commands: common commands above.
  Result: pass.
  Revision: 775fafe + uncommitted changes.
  Gaps: none.
```

```text
CC-13:
  Implementation: cc/lower/lambda/wrapper.rs `make_wrapper`/`eta_expanded_wrapper`;
    cc/lower/call/application.rs over-application fallback and
    `lower_indirect_application`; cc/lower/call/partial.rs shared allocator.
  Tests: cc_ir_audit::over_application_of_a_global_value_executes,
    ::global_value_with_partial_lambda_prefix_executes,
    ::partial_application_value_executes,
    ::partial_applications_in_linked_modules_get_distinct_symbols, all executed
    under Wasmtime.
  Input boundary: source and verified Typed Core.
  Commands: common commands above.
  Result: pass; the four execution cases ran under Wasmtime.
  Revision: 775fafe + uncommitted changes.
  Gaps: none.
```

## Remaining work and blockers

- **CC-03 / CC-05 — `ValueShape::String`.** The design requires a distinct
  string shape so the CC verifier rejects arithmetic on string pointers, but CC
  folds `I32`/`Char`/`String`/`Unit` to `ValueShape::Integer`. Adding the variant
  changes the shared representation model, the erased protocol, and the P9
  value-type mapping (`mir/layout`, owned by the data-representation topic), so
  it is a cross-topic handoff. Resumption: add the variant and its MIR layout,
  intern string signatures, and tighten `cc/verify/scalar.rs` and
  `cc/verify/ops/mod.rs`; then re-run CC-03 and CC-05.
- **Design doc alignment.** `docs/design/backend/fp/cc-ir.md` now lists
  `TagSwitch`/`Unreachable` in the operation vocabulary and the tag-switch
  verifier invariant, matching
  [pattern matching](../../design/backend/fp/pattern-matching.md) and the implementation.
