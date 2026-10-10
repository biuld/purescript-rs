# Effects Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Effects](../../design/backend/fp/effects.md)

**Progress:** EF-01 through EF-13 are Verified on the explicit trusted-identity
contract. The effect-entry measurement on 2026-10-04 was 124/413 (Wasmtime
49.0.2, `purs` 0.15.16). Those 124 files are the previous non-`Int` entries;
each exits 0. A later `Data.Functor` measurement on the same day moved the
board to 125/413; the added file is `passing/3549.purs`.
The 63 files with no selected `main` stay blocked. BE-21 stays Partial. A type
table changed after `lower_effects` returns is not checked again. Historical
records below describe the earlier encoding and are not the current evidence.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-21, with BE-02 and BE-26 at closure/library boundaries.

## Scope and dependencies

The abstract-constructor/dictionary closure boundary is landed in the
vendored-library iteration. The 2026-10-05
[constructor investigation](polymorphism-and-erasure.md#constructor-and-closure-investigation-2026-10-05)
reproduces a matching producer/consumer signature failure without Effect. At
`675f0e3` the discard, delayed-map, and fixed-payload Effect cases compiled
and then trapped. The landed checkpoint recorded there executes those three
programs and the Reader reproduction, and the representation policy is the same
one PE-13 verifies. Historical EF evidence below does not establish that
boundary. The general checked-conversion contract owns the repair; Effect
contributes its trusted token protocol. On this tree the L6/M7 scoreboard moved
from 164/413 to 210/413 and the D-04 and README runtime rows are updated; L1–L5
are unchanged.

Complete the linked design's `Effect a` representation, `pure`, `bind`,
`runEffect`, hidden execution token, and sequencing through CC/MIR/Wasm. An
Effect value defers its operation until an explicit source runner or the
generated Effect Unit command adapter invokes it; strict source arguments are
still evaluated normally. The linked design's contract is authoritative beyond
this matrix. Source do/ado desugaring and class elaboration are frontend inputs;
WASI service availability belongs to the platform topic. This topic verifies
generic closure conversion and the lexical runEffect reference rule, which is
not a capability or non-escape guarantee. The embedded library declares
`foreign import data Effect` and abstract `psrs:effect` operations;
`lower_effects` supplies their closures.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. A
Verified row needs behavior-sensitive execution, not only a closure-shaped IR.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| EF-01 | `Effect a` is abstract through checking and Typed Core. Trusted-library binding resolves the constructor and operation identities once and passes them explicitly to lowering; the runtime form is a generic closure, with no dedicated Effect IR or runtime object. Checked WIT external schemes preserve alias expansion and quantification across THIR, Core, and the backend boundary. | Positive trusted import/re-export cases and same-name untrusted declarations; inspect identity metadata, checked WIT external schemes, and generic closure output. | Verified |
| EF-02 | Effectful WIT imports are classified from trusted identities and checked, synonym-expanded external schemes before type erasure. Only imports with explicit suspension plans receive wrappers; an ordinary WIT import returning a one-parameter closure remains ordinary. The deferred operation is inert, while strict argument expressions keep their source behavior. | Source execution tests check planned-import timing and strict argument behavior. A direct Core/binding fixture proves that an ordinary closure-shaped WIT external is not classified as Effect. | Verified |
| EF-03 | `pure` returns the supplied value when run and invokes no external action. | Source and verified Core cases for scalar/reference values; inspect closure call count and value-sensitive result. | Verified |
| EF-04 | `bind` runs the first effect before applying the continuation and then runs the returned effect exactly once. | Observable output/call-count order, including a continuation that ignores its argument, nested binds, and expected traps. | Verified |
| EF-05 | A direct source reference to trusted `runEffect` is allowed only inside the selected entry declaration. The entry may pass the function value to a helper, which may invoke it; this is not an authority or non-escape guarantee. | A direct reference outside the selected entry receives a source diagnostic; a runner value passed from the entry to a helper is accepted and invokes the closure once per call. | Verified |
| EF-06 | Under-application of a source arrow captures the supplied arguments. The token is a parameter of the effect closure, not a remaining parameter of a function such as `log :: String -> Effect Unit`. | `log "message"` is a saturated call that returns a closure; a genuinely partial source application still captures once and defers the action. | Verified |
| EF-07 | Core/P8 preserve strict source order of `let`, effect construction, and effect execution. | Source order cases with distinguishable WASI outputs, failures, and nested/conditional effects; compare Core, CC and runtime sequence. | Verified |
| EF-08 | Polymorphic `Effect a` uses the normal erasure, boxing and closure adapters without exposing the token. | Execute effects returning Int, Number, String and a GC aggregate through generic functions; inspect signatures and recovered values. | Verified |
| EF-09 | Linked source modules forward Effect values without running them. The same resolved entry identity governs the lexical `runEffect` check across linked modules. | Producer/consumer modules with delayed execution and repeated forwarding; a direct runner reference in a non-entry declaration is rejected. | Verified |
| EF-10 | The command entry preserves the selected source behavior: `Int` keeps its exit code; `Effect Unit` runs its returned action once, returns zero after normal completion, and propagates a guest trap. | Mandatory Wasmtime execution checks exact output, process status, action count, and an explicit trap marker for both entry forms. | Verified |
| EF-11 | Structural verification checks trusted identities and checked WIT operation signatures, every recorded Effect application closure, each import plan against its host wrapper, and the complete transformed Core including generated wrappers. | Malformed identity, checked WIT scheme, import-plan, closure-shape, wrapper-signature, and post-wrapper Core fixtures fail before CC/encoding; these checks are reported as structural evidence only. | Verified |
| EF-12 | `trap` is the `Effect Unit` whose application ends the guest instead of returning, and the effect chain sequenced after it does not run. | A failing library assertion writes its message and traps; a held one lets the program finish; a statement after the trap never writes. | Verified |
| EF-13 | Entry selection resolves one source declaration: prefer `Main.main`, otherwise require a unique top-level `main`. The same `SymbolId` drives the runner check and any generated adapter; accepted result types are `Int` and trusted `Effect Unit`. | Source tests for preferred/fallback/ambiguous selection, aliases of `Effect Unit`, and agreement between selected identity, runner diagnostic, and generated adapter. | Verified |
| EF-14 | Effect application-to-closure lowering composes with the common abstract-constructor representation policy across generic functions, dictionary methods and callbacks. | Execute discard, delayed map and `f Unit` cases with mandatory Wasmtime and exact output/status; retain the non-Effect Reader regression and verify the complete lowered representation. | Verified |

## Current evidence (2026-10-04)

Wasmtime 49.0.2. `purs` 0.15.16. `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace`
passed, as did `cargo clippy --workspace --all-targets -- -D warnings`. The
annotations scoreboard measured L6/M7 at 124/413. The same command, after
the wrapper kept its binders on `quantified` and the closure monotype in `ty`,
measured 124/413 again with the same blocker split, and all 124 files still
exited 0. L1–L5 stayed unchanged. Scoreboard completion is not a golden
comparison: a numeric exit with no trap marker counts as completion.
The focused tests below assert stdout, status, or a trap marker.

```text
EF-01:
  Tests: tests::effects::an_untrusted_prelude_effect_remains_an_ordinary_user_type,
    transitive_effect_types_keep_their_closure_representation;
    psrs-core and psrs-thir external-signature tests.
  Result: pass. Trusted identity is explicit. A same-name untrusted Effect
    stays an ordinary user type. Typed Core keeps the application abstract.
  Gaps: none for this obligation.
EF-02:
  Tests: tests::effects::constructing_an_effect_does_not_execute_it,
    effect_import_alias_is_expanded_before_suspension_planning,
    a_quantified_effect_import_lowers_to_a_monotype_wrapper,
    effect_suspension_conformance_errors_keep_the_imports_source_origin,
    class_constrained_wit_imports_are_rejected_with_a_source_diagnostic.
  Result: pass. A planned import is suspended from its checked scheme.
    The wrapper declaration keeps the scheme's binders on `quantified` and
    the closure monotype in `ty`, for a quantified import as well as a
    monomorphic one. A class-constrained WIT signature is rejected rather
    than dropped.
  Gaps: none for this obligation.
EF-05:
  Tests: tests::effects::run_effect_is_only_available_from_the_selected_entry,
    instance_member_references_do_not_bypass_the_run_effect_scope,
    the_selected_entry_may_pass_run_effect_to_a_higher_order_helper.
  Result: pass. The restriction is lexical. Passing the runner to a helper
    is accepted and is not described as capability confinement.
  Gaps: none for this obligation.
EF-10:
  Tests: tests::effects::an_effect_unit_entry_executes_its_action_once_through_both_source_apis,
    effect_unit_entry_propagates_a_trap_from_the_action,
    effect_unit_entry_runs_strict_construction_effects_before_its_action,
    both_single_source_apis_compile_int_main_with_the_trusted_effect_library.
  Result: pass under mandatory Wasmtime. The Effect Unit entry runs once,
    returns 0, and a trap keeps the earlier stdout and suppresses later output.
    An Int entry still returns its value.
  Gaps: the scoreboard does not compare stdout with an upstream golden.
EF-11:
  Tests: tests::effects::the_backend_rejects_partial_and_duplicate_trusted_operation_metadata,
    the_backend_rejects_operation_identity_and_checked_signature_mismatches,
    the_backend_rejects_an_effect_entry_context_for_an_integer_source_entry,
    effect_command_metadata_must_name_the_selected_source_entry,
    an_effect_source_entry_requires_command_metadata,
    checked_import_verification_keeps_the_foreign_source_module.
  Result: pass. These failures are returned by lowering before encoding.
    Optimization and effect lowering both keep the module recorded on a Core
    verification error.
  Gaps: a type table rewritten after lower_effects returns is not checked
    again. The Core negatives that mutate the table after a successful pass
    still fail in EffectLowering::verify, not in the backend mapping.
EF-13:
  Tests: tests::effects::a_main_in_the_main_module_takes_precedence_and_unique_main_is_the_fallback,
    an_effect_int_entry_remains_invalid,
    effect_unit_entry_accepts_a_type_synonym_and_cross_module_value,
    typechecking_does_not_require_a_unique_command_entry.
  Result: pass. Selection prefers Main.main, otherwise one top-level main.
    Effect Int is rejected. Effect Unit synonyms are accepted.
  Gaps: files with no selected main stay scoreboard blockers (63).
EF-14:
  Implementation: crates/psrs-core/src/instantiation.rs (checked
    instantiation), crates/psrs-backend/src/effects/mod.rs (the Effect
    representation owner registers the runtime token as a
    `RepresentationPolicy` in the Core-to-CC registry), crates/psrs-backend/
    src/boundary.rs (`BoundaryEvidence`), crates/psrs-backend/src/cc/lower/
    conversion/ (transport.rs and callable.rs) and cc/lower/{global,record,erased}
    (evidence read from the boundary at each use). The token is the compiler-owned
    opaque `TypeId::STATE_TOKEN`; effect lowering threads it and `run` supplies the
    `StateToken` value, the `State# RealWorld` analogue.
  Tests: tests::effects::discard_defined_from_bind_sequences_effects (stdout
    `a\nb\n`, exit 0); tests::functor::
    mapping_an_effect_does_not_run_it_until_the_action_runs (stdout
    `before\ntick\n2\n`, exit 0); tests::closure_protocol::
    fixed_unit_payload_through_a_bind_constraint_repeats_the_action
    (`forall f. Bind f => f Unit -> f Unit` at Effect, stdout `again\nagain\n`,
    exit 0) and reader_dictionary_returns_the_concrete_result (the non-Effect
    regression, exit 42); tests::closure_protocol::
    abstract_callable_transport_emits_a_checked_adapter (the generated adapter
    and factory).
  Input boundary: source; executed Wasm component.
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace.
  Result: pass. The Effect discard, delayed-map, and fixed-payload programs
    run to completion with the exact output and status, and the non-Effect
    Reader regression still returns 42.
  Revision: 675f0e3 plus this slice.
  Gaps: none. Under-application of an indirect callee, including a dictionary
    method, is lowered by the same generated indirect call the PE-13 evidence
    records for [polymorphism and erasure](polymorphism-and-erasure.md).
```

## Vertical execution order

1. Audit trusted library identity, import signatures, entry selection, Core
   wrapper insertion, external lowering, and the component runner against the
   design.
2. Carry trusted Effect identities through the Core boundary; classify imports
   before erasure; build wrappers only from plans; verify the transformed Core.
3. Execute inertness, order, repeated-run, polymorphic, and cross-module cases
   through the normal component path with mandatory Wasmtime.
4. Record evidence and update D-04. Do/ado syntax and unrelated WASI services
   remain separately owned; do not claim them from Core fixtures.

## Evidence record and completion rule

For each ID record code paths/functions, exact tests/assertions, input
boundary, commands, Wasmtime version, actual executions/skips, revision, and
gaps. Distinguish source diagnostics, structural IR checks, and behavior
observed through execution. Runtime cases require `PSRS_REQUIRE_WASMTIME=1`; a
skip is not verification. Scoreboard runtime completion is not a golden output
comparison and cannot identify every host CLI failure; focused tests must
assert exact stdout/status or an explicit trap marker. After Rust edits
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
  Gaps: this historical evidence did not check that trusted Effect identity
    was passed explicitly; the old lowering recovered it from the qualified
    type name and opacity metadata. Same-name untrusted declarations and
    trusted re-exports need source-level coverage.
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
  Gaps: these cases did not include an ordinary closure-shaped import decoy.
    Import classification and wrapper timing require new tests.
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
  Gaps: the direct-reference rule is lexical. This record does not test a
    runner value passed from the selected entry to a helper; it must not be
    described as whole-program privilege isolation.
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
  Result: pass for cross-module Effect forwarding and delayed execution. The
    producer does not directly reference `runEffect`.
  Revision: 95aebe3 + uncommitted
  Gaps: direct lexical reference restriction is not capability isolation; a
    runner passed from the selected entry to a helper may be invoked there.
EF-10:
  Implementation: crates/psrs-backend/src/linking/mod.rs,
    crates/psrs-linker/src/compose.rs,
    crates/psrs-backend/src/wasm/lower/mod.rs (entry wrapper),
    crates/psrs-driver/src/tests/effects.rs (component execution)
  Tests: every executed test above runs the produced component under
    `wasmtime run`; bind_preserves_wasi_results_across_stdout_and_stderr
    observes both stdout and stderr; tests assert exit codes and a valid
    binary. tests/wasmtime_required.rs gates the runtime baseline.
  Input boundary: executed Wasm component
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
  Result: pass for the historical `Int` entry behavior under
    `PSRS_REQUIRE_WASMTIME=1`.
  Revision: 95aebe3 + uncommitted
  Gaps: this does not cover the generated `Effect Unit` adapter, normal zero
    exit, exactly-once execution, or trap propagation through that adapter.
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
  Gaps: these historical negatives stop at the effect-lowering Core check.
    They do not exercise semantic identity validation, import-plan consistency,
    generated wrapper structure, or verification of the complete transformed
    Core. `Type::Closure` remains the generic runtime representation.
    BE-21 remains the broader landing gate for this topic.
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
  Gaps: the current public signature is `Effect Unit` because that is the
    assertion and platform API the library needs; this is not forced by
    non-returning lowering. `assertThrows` needs to observe a trap from inside
    the guest, which the target profile does not provide
    ([DEC-05](../../decision/DEC-05-wasmtime-feature-set.md)), so the standard
    library omits it rather than approximating it.
```

## Discovered obligations

- The token is the compiler-owned opaque `TypeId::STATE_TOKEN` chosen by effect
  lowering. Its one value is the `StateToken` expression `run` supplies; the
  runtime shape is a scalar constant because the synchronous token carries no
  payload. Source programs cannot name the token; order comes from the calls and
  optimizer contracts. A later stateful token is an open question, not a second
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
