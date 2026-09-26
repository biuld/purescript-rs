# Optimization Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Backend optimization](../../design/backend/opt/README.md), with
[Typed Core optimization](../../design/backend/opt/core.md) (P7) and
[MIR optimization](../../design/backend/opt/mir.md) (P10).

**Progress:** OPT-01 through OPT-14 Verified. The official `M8-O`
optimization/`CoreFn` compatibility gate and broader pass coverage remain
tracked on the broader `BE-12` feature row in
[D-04](../../design/D-04-suite-roadmap.md#backend-feature-matrix).

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-12.

## Scope and dependencies

Complete the two optimization stages defined by the linked designs: P7 over
verified Typed Core (`crates/psrs-core/src/opt/`) and P10 over verified MIR
(`crates/psrs-backend/src/mir/opt/`). Every pass takes and returns the same
representation, verifies its input and output, preserves source spans on
retained operations, and is optional for correctness. This topic does not own
layouts (P9), structuring or tail calls (P10 structurer,
[control flow](../fp/control-flow-and-tail-calls.md)), or the official
`M8-O` suite gate, which stays on the broader BE-12 row.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. A
Verified row requires named implementation, negative/equivalence checks, and
differential execution where behavior is observable.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| OPT-01 | P7 folds total scalar operations with exact source semantics and never folds a primitive that would trap. | Wrapping, division-by-zero, and euclidean-remainder folding cases with span checks. | Verified |
| OPT-02 | P7 reduces known `if`/`case` only when decidable without a nested erased-cast test, preserving first-match and scrutinee evaluation. | Known-constructor, unknown-scrutinee, and out-of-range-index cases. | Verified |
| OPT-03 | P7 projects known record/dictionary fields while preserving input evaluation order and construction behavior. | Field-order trace test plus dictionary projection through the same rule. | Verified |
| OPT-04 | P7 bounded inlining binds arguments once, in order, preserves effects/traps/spans, and keeps recursive calls. | Effectful-argument beta, named-global order/span, recursion-stays-call, and case-body exclusion tests. | Verified |
| OPT-05 | P7 removes only inert dead bindings and retains effectful or trapping initializers. | Inert-unused removal and effectful-operand retention tests. | Verified |
| OPT-06 | P7 specialization is bounded, deduplicated, retains the generic declaration, and falls back on unresolved or cross-module calls. | Distinct/nested instances, dedup, generic fallback, cross-module, and budget tests. | Verified |
| OPT-07 | P7 preserves checked types, scopes/IDs, spans, and the validated external binding table, verifying before and after each pass. | Span-preservation assertions and the unchanged binding-validation tests. | Verified |
| OPT-08 | P10 runs the documented pass order and verifies MIR before and after every enabled pass. | `mir::opt::optimize` verifies around each pass; focused pass tests exercise the driver. | Verified |
| OPT-09 | P10 constant propagation follows the lattice (loops stay `Unknown`, block parameters constant only when all incoming agree) and does not fold trapping operations. | Total-folding/kept-trap test and branch-merge/loop fixtures. | Verified |
| OPT-10 | P10 terminator simplification and reachability pruning preserve switch shape, trap, and effect order. | Branch/merge fold+prune and switch-selector/successor-preservation tests. | Verified |
| OPT-11 | P10 bounded inlining clones a single-block callee with fresh IDs, preserving spans, traps, effect order, and the callee result value. | Inline tests plus the copied function-result test. | Verified |
| OPT-12 | P10 value forwarding and dead pure-instruction elimination treat `Function.result` and effectful operations as live. | Copy-forwarding result test and the kept-trapping-operation test. | Verified |
| OPT-13 | P10 reachable-import projection keeps imports used by direct calls, function references, and closure construction. | Import projection tests including closure construction. | Verified |
| OPT-14 | Differential execution compares optimized and unoptimized MIR through the same Wasm target, including traps. | Arithmetic and division-by-zero differential components. | Verified |

## Vertical execution order

1. Audit P7 and P10 code and tests against the design pass lists and invariants.
2. Close evidence gaps with equivalence/differential tests rather than weakening
   a pass.
3. Record evidence and update BE-12 without promoting the broader official-suite
   gate from this topic alone.

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names, input boundary,
commands, runtime, executed/skipped cases, revision, and gaps. Runtime cases use
`PSRS_REQUIRE_WASMTIME=1`. After Rust edits run `cargo fmt --all --check`,
`cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`.

## Recorded evidence

Revision: `6c231d2` plus the optimization evidence changes in this worktree.
Runtime: `wasmtime 49.0.0` under `PSRS_REQUIRE_WASMTIME=1`.

```text
OPT-01:
  Implementation: crates/psrs-core/src/opt/simplify.rs.
  Tests: psrs-core opt::tests::folds_wrapping_integer_arithmetic_and_keeps_the_operation_span,
    leaves_constant_division_that_would_trap,
    retains_an_unused_euclidean_division_that_may_trap.
  Input boundary: verified Typed Core.
  Commands: cargo test -p psrs-core opt::.
  Result: pass.
  Gaps: none.
```

```text
OPT-02:
  Implementation: crates/psrs-core/src/opt/simplify.rs.
  Tests: psrs-core opt::tests::selects_a_known_constructor_case_and_substitutes_its_field,
    opt::tests::edge::does_not_select_a_wildcard_after_an_unchecked_unknown_scrutinee,
    opt::tests::edge::out_of_range_array_index_remains_a_trapping_operation.
  Input boundary: verified Typed Core.
  Commands: cargo test -p psrs-core opt::.
  Result: pass.
  Gaps: none.
```

```text
OPT-03:
  Implementation: crates/psrs-core/src/opt/simplify.rs (record and dictionary
    projection share one known-record rule).
  Tests: psrs-core opt::tests::projection_from_a_known_record_preserves_field_evaluation_order.
  Input boundary: verified Typed Core.
  Commands: cargo test -p psrs-core opt::.
  Result: pass.
  Gaps: none.
```

```text
OPT-04:
  Implementation: crates/psrs-core/src/opt/inline/.
  Tests: psrs-core opt::tests::beta_reduction_binds_an_effectful_argument_once_and_before_the_body;
    opt::tests::global_inline::named_global_inlining_binds_arguments_once_before_effects_and_preserves_spans,
    global_inline::named_global_recursion_stays_as_a_call,
    global_inline::leaves_case_bodies_out_of_global_inlining_to_keep_diagnostics_singular.
  Input boundary: verified Typed Core.
  Commands: cargo test -p psrs-core opt::.
  Result: pass.
  Gaps: none.
```

```text
OPT-05:
  Implementation: crates/psrs-core/src/opt/dead.rs.
  Tests: psrs-core opt::tests::removes_only_inert_unused_bindings,
    algebraic_zero_does_not_remove_an_effectful_operand.
  Input boundary: verified Typed Core.
  Commands: cargo test -p psrs-core opt::.
  Result: pass.
  Gaps: none.
```

```text
OPT-06:
  Implementation: crates/psrs-core/src/opt/specialize/.
  Tests: psrs-core opt::tests::specialization::
    creates_distinct_specializations_for_distinct_concrete_type_arguments,
    specializes_concrete_calls_deduplicates_and_keeps_generic_fallback,
    specializes_nested_concrete_types_and_respects_budgets,
    does_not_specialize_a_call_that_still_has_a_type_variable,
    keeps_cross_module_calls_on_the_generic_declaration,
    inlines_a_small_specialization_without_discarding_the_generic_declaration.
  Input boundary: verified Typed Core.
  Commands: cargo test -p psrs-core opt::.
  Result: pass.
  Gaps: none.
```

```text
OPT-07:
  Implementation: crates/psrs-core/src/opt/mod.rs (verify before/after each
    pass); span/ID preservation in inline and specialize.
  Tests: psrs-core opt::tests::folds_wrapping_integer_arithmetic_and_keeps_the_operation_span,
    global_inline::named_global_inlining_binds_arguments_once_before_effects_and_preserves_spans;
    external binding validation is unchanged by P7
    (crates/psrs-backend/src/bindings tests; psrs-driver integration).
  Input boundary: verified Typed Core.
  Commands: cargo test -p psrs-core opt::; cargo test -p psrs-backend bindings.
  Result: pass.
  Gaps: none.
```

```text
OPT-08:
  Implementation: crates/psrs-backend/src/mir/opt/mod.rs (`optimize` verifies
    after every pass).
  Tests: every mir::opt test runs `optimize`; mir::opt::tests::control_flow and
    inline/constants/copy tests exercise the driver order.
  Input boundary: verified MIR.
  Commands: cargo test -p psrs-backend mir::opt.
  Result: pass.
  Gaps: none.
```

```text
OPT-09:
  Implementation: crates/psrs-backend/src/mir/opt/constants.rs.
  Tests: mir::opt::tests::constants::folds_total_arithmetic_but_keeps_an_unused_trapping_operation;
    mir::opt::tests::control_flow::folds_branch_merges_prunes_blocks_and_projects_imports.
  Input boundary: verified MIR.
  Commands: cargo test -p psrs-backend mir::opt.
  Result: pass.
  Gaps: none.
```

```text
OPT-10:
  Implementation: crates/psrs-backend/src/mir/opt/cfg.rs.
  Tests: mir::opt::tests::control_flow::
    folds_branch_merges_prunes_blocks_and_projects_imports,
    preserves_switch_selectors_and_all_successors_through_p10.
  Input boundary: verified MIR.
  Commands: cargo test -p psrs-backend mir::opt.
  Result: pass.
  Gaps: none.
```

```text
OPT-11:
  Implementation: crates/psrs-backend/src/mir/opt/inline.rs.
  Tests: mir::opt::tests::inline::{inlines_small_direct_functions_and_folds_the_result,
    inlining_uses_the_callee_function_result_value}.
  Input boundary: verified MIR.
  Commands: cargo test -p psrs-backend mir::opt.
  Result: pass.
  Gaps: none.
```

```text
OPT-12:
  Implementation: crates/psrs-backend/src/mir/opt/values.rs,
    effects.rs.
  Tests: mir::opt::tests::copy::copy_forwarding_updates_the_structurer_result_value;
    mir::opt::tests::constants::folds_total_arithmetic_but_keeps_an_unused_trapping_operation.
  Input boundary: verified MIR.
  Commands: cargo test -p psrs-backend mir::opt.
  Result: pass.
  Gaps: none.
```

```text
OPT-13:
  Implementation: crates/psrs-backend/src/mir/opt/imports.rs.
  Tests: mir::opt::imports::tests::retains_imports_referenced_by_closure_construction;
    mir::binding_tests::p9_does_not_emit_unreachable_external_bindings.
  Input boundary: verified MIR.
  Commands: cargo test -p psrs-backend mir::opt.
  Result: pass.
  Gaps: none.
```

```text
OPT-14:
  Implementation: the normal optimized pipeline and the unoptimized
    `mir::lower_module_with_capabilities` path, both through
    `wasm::lower_module_with_capabilities`.
  Tests: mir::gc_tests::optimized_and_unoptimized_arithmetic_agree_on_an_oracle
    (Rust wrapping-i32 oracle); mir::gc_tests::optimized_and_unoptimized_traps_agree
    (both trap on `1 / 0` with "divide by zero"); driver effect, branch, and
    recursion suites run the optimized pipeline under mandatory Wasmtime.
  Input boundary: CC fixture and source; executed components.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-backend gc_tests;
    PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib.
  Result: pass; both pipelines agree on values and traps.
  Gaps: none for MIR; P7 differential coverage is by the Core optimization
    tests above rather than a separate unoptimized Core execution path.
```

## Remaining work and blockers

The optimization topic's own obligations are Verified. The broader BE-12 row
stays `Partial` until both stages reach the official `M8-O` optimization and
`CoreFn` compatibility gate and named-global/cross-module specialization
coverage grows under explicit linkage rules.
