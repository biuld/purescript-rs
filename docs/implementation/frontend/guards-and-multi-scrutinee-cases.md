# Guards and Multi-Scrutinee Cases Acceptance

**Design:** [Resolved HIR Desugaring](../../design/frontend/semantics/desugaring.md)

**Roadmap:** [D-04 FE-06 and L2/L6](../../design/D-04-suite-roadmap.md)

**Status:** In progress. Local Wasmtime behavior and all named P2 source
fixtures are verified. The named upstream runtime cases stop at P3 because this
checkout lacks their imported standard library modules, so official end-to-end
behavior is still unverified.

## Requirements and evidence

| ID | Requirement | Implementation and evidence | State |
| --- | --- | --- | --- |
| GU-01 | Adjacent function equations remain in source order, share one argument product, and preserve first-match behavior. Refutable integer patterns fall through through compiler-owned integer equality. | `psrs-ast/src/expr/guards.rs` lowers literals to `IntegerEqual`; P3 resolves that node directly to `Intrinsic::I32Eq`. `integer_pattern_equality_resolves_to_the_compiler_intrinsic` checks the resolved symbol; `integer_literal_function_equations_preserve_clause_order` executes result 11 for the first matching row. | Verified locally |
| GU-02 | Boolean guards in a comma list short-circuit; a failed guard skips its body and continues with the next guarded clause. | P2 retains guard lists, P3 resolves them in order, P4 builds nested conditionals; `comma_guards_short_circuit_then_use_the_next_guarded_clause` executes fallback result 22. | Verified locally |
| GU-03 | Pattern guards bind names for later guards and the body, and a failed refutable pattern continues at the next guard or equation. | P3 scopes pattern-guard binders through the suffix; P4 emits a generated case with a fallthrough row; `case_alternative_pattern_guard_scopes_binders_and_falls_through` executes 43 and `failed_pattern_guard_falls_through_to_the_next_function_equation` executes 41. | Verified locally |
| GU-04 | Let-bound and guarded `where` declarations retain their lexical scope and written annotations; P5 checks those annotations. | `resolve_local_bindings` resolves each annotation into HIR `Typed`. `let_guards_bind_values_for_later_guards_and_the_body` executes 10; `local_let_type_annotations_are_checked` and `guarded_where_type_annotations_are_checked` each reach P5 and report `TypesDoNotUnify` for a deliberately mismatched annotation. | Verified locally |
| GU-05 | Multiple case scrutinees are evaluated once, left to right, before pattern tests; alternatives retain top-to-bottom first-match and guard fallthrough. | P2 normalizes scrutinees to one ordered closed record product without a tuple library dependency; P4 binds it once. `multiple_case_scrutinees_preserve_order_and_guard_fallthrough` executes result 22 and asserts stdout `1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n`. | Verified locally |
| GU-06 | A guarded branch does not establish coverage unless one guard clause is statically unconditional. Generated fallback cases do not create user warnings. Partial guarded equations, Boolean cases, and incomplete Boolean products are rejected. | `CaseBranchCoverage::{Source, Guarded, Generated}` propagates through Core coverage. P4 proves unconditional guards from resolved true symbols over the whole program, including transparent aliases; an imported/re-exported false `otherwise` does not prove coverage. `guarded_rows_do_not_claim_unconditional_coverage` checks partial and exhaustive guards and asserts exactly one diagnostic for a partial guarded row; nested Boolean generated helpers use `Generated` coverage. | Verified locally |
| GU-07 | Boolean case patterns preserve source order, including Boolean fields in multi-scrutinee products and record pattern guards. | P4 binds each case input once and emits ordered tests/conditionals; `boolean_case_patterns_preserve_source_order`, `boolean_case_guards_preserve_alternative_fallthrough`, `multi_scrutinee_boolean_patterns_lower_and_prove_coverage`, and `nested_boolean_record_pattern_guard_matches_through_a_compiler_test` execute the selected values. The normalized-HIR verifier checks pattern trees recursively; Boolean patterns nested under unsupported constructor forms produce P4 `UnsupportedSource`. | Verified locally |
| GU-08 | Local pattern declarations scope over following declarations only, and duplicate names in case binders are rejected, including `@` patterns. | P2 nests each pattern declaration's single-evaluation case around only later declarations. `local_pattern_binding_is_visible_to_following_declarations` executes 10; `local_pattern_binding_does_not_scope_over_earlier_declarations` matches `LetPatterns2` with `UnknownName`; `repeated_names_inside_a_case_named_pattern_are_rejected` matches `OverlappingBinders` with `OverlappingArgNames`. | Verified locally and by M2 scoreboard |

The focused source tests run with:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::guards -- --nocapture
```

All 25 guard tests passed with Wasmtime required; none were skipped. The
multi-case test proves one evaluation per source scrutinee and left-to-right
order through guest stdout, and guard tests assert selected fallback values.

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

| Command/filter | Measurement | Exact blocker |
| --- | --- | --- |
| `PSRS_ORACLE=annotations PSRS_SUITE_FILTER=Guards cargo test -p psrs-driver --test suite -- --ignored --nocapture` | Parse agreement 4/4 across two passing and two layout fixtures. Runtime agreement 0/2. | `passing/FunctionAndCaseGuards.purs` cannot resolve `Effect`; `passing/Guards.purs` cannot resolve `Effect.Console`. |
| `PSRS_REQUIRE_WASMTIME=1 PSRS_SUITE_FILTER=Guards cargo test -p psrs-driver --test suite runtime::l6_runtime_scoreboard -- --ignored --nocapture` | Runtime agreement 0/2. | Both cases stop at P3 with the missing modules above; no Wasm case is skipped or claimed as executed. |
| `PSRS_REQUIRE_WASMTIME=1 PSRS_SUITE_FILTER=CaseMultipleExpressions cargo test -p psrs-driver --test suite runtime::l6_runtime_scoreboard -- --ignored --nocapture` | Runtime agreement 0/1. | `passing/CaseMultipleExpressions.purs` stops at P3 because `Partial.Unsafe` is missing. |

The current vendored annotations and runtime measurements are recorded in
D-04 below. The three guard-related runtime fixtures above account for 0
executed upstream runtime cases because they stop before type checking. The
upstream differential command
`PURESCRIPT_REPO=/Users/biu/Projects/purescript cargo test -p psrs-driver --test upstream -- --nocapture`
passed 4/4 selected type-system cases; that differential battery contains no
guard fixture. These measurements and their decompositions are recorded in D-04.

Remaining FE-06 work includes Boolean patterns nested under constructor
arguments, broader nested and array literal patterns, redundancy warnings for
Boolean cases lowered to conditionals, and running the named upstream runtime
fixtures after the `Effect`, `Effect.Console`, and `Partial.Unsafe` modules are
available. A constructor-contained Boolean pattern is rejected at P4 as
`UnsupportedSource`; it is not allowed to reach type checking as invalid HIR.
This record does not mark those gaps as verified by the local runtime tests.
