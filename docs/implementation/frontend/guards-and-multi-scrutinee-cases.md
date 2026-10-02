# Guards and Multi-Scrutinee Cases Acceptance

**Design:** [Resolved HIR Desugaring](../../design/frontend/semantics/desugaring.md)

**Roadmap:** [D-04 FE-06 and L2/L6](../../design/D-04-suite-roadmap.md)

**Status:** In progress. Local Wasmtime behavior is verified. The named upstream
runtime cases stop at P3 because this checkout lacks their imported standard
library modules, so official end-to-end behavior is still unverified.

## Requirements and evidence

| ID | Requirement | Implementation and evidence | State |
| --- | --- | --- | --- |
| GU-01 | Adjacent function equations remain in source order, share one argument product, and preserve first-match behavior. Refutable integer patterns fall through through equality guards. | `psrs-ast/src/equations.rs`; `integer_literal_function_equations_preserve_clause_order` executes result 11 for the first matching row. | Verified locally |
| GU-02 | Boolean guards in a comma list short-circuit; a failed guard skips its body and continues with the next guarded clause. | P2 retains guard lists, P3 resolves them in order, P4 builds nested conditionals; `comma_guards_short_circuit_then_use_the_next_guarded_clause` executes fallback result 22. | Verified locally |
| GU-03 | Pattern guards bind names for later guards and the body, and a failed refutable pattern continues at the next guard or equation. | P3 scopes pattern-guard binders through the suffix; P4 emits a generated case with a fallthrough row; `case_alternative_pattern_guard_scopes_binders_and_falls_through` executes 43 and `failed_pattern_guard_falls_through_to_the_next_function_equation` executes 41. | Verified locally |
| GU-04 | Let guards bind declarations for subsequent guards and the result; `where` bindings cover the guarded clauses in their source scope. | Resolver and P4 use explicit scoped bindings; `let_guards_bind_values_for_later_guards_and_the_body` executes 10. | Verified locally |
| GU-05 | Multiple case scrutinees are evaluated once, left to right, before pattern tests; alternatives retain top-to-bottom first-match and guard fallthrough. | P2 normalizes scrutinees to one anonymous record; P4 binds that record once. `multiple_case_scrutinees_preserve_order_and_guard_fallthrough` executes result 22 and asserts stdout `left\nright\n`. | Verified locally |
| GU-06 | A guarded branch does not establish coverage unless one guard clause is statically unconditional. Generated fallback cases do not create user warnings. Partial guarded equations and Boolean cases are rejected. | `CaseBranchCoverage::{Source, Guarded, Generated}` propagates to backend coverage analysis. `guarded_rows_do_not_claim_unconditional_coverage` checks partial and exhaustive guards plus a Boolean case whose only `true` path has a failing guard. | Verified locally |
| GU-07 | Boolean case patterns preserve source order, and Boolean guards on case alternatives fall through to the next source row. | P4 binds the scrutinee once and emits ordered conditionals; `boolean_case_patterns_preserve_source_order` executes 22 and `boolean_case_guards_preserve_alternative_fallthrough` executes 22. | Verified locally |
| GU-08 | Local pattern declarations scope over following declarations only, and duplicate names in case binders are rejected, including `@` patterns. | P2 nests each pattern declaration's single-evaluation case around only later declarations. `local_pattern_binding_is_visible_to_following_declarations` executes 10; `local_pattern_binding_does_not_scope_over_earlier_declarations` matches `LetPatterns2` with `UnknownName`; `repeated_names_inside_a_case_named_pattern_are_rejected` matches `OverlappingBinders` with `OverlappingArgNames`. | Verified locally and by M2 scoreboard |

The focused source tests run with:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::guards -- --nocapture
```

All 13 tests passed with Wasmtime required; none were skipped. The multi-case
test proves one evaluation per scrutinee and left-to-right order through its
guest stdout, and the guard tests assert selected fallback values.

## Official-suite evidence and remaining work

The named upstream fixtures parse, but their runtime scoreboards cannot reach
the changed P4/P5/P8 passes with the standard library in this checkout:

| Command/filter | Measurement | Exact blocker |
| --- | --- | --- |
| `PSRS_ORACLE=annotations PSRS_SUITE_FILTER=Guards cargo test -p psrs-driver --test suite -- --ignored --nocapture` | Parse agreement 4/4 across two passing and two layout fixtures. Runtime agreement 0/2. | `passing/FunctionAndCaseGuards.purs` cannot resolve `Effect`; `passing/Guards.purs` cannot resolve `Effect.Console`. |
| `PSRS_REQUIRE_WASMTIME=1 PSRS_SUITE_FILTER=Guards cargo test -p psrs-driver --test suite runtime::l6_runtime_scoreboard -- --ignored --nocapture` | Runtime agreement 0/2. | Both cases stop at P3 with the missing modules above; no Wasm case is skipped or claimed as executed. |
| `PSRS_REQUIRE_WASMTIME=1 PSRS_SUITE_FILTER=CaseMultipleExpressions cargo test -p psrs-driver --test suite runtime::l6_runtime_scoreboard -- --ignored --nocapture` | Runtime agreement 0/1. | `passing/CaseMultipleExpressions.purs` stops at P3 because `Partial.Unsafe` is missing. |

The full annotations scoreboard measured L1 parse agreement at 904/908, M2
failing agreement at 56/70, 52/413 passing modules resolved, 265 module-not-found
blockers, and 85 P2 surface-lowering blockers. The three guard-related runtime
fixtures above account for 0 executed upstream runtime cases because they stop
before type checking. The upstream differential command
`PURESCRIPT_REPO=/Users/biu/Projects/purescript cargo test -p psrs-driver --test upstream -- --nocapture`
passed 4/4 selected type-system cases; that differential battery contains no
guard fixture. These measurements and their decompositions are recorded in D-04.

Remaining FE-06 work includes broader nested and array literal patterns,
redundancy warnings for Boolean cases lowered to conditionals, and running the
named upstream runtime fixtures after the `Effect`, `Effect.Console`, and
`Partial.Unsafe` modules are available. This record does not mark those gaps as
verified by the local runtime tests.
