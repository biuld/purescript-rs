# Effects Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Effects](../../design/backend/fp/effects.md)

**Progress:** `lower_effects` replaces the opaque `Prelude.Effect` application
with a one-parameter closure after Typed Core and records every closure it
wrote. `EffectLowering::verify` rejects a recorded closure whose parameter list
is not `[Token]` or whose result is not the lowered effect result. The executed
negative fixtures call that check after replacing the node; they stay in Core.
EF-01 through EF-11 are verified on that encoding, including a saturated
`log "message"` and a partial application of an effect-returning function.
`callable_types` remains on the typed module and is always empty.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-21, with BE-02 and BE-26 at closure/library boundaries.

## Scope and dependencies

Complete the linked design's `Effect a` representation, `pure`, `bind`,
`runEffect`, hidden execution token, and sequencing through CC/MIR/Wasm. A
computation value is inert until an authorized runner invokes it. The linked
design's present-tense contract is authoritative beyond this matrix. Source
do/ado desugaring and class elaboration are frontend inputs; WASI service
availability belongs to the platform topic. This topic must verify the
ordinary closure interface and the trusted entry boundary. The embedded
library declares `foreign import data Effect` and abstract `psrs:effect`
operations; `lower_effects` supplies their closures.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. A
Verified row needs behavior-sensitive execution, not only a closure-shaped IR.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| EF-01 | `Effect a` is abstract through checking and Typed Core. One representation lowering emits a token closure; later passes do not match `Effect` or consult `callable_types`. | Inspect the lowering and generated CC/MIR; a source fixture cannot pass a function as an effect or name the token. | Verified |
| EF-02 | Constructing, storing, passing, returning, or capturing an Effect value performs no action. | Compile programs that build and discard or store an effect; mandatory execution asserts no import call/output before run. | Verified |
| EF-03 | `pure` returns the supplied value when run and invokes no external action. | Source and verified Core cases for scalar/reference values; inspect closure call count and value-sensitive result. | Verified |
| EF-04 | `bind` runs the first effect before applying the continuation and then runs the returned effect exactly once. | Observable output/call-count order, including a continuation that ignores its argument, nested binds, and expected traps. | Verified |
| EF-05 | `runEffect` is available only to trusted entry/runtime code and invokes the closure once per call. | Unauthorized source call fails with source-associated diagnostic; two authorized runs produce two actions and a single run one action. | Verified |
| EF-06 | Under-application of a source arrow captures the supplied arguments. The token is a parameter of the effect closure, not a remaining parameter of a function such as `log :: String -> Effect Unit`. | `log "message"` is a saturated call that returns a closure; a genuinely partial source application still captures once and defers the action. | Verified |
| EF-07 | Core/P8 preserve strict source order of `let`, effect construction, and effect execution. | Source order cases with distinguishable WASI outputs, failures, and nested/conditional effects; compare Core, CC and runtime sequence. | Verified |
| EF-08 | Polymorphic `Effect a` uses the normal erasure, boxing and closure adapters without exposing the token. | Execute effects returning Int, Number, String and a GC aggregate through generic functions; inspect signatures and recovered values. | Verified |
| EF-09 | Linked source modules forward Effect values without running them or granting untrusted modules runner privilege. | Producer/consumer modules with delayed execution, repeated forwarding, and unauthorized `runEffect` attempt. | Verified |
| EF-10 | Wasm/component entry executes only the selected trusted action and preserves WASI call order, results, and failures. | Mandatory Wasmtime component execution with stdout/stderr or another observable import, call counts, exit behavior, and valid binary. | Verified |
| EF-11 | A representation closure whose parameter list is not `[Token]`, or whose result is not the lowered effect result, fails verification before encoding. | Negative fixtures for the closure emitted by representation lowering, including an `Effect (a -> b)` closure that was flattened to arity two. | Verified |
| EF-12 | `trap` is the `Effect Unit` whose application ends the guest instead of returning, and the effect chain sequenced after it does not run. | A failing library assertion writes its message and traps; a held one lets the program finish; a statement after the trap never writes. | Verified |

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

