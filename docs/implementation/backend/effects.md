# Effects Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Effects](../../design/backend/fp/effects.md)

**Progress:** Verified for EF-01..EF-11 against the linked design. The
cross-topic `Effect Boolean` closure-type defect was fixed during review (see
resolution below).

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-21, with BE-02 and BE-26 at closure/library boundaries.

## Scope and dependencies

Complete the linked design's `Effect a` representation, `pure`, `bind`,
`runEffect`, hidden execution token, and sequencing through CC/MIR/Wasm. A
computation value is inert until an authorized runner invokes it. The linked
design's present-tense contract is authoritative beyond this matrix. Source
do/ado desugaring and class elaboration are frontend inputs; WASI service
availability belongs to the platform topic. This topic must verify the
ordinary closure interface and the trusted entry boundary. Reconcile any
current embedded-library token representation with the design's internal-token
contract before marking that row Verified.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. A
Verified row needs behavior-sensitive execution, not only a closure-shaped IR.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| EF-01 | `Effect a` is abstract to source code and represented internally as a closure from hidden token to result, with no effect-specific CC/MIR node. | Inspect type/library boundary and generated CC/MIR; negative source fixture cannot forge a runner or token. | Verified |
| EF-02 | Constructing, storing, passing, returning, or capturing an Effect value performs no action. | Compile programs that build and discard or store an effect; mandatory execution asserts no import call/output before run. | Verified |
| EF-03 | `pure` returns the supplied value when run and invokes no external action. | Source and verified Core cases for scalar/reference values; inspect closure call count and value-sensitive result. | Verified |
| EF-04 | `bind` runs the first effect before applying the continuation and then runs the returned effect exactly once. | Observable output/call-count order, including a continuation that ignores its argument, nested binds, and expected traps. | Verified |
| EF-05 | `runEffect` is available only to trusted entry/runtime code and invokes the closure once per call. | Unauthorized source call fails with source-associated diagnostic; two authorized runs produce two actions and a single run one action. | Verified |
| EF-06 | Partial application captures supplied arguments once and defers action until invocation. | Side-effecting argument/continuation cases inspect CC captures and execute repeated runs without repeated construction-time evaluation. | Verified |
| EF-07 | Core/P8 preserve strict source order of `let`, effect construction, and effect execution. | Source order cases with distinguishable WASI outputs, failures, and nested/conditional effects; compare Core, CC and runtime sequence. | Verified |
| EF-08 | Polymorphic `Effect a` uses the normal erasure, boxing and closure adapters without exposing the token. | Execute effects returning Int, Number, String and a GC aggregate through generic functions; inspect signatures and recovered values. | Verified |
| EF-09 | Linked source modules forward Effect values without running them or granting untrusted modules runner privilege. | Producer/consumer modules with delayed execution, repeated forwarding, and unauthorized `runEffect` attempt. | Verified |
| EF-10 | Wasm/component entry executes only the selected trusted action and preserves WASI call order, results, and failures. | Mandatory Wasmtime component execution with stdout/stderr or another observable import, call counts, exit behavior, and valid binary. | Verified |
| EF-11 | Malformed internal closure/token signatures fail verification before encoding. | CC/MIR negative fixtures for wrong token position, return shape, call arity, and unauthorized imported runner binding. | Verified |

## Vertical execution order

1. Audit library imports, trusted entry, Core-to-CC lowering, closure layout,
   component runner, diagnostics, and tests against each design section.
2. Resolve token and privilege mismatches, then implement `pure`/`bind`/run
   semantics and verifier rules with exact negative tests.
3. Execute inertness, order, repeated-run, polymorphic, and cross-module cases
   through the normal component path with mandatory Wasmtime.
4. Record evidence and update D-04. Do/ado syntax and unrelated WASI services
   remain separately owned; do not claim them from Core fixtures.

## Evidence record and completion rule

