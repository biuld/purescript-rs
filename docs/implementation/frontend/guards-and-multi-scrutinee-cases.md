# Guards and Multi-Scrutinee Cases Acceptance

**Design:** [Resolved HIR Desugaring](../../design/frontend/semantics/desugaring.md)

**Roadmap:** [D-04 FE-06 and L2/L6](../../design/D-04-suite-roadmap.md)

**Status:** Partial. Local Wasmtime behavior and all twelve named P2 source
fixtures are verified. The named upstream runtime cases stop at P3 because this
checkout lacks their imported standard library modules, so official end-to-end
behavior is still unverified.

## Requirements and evidence

| ID | Requirement | Implementation and evidence | State |
| --- | --- | --- | --- |
| GU-01 | Adjacent function equations remain in source order, share one argument product, and preserve first-match behavior. Refutable integer patterns fall through through compiler-owned integer equality. | `psrs-ast/src/expr/guards.rs` lowers literals to `IntegerEqual`; P3 resolves that node directly to `Intrinsic::I32Eq`. `integer_pattern_equality_resolves_to_the_compiler_intrinsic` checks the resolved symbol; `integer_patterns_ignore_a_user_defined_equal_operator` executes result 10 even though the source alias for `==` always returns false; `integer_literal_function_equations_preserve_clause_order` executes result 11 for the first matching row. | Verified locally |
| GU-02 | Boolean guards in a comma list short-circuit; ordinary `let ... in ...` expressions are valid conditions and keep expression-local scope. A failed guard skips its body and continues with the next guarded clause. | P2 retains guard lists, P3 resolves them in order, P4 builds nested conditionals; `comma_guards_short_circuit_then_use_the_next_guarded_clause` executes fallback result 22. `ordinary_let_in_expression_is_supported_as_a_boolean_guard` executes 10, and `ordinary_let_in_guard_binders_do_not_escape_the_expression` checks that the local name is unavailable to later guards and the body. The equivalent `let next = n in condition next` condition compiled with `purs 0.15.16`. | Verified locally and by purs syntax/typecheck |
| GU-03 | Pattern guards bind names for later guards and the body, and a failed refutable pattern continues at the next guard or equation. | P3 scopes pattern-guard binders through the suffix; P4 emits a generated case with a fallthrough row; `case_alternative_pattern_guard_scopes_binders_and_falls_through` executes 43 and `failed_pattern_guard_falls_through_to_the_next_function_equation` executes 41. | Verified locally |
| GU-04 | Local `let` and guarded `where` declarations retain their lexical scope and written annotations; the issue-requested bare `let` guard qualifier binds through later guards and the result. | `resolve_local_bindings` resolves each annotation into HIR `Typed`. `issue_requested_naked_let_guard_binds_values_for_later_guards_and_the_body` executes 10; this qualifier is an implementation extension, not PureScript 0.15.16 syntax. `local_let_type_annotations_are_checked` and `guarded_where_type_annotations_are_checked` reach P5 and report `TypesDoNotUnify` for deliberately mismatched annotations. | Verified locally |
| GU-05 | Multiple case scrutinees are evaluated once, left to right, before pattern tests; alternatives retain top-to-bottom first-match and guard fallthrough. | P2 normalizes scrutinees to one ordered closed record product without a tuple library dependency; P4 binds it once. `multiple_case_scrutinees_preserve_order_and_guard_fallthrough` executes result 22 and asserts stdout `1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n`. | Verified locally |
| GU-06 | A guarded branch does not establish coverage unless one guard clause is statically unconditional. A guarded Boolean row is redundant when an earlier unconditional row already covers its pattern; guarded rows do not cover or make each other redundant. Generated fallback cases do not create user warnings. Partial guarded equations, Boolean cases, and incomplete Boolean products are rejected. | `CaseBranchCoverage::{Source, Guarded, Generated}` propagates through Core coverage. P4 proves unconditional guards from resolved true symbols over the whole program, including transparent aliases; an imported/re-exported false `otherwise` does not prove coverage. `guarded_rows_do_not_claim_unconditional_coverage` checks partial and exhaustive guards and asserts exactly one diagnostic for a partial guarded row. Core Boolean-matrix tests cover redundant guarded rows, non-covering guarded rows, and generated rows. `shadowed_boolean_case_guard_reports_its_source_row_and_keeps_first_match` asserts the exact redundant alternative span and executes result 11; `guarded_boolean_rows_do_not_shadow_each_other_or_change_fallthrough` executes result 22 after the first guard fails and checks for no false redundancy warning. Generated guard helper cases mark both the cloned source row and fallback as `Generated`; `guarded_helper_cases_do_not_emit_source_coverage_diagnostics` verifies no false partial or redundant warning. | Verified locally |
| GU-07 | Boolean case patterns preserve source order, including Boolean fields nested in constructors, multi-scrutinee products, and record pattern guards. | P4 preserves Boolean pattern structure for P5; Typed Core sends the checked patterns through the shared matrix. `nested_boolean_constructor_pattern_selects_the_matching_row` executes a nested `Flag true` pattern; `upstream_1185_multifield_constructor_and_boolean_pattern_select_the_name` executes the multi-field `Person name true` pattern and returns 85. `boolean_case_patterns_preserve_source_order`, `boolean_case_guards_preserve_alternative_fallthrough`, `multi_scrutinee_boolean_patterns_lower_and_prove_coverage`, and `nested_boolean_record_pattern_guard_matches_through_a_compiler_test` execute their selected values. The normalized-HIR verifier checks pattern trees recursively. | Verified locally |
| GU-08 | Local pattern declarations scope over following declarations only, and duplicate names in case binders are rejected, including `@` patterns. | P2 nests each pattern declaration's single-evaluation case around only later declarations. `local_pattern_binding_is_visible_to_following_declarations` executes 10; `local_pattern_binding_does_not_scope_over_earlier_declarations` matches `LetPatterns2` with `UnknownName`; `repeated_names_inside_a_case_named_pattern_are_rejected` matches `OverlappingBinders` with `OverlappingArgNames`. | Verified locally and by M2 scoreboard |