Records (one test may support several IDs). Revision: `253809f` plus the
uncommitted changes described here; Wasmtime 49.0.1 (46c23a87d 2026-09-24).
A row's own `Revision` line records when that row was first verified.

```text
EF-01:
  Implementation: stdlib/lib/Prelude.purs (`foreign import data Effect`,
    `psrs:effect#pure`, `psrs:effect#bind`, `psrs:effect#run`),
    crates/psrs-core/src/effect/ (`lower_effects`),
    crates/psrs-backend/src/effects.rs (drop synthesized imports and suspend
    host calls), crates/psrs-driver/src/program/effects.rs (runner scope)
  Tests: psrs-driver tests/effects.rs
    a_function_cannot_be_passed_to_run_effect_as_an_effect (P5 type mismatch
    on `runEffect (\token -> 42)`),
    an_untrusted_prelude_effect_remains_an_ordinary_user_type,
    transitive_effect_types_keep_their_closure_representation (Typed Core
    stays a `User` application);
    psrs-driver tests/effect_arity.rs
    an_effect_of_a_function_is_not_arity_two_and_log_is_saturated (CC
    parameter counts, Wasmtime exit 42, empty stdout)
  Input boundary: source diagnostics; Typed Core; generated CC; executed Wasm
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
    effects::; PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
    effect_arity::
  Result: pass under PSRS_REQUIRE_WASMTIME=1 on 2026-10-01. The saturated
    arity test passed in the workspace suite; the partial-application test
    passed in that same command after it was added.
  Revision: uncommitted on issue/wit-abi-resolved-type-lowering
  Gaps: `callable_types` is still a field and is always empty. Closure
    conversion and MIR do not read it.
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
  Implementation: crates/psrs-core/src/effect/ (`pure_declaration`),
    crates/psrs-backend/src/cc/lower/call/partial.rs
  Tests: running_pure_returns_the_supplied_value_without_an_external_action
    (asserts exit code 42 and empty stdout)
  Input boundary: source; executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver effects::
  Result: pass
  Revision: 95aebe3 + uncommitted
  Gaps: none