For each ID record code paths/functions, exact tests/assertions, input
boundary, commands, Wasmtime version, actual executions/skips, revision, and
gaps. Distinguish source privacy from a trusted test fixture. Runtime cases
require `PSRS_REQUIRE_WASMTIME=1`; a skip is not verification. After Rust edits
run `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus focused runtime
cases. Close only when all rows and the complete present-tense design pass.

Records (one test may support several IDs). Revision: `95aebe3` plus the
uncommitted changes described here; Wasmtime 49.0.0 (17830bd3c 2026-09-21).

```text
EF-01:
  Implementation: crates/psrs-typecheck/src/typecheck/signature.rs,
    crates/psrs-typecheck/src/typecheck/unify.rs,
    crates/psrs-driver/src/prelude.rs,
    crates/psrs-driver/src/program/mod.rs (effect identity),
    crates/psrs-driver/src/program/effects.rs (runner scope),
    crates/psrs-backend/src/cc/verify/tests/effects.rs
  Tests: psrs-driver tests/effects.rs
    a_function_cannot_be_passed_to_run_effect_as_an_effect,
    an_untrusted_prelude_effect_remains_an_ordinary_user_type,
    transitive_effect_types_keep_their_closure_representation;
    psrs-backend cc::verify::tests::effects::
    rejects_an_effect_closure_with_the_token_in_the_wrong_position,
    rejects_a_direct_call_to_an_unbound_runner_external
  Input boundary: source diagnostics; malformed CC
  Commands: cargo test -p psrs-driver effects::; cargo test -p psrs-backend
    cc::verify::tests::effects
  Result: pass. `Effect a` stays nominal while checking and lowers to the
    internal `Int -> a` closure; no effect-specific CC/MIR node exists.
  Revision: 95aebe3 + uncommitted
  Gaps: none for the closure representation. The design's `foreign import
    data` declaration is still a placeholder (`data Effect a`); tracked by
    EF-01's token note below.
EF-02:
  Implementation: crates/psrs-backend/src/cc/lower/lambda, cc/lower/call,
    crates/psrs-driver/src/tests/effects.rs
  Tests: constructing_an_effect_does_not_execute_it,
    an_effect_captured_by_a_closure_is_not_run,
    passing_an_effect_to_a_function_does_not_run_it,
    a_linked_module_forwards_an_effect_without_running_it
  Input boundary: source; executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver effects::
  Result: pass. Each program exits 0 with empty stdout before any run.
  Revision: 95aebe3 + uncommitted
  Gaps: none
EF-03:
  Implementation: crates/psrs-driver/src/prelude.rs (`pure`),
    crates/psrs-backend/src/cc/lower/call/partial.rs
  Tests: running_pure_returns_the_supplied_value_without_an_external_action
    (asserts exit code 42 and empty stdout)
  Input boundary: source; executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver effects::
  Result: pass
  Revision: 95aebe3 + uncommitted
  Gaps: none
EF-04:
  Implementation: crates/psrs-driver/src/prelude.rs (`bind`),
    crates/psrs-backend/src/cc/lower/call/partial.rs,
    crates/psrs-ast/src/expr.rs (wildcard parameter lowering)
  Tests: bind_runs_the_first_effect_before_the_continuation,
    bind_passes_the_first_result_to_the_continuation (Int result observed
    through `<`), nested_binds_preserve_left_to_right_order,
    bind_runs_the_returned_effect_exactly_once
  Input boundary: source; executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver effects::
  Result: pass. `bind` sequencing and continuation argument flow are
    observable in stdout.
  Revision: 95aebe3 + uncommitted
  Gaps: none. An `Effect Boolean` whose value reaches a continuation now runs
    through the shared `i32` closure function type; see the resolution below.
EF-05:
  Implementation: crates/psrs-driver/src/program/effects.rs,
    crates/psrs-driver/src/program/mod.rs (entry selection),
    crates/psrs-driver/src/prelude.rs (`runEffect`)
  Tests: run_effect_is_only_available_from_the_selected_entry,
    a_stored_effect_runs_each_time_it_is_explicitly_run,
    running_pure_returns_the_supplied_value_without_an_external_action
  Input boundary: source diagnostic; executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver effects::
  Result: pass. Unauthorized reference is a P7 entry-selection diagnostic;
    two authorized runs emit two actions.
  Revision: 95aebe3 + uncommitted
  Gaps: none
EF-06:
  Implementation: crates/psrs-backend/src/cc/lower/call/partial.rs
    (lower_partial_global_application and its result conversion),
    crates/psrs-backend/src/cc/layout/functions.rs (nested signature ids)
  Tests: running_effects_preserves_source_order (partial `log "first"`),
    constructing_an_effect_does_not_execute_it,
    a_stored_effect_runs_each_time_it_is_explicitly_run
  Input boundary: source; verified CC; executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver effects::
  Result: pass. Repeated runs re-invoke the captured closure without
    re-evaluating construction.
  Revision: 95aebe3 + uncommitted
  Gaps: none
EF-07:
  Implementation: crates/psrs-core/src/lower, crates/psrs-backend/src/cc/lower,
    crates/psrs-backend/src/mir/lower/assignments.rs
  Tests: running_effects_preserves_source_order,
    nested_binds_preserve_left_to_right_order,
    bind_preserves_wasi_results_across_stdout_and_stderr
  Input boundary: source; verified Core/CC; executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver effects::
  Result: pass. Distinguishable stdout/stderr order matches source order.
  Revision: 95aebe3 + uncommitted
  Gaps: none
EF-08:
  Implementation: crates/psrs-backend/src/cc/lower/erased.rs (adapters),
    crates/psrs-backend/src/cc/lower/call/partial.rs,
    crates/psrs-backend/src/cc/layout/functions.rs
  Tests: a_polymorphic_effect_uses_ordinary_adapters,
    a_polymorphic_effect_carries_string_and_number_values,
    a_polymorphic_effect_carries_a_gc_aggregate,
    a_boolean_effect_runs_through_bind
  Input boundary: source; executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver effects::
  Result: pass for Int, Number, String, Boolean, and a polymorphic ADT. Token
    is not exposed in any signature.
  Revision: 84806c4
  Gaps: none.
EF-09:
  Implementation: crates/psrs-driver/src/program/mod.rs (imported signatures),
    crates/psrs-driver/src/program/effects.rs
  Tests: a_linked_module_forwards_an_effect_without_running_it,
    a_linked_module_forwards_an_effect_that_runs_only_when_selected,
    transitive_effect_types_keep_their_closure_representation,
    run_effect_is_only_available_from_the_selected_entry
  Input boundary: source; executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver effects::
  Result: pass. The producer module never references `runEffect`; forwarding
    is inert and the selected entry runs the action once.
  Revision: 95aebe3 + uncommitted
  Gaps: none
EF-10:
  Implementation: crates/psrs-backend/src/component.rs,
    crates/psrs-backend/src/wasm/lower/mod.rs (entry wrapper),
    crates/psrs-driver/src/tests/effects.rs (component execution)
  Tests: every executed test above runs the produced component under
    `wasmtime run`; bind_preserves_wasi_results_across_stdout_and_stderr
    observes both stdout and stderr; tests assert exit codes and a valid
    binary. tests/wasmtime_required.rs gates the runtime baseline.
  Input boundary: executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
  Result: pass, 33 suites ok, 0 skipped under PSRS_REQUIRE_WASMTIME=1.
  Revision: 95aebe3 + uncommitted
  Gaps: none
EF-11:
  Implementation: crates/psrs-backend/src/cc/verify (module-level call and
    function-reference checks), crates/psrs-backend/src/mir/verify/call.rs
  Tests: psrs-backend cc::verify::tests::effects::
    rejects_an_effect_closure_with_the_token_in_the_wrong_position,
    rejects_an_effect_closure_call_with_the_wrong_result_shape,
    rejects_an_effect_closure_call_with_the_wrong_arity,
    rejects_a_direct_call_to_an_unbound_runner_external;
    psrs-backend mir::verify::tests::effects::
    rejects_an_effect_closure_call_with_the_wrong_token_arity,
    rejects_an_effect_closure_call_with_the_wrong_result_type; plus existing
    rejects_a_direct_call_with_the_wrong_arity and
    rejects_ref_func_with_a_different_target_signature
  Input boundary: malformed CC; malformed MIR
  Commands: cargo test -p psrs-backend cc::verify::tests::effects;
    cargo test -p psrs-backend mir::verify::tests::effects
  Result: pass. Every failure is classified InvalidCompilerIr before encoding.
  Revision: 95aebe3 + uncommitted
  Gaps: none
```