The focused source tests run with:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::guard -- --nocapture
```

All 31 guard-filtered tests passed with Wasmtime required; none were skipped. The
multi-case test passes ten effectful scrutinees directly in the source `case`,
then proves single evaluation and left-to-right order through guest stdout.
Guard tests assert selected fallback values. Ordinary `let ... in ...`
expressions in Boolean conditions are official expression-guard syntax. The
separate bare `| let name = value, ...` qualifier is tested as a project
extension only.

## Official-suite evidence and remaining work

The named upstream fixtures lower through P2, but their runtime scoreboards
cannot reach the changed P4/P5/P8 passes with the standard library in this
checkout. The test `official_guard_and_case_sources_lower_through_p2` reads the
vendored corpus by default and lowers all twelve issue fixtures without
unsupported guard or case forms:

`2787.purs`, `2806.purs`, `DctorName.purs`, `FunctionAndCaseGuards.purs`,
`TCO.purs`, `2795.purs`, `3114/VendoredVariant.purs`, `4357.purs`, `Guards.purs`,
`MultiArgFunctions.purs`, `CaseMultipleExpressions.purs`, and
`CaseInputWildcard.purs`.

| Command/filter | Measurement | Exact first blocker |
| --- | --- | --- |
| `PSRS_REQUIRE_WASMTIME=1` runtime scoreboard filtered separately for each of the 12 named fixtures | Runtime agreement 0/12; all twelve pass P2 and stop at P3. | `2787.purs`, `2806.purs`, `DctorName.purs`, `2795.purs`, `TCO.purs`, and `Guards.purs`: `Effect.Console`; `FunctionAndCaseGuards.purs` and `CaseInputWildcard.purs`: `Effect`; `3114/VendoredVariant.purs`: `Prim.Row`; `4357.purs`: `Data.Foldable`; `MultiArgFunctions.purs`: `Data.Function.Uncurried`; `CaseMultipleExpressions.purs`: `Partial.Unsafe`. |
| `PSRS_ORACLE=annotations PSRS_REQUIRE_WASMTIME=1 PSRS_SUITE_FILTER=Guards cargo test -p psrs-driver --test suite -- --ignored --nocapture` | Parse agreement 4/4 across two passing and two layout fixtures. Runtime agreement 0/2. | `FunctionAndCaseGuards.purs` stops at P3 on `Effect`; `Guards.purs` stops at P3 on `Effect.Console`. |
| `PSRS_REQUIRE_WASMTIME=1 PSRS_ORACLE=annotations PSRS_SUITE_FILTER=CaseMultipleExpressions cargo test -p psrs-driver --test suite runtime::l6_runtime_scoreboard -- --ignored --nocapture` | Runtime agreement 0/1. | `CaseMultipleExpressions.purs` stops at P3 on `Partial.Unsafe`; no Wasm case is skipped or claimed as executed. |

`official_guard_and_case_sources_lower_through_p2` reads the vendored corpus by
default and lowers all twelve issue fixtures. The equivalent ordinary
let-in expression guard compiles with `purs 0.15.16`; the local runtime test
checks its guard value and lexical scope. The bare cross-guard let qualifier is
not part of the official syntax. On 2026-10-02, the full annotations scoreboard
reported 904/908 parse agreement. L2 resolved 59/413 passing modules; 354 cases
were blocked (337 by missing library modules, 8 at P3, 5 at P2, and 4 at P0).
The required-runtime scoreboard agreed on 0/413 cases: 337 lacked library
modules, 55 stopped at P10 without a component entry point, and the remaining
cases stopped earlier. The twelve named cases above still account for 0
executed upstream runtime cases because they stop before type checking. The
upstream differential command
`PURESCRIPT_REPO=/Users/biu/Projects/purescript cargo test -p psrs-driver --test upstream -- --nocapture`
passed 4/4 selected type-system cases; that differential battery contains no
guard fixture. These current counts match the corresponding D-04 language and
runtime summaries; the guard-specific filtered results are listed above.

Remaining FE-06 work includes broader nested and array literal patterns and
running the named upstream runtime fixtures after the `Effect`,
`Effect.Console`, and `Partial.Unsafe` modules are available. Guarded Boolean
alternative redundancy and coverage provenance have focused source and matrix
evidence under PM-14.
This record does not mark those gaps as verified by the local runtime tests.