EF-04:
  Implementation: crates/psrs-core/src/effect/ (`bind` synthesis),
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
    crates/psrs-core/src/effect/ (`runEffect` synthesis, token `0`)
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
  Implementation: crates/psrs-core/src/effect/ (closure parameter list stops
    at the token), crates/psrs-backend/src/cc/lower/call/partial.rs
    (`lower_partial_global_application`),
    crates/psrs-backend/src/cc/lower/call/helpers.rs (saturation stops at the
    source arity when the use-site arity is reached)
  Tests: tests/effect_arity.rs
    an_effect_of_a_function_is_not_arity_two_and_log_is_saturated (`log` has
    one CC parameter; every direct call has one argument; Wasmtime exit 42
    and empty stdout),
    a_partial_source_application_captures_once_and_defers_the_effect (`pick`
    has two parameters; one `partial_` closure captures index 0 once and calls
    `pick` with two arguments; storing `pick true` exits 0 with empty stdout;
    two runs print `again\nagain\n`);
    tests/effects.rs constructing_an_effect_does_not_execute_it,
    a_stored_effect_runs_each_time_it_is_explicitly_run
  Input boundary: source; generated CC; executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
    effect_arity::; PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
    effects::
  Result: pass under PSRS_REQUIRE_WASMTIME=1 on 2026-10-01. The saturated
    `log` case passed in the workspace suite. The partial-application case
    passed in the workspace command that includes it.
  Revision: uncommitted on issue/wit-abi-resolved-type-lowering
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
  Implementation: crates/psrs-core/src/effect/mod.rs (`lower_effects`,
    `rewrite_effect_applications`, `EffectLowering::verify`). The pass records
    each written closure with its token and lowered result and calls `verify`
    before returning. `verify` rejects a recorded node that is no longer that
    closure. crates/psrs-backend/src/effects.rs (`verification_errors`) maps a
    `VerifyError` returned by `lower_effects` to `InvalidCompilerIr` under
    `P8 effect lowering`. The closures the pass writes already match the
    record, so the negative fixtures do not exercise that mapping. CC and MIR
    still reject a call whose arguments do not match an ordinary closure
    signature, unchanged.
  Tests: psrs-core tests::effects::
    lowered_effect_closures_are_one_token_closure_over_the_effect_result (the
    written `[Token]` closure keeps `a -> b` whole as its result, and
    `module.verify()` accepts the table),
    a_lowered_effect_closure_flattened_to_arity_two_is_rejected (the
    `Effect (a -> b)` closure rewritten to `[Token, a]`; the general Core
    verifier accepts that table, this check rejects it with "a lowered effect
    closure must take only the runtime token"),
    a_lowered_effect_closure_with_the_wrong_result_is_rejected (`[Token]` over
    `b`; "a lowered effect closure must return the lowered effect result"),
    a_lowered_effect_that_is_not_a_closure_is_rejected,
    a_module_without_the_library_effect_has_no_lowered_closures; plus existing
    psrs-backend cc::verify::tests::effects and mir::verify::tests::effects
  Input boundary: linked Core type table carrying the opaque
    `Prelude.Effect` application; no source fixture and no Wasm encoding
  Commands: cargo test -p psrs-core tests::effects; cargo test --workspace;
    PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
  Result: `cargo test -p psrs-core --lib tests::effects` passed on 2026-10-01,
    5 tests. Both required negatives run `lower_effects`, replace the recorded
    node, and fail in `EffectLowering::verify` with a Core `VerifyError`. They
    do not enter the backend, CC, MIR, or the encoder.
  Revision: 253809f + uncommitted on issue/wit-abi-resolved-type-lowering
  Gaps: the check sees only the closures this pass recorded, which is the only
    place that still recognizes `Effect`; a table mutated after the pass ran is
    not re-checked. `Type::Closure` stays a general representation: only nodes
    the lowering wrote are constrained to `[Token]`. BE-21 remains the broader
    landing gate for this topic.
EF-12:
  Implementation: stdlib/lib/Prelude.purs (`psrs:effect#trap`,
    `trap :: Effect Unit`), crates/psrs-core/src/effect/operations.rs
    (`trap_declaration`, a one-token closure whose body is the Core trap),
    crates/psrs-core/src/lib.rs (`ExprKind::Trap`),
    crates/psrs-backend/src/cc/lower/literals.rs (`lower_trap`, an
    `Unreachable` assignment), stdlib/lib/Test/Assert.purs (the first consumer)
  Tests: psrs-driver tests/assertions.rs
    a_failed_assertion_traps_with_its_message_when_wasmtime_is_available,
    a_failed_assertion_writes_the_message_before_it_traps,
    assert_true_and_assert_false_report_the_value_that_did_not_hold,
    a_statement_after_a_failed_assertion_never_runs,
    a_held_assertion_lets_the_program_finish_when_wasmtime_is_available
  Input boundary: source (`Test.Assert` on the on-disk library); executed Wasm
    component observed through the process exit status, stdout, and stderr
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
    tests::assertions
  Result: pass under PSRS_REQUIRE_WASMTIME=1 on 2026-10-03. Each failure case
    writes its message first and then traps (`wasm trap: wasm 'unreachable'
    instruction executed`); the held cases exit 0 and the trap stops the chain,
    so the statement after it never writes.
  Revision: uncommitted on feat/ph3-unit-and-test-assert
  Gaps: `trap` is `Effect Unit`, not `forall a. Effect a`: a polymorphic form
    would need the backend to fill a use-site result type on a path that never
    produces one, and no library caller needs it. `assertThrows` needs to
    observe a trap from inside the guest, which the target profile does not
    provide ([DEC-05](../../decision/DEC-05-wasmtime-feature-set.md)), so the
    standard library omits it rather than approximating it.
```

## Discovered obligations

- The token is the Core `Int` chosen by `lower_effects`. The synthesized
  `runEffect` applies the integer `0`. Source programs cannot name that token.
  A later stateful token is an open question in the design, not a second
  representation.
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