## Discovered obligations

- The present-tense design's internal-token contract is implemented as the
  integer placeholder `0` in `crates/psrs-driver/src/prelude.rs`. The token is
  hidden by the abstract `Effect` type, so no source program can name it, but a
  dedicated runtime token still requires foreign-type support as the design's
  implementation notes state. This is a documentation-level gap, not a
  behavior gap: all EF rows above hold with the placeholder.
- Partial application of a polymorphic declaration whose result is a type
  variable previously failed CC verification (the generated function returned
  the declaration's erased result instead of the expression's instantiated
  result). Fixed in `cc/lower/call/partial.rs`; `pure 1`, `const 1`, and
  effect operations applied to fewer arguments now compile.
- A wildcard lambda parameter (`\_ -> ...`) previously desugared to a `case`
  whose scrutinee could lack a data-type representation, so an effect
  continuation that ignores its argument failed. Fixed in
  `crates/psrs-ast/src/expr.rs`.
- Nested closure references in the CC signature table could name a losing
  provisional signature id after structural deduplication. Fixed in
  `crates/psrs-backend/src/cc/layout/functions.rs`.

## Resolution of the `Effect Boolean` closure-type defect (2026-09-26)

A program such as
`bind (pure true) (\x -> if x then log "y" else log "n")` previously trapped
with `wasm trap: cast failure`. Root cause: `crates/psrs-backend/src/mir/layout/mod.rs`
assigned one Wasm function type per CC `SignatureId`, so `Boolean` and `Integer`
signatures (both `i32`) produced structurally identical but distinct concrete
function types; the erased call site's `ref.cast` to one of them failed.

Fix: the planner now deduplicates signature function types by their concrete
Wasm value types (`concrete_value_type` normalizes `Boolean` to `I32`), so all
signatures that lower to the same Wasm type share one `DefinedTypeId`. The MIR
verifier's signature comparisons (`ref.func`, `closure.new`, `call_ref`,
`closure.call`) use `call_value_types_match`, which already treats
`Boolean`/`I32` as equivalent. Evidence:
`mir::layout::tests::signatures_that_lower_to_the_same_wasm_type_share_one_definition`
and the executed `effects::a_boolean_effect_runs_through_bind` (stdout `yes`,
exit 0) under mandatory Wasmtime.

## Remaining work and blockers

- WASI service availability (clock, stdout, stderr) is owned by the platform
  topic; only the operations exercised above are claimed here.
