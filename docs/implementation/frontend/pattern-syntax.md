# Pattern Syntax Acceptance

**Design:** [AST lowering](../../design/frontend/syntax/ast-lowering.md)
**Issue:** #85

This record covers source pattern acceptance through P2 and the frontend
diagnostics attached to repeated binders. Typed Core and runtime pattern
execution are owned by the linked design and backend acceptance records.

## Requirements

| ID | Requirement | Evidence | Status |
| --- | --- | --- | --- |
| PS-01 | Every fixed issue-85 baseline pattern source and the named passing fixtures lower through lexing, layout, parsing, and AST construction without loading imports. | `psrs-driver/tests/patterns.rs::issue_85_p2_pattern_corpus_lowers_to_ast_without_loading_imports` (34 fixed baseline paths plus named fixtures; 38 unique source files) | Verified |
| PS-02 | AST construction preserves literal, array, named, typed, nested, and multi-field record patterns; typed binders retain their type names and source spans. | `psrs-driver/tests/patterns.rs::p2_ast_keeps_literal_array_named_typed_nested_and_multifield_patterns`; `::p2_ast_retains_type_wildcards_inside_typed_patterns`; corpus matrix below | Verified |
| PS-03 | Repeated argument binders report the annotated `OverlappingArgNames` or `OverlappingNamesInLet` code at the second binder's source span. | `psrs-driver/tests/patterns.rs::annotated_pattern_failures_match_their_official_error_codes`; `::lambda_array_and_as_pattern_duplicates_report_overlapping_arg_names` | Verified |
| PS-04 | A pattern guard may repeat a binder; the first occurrence determines its type and value. | `psrs-driver/tests/patterns.rs::pattern_guard_keeps_duplicate_binders_and_checks_the_first_binding_type`; `psrs-driver/tests/pattern_matrix_execution.rs::pattern_guard_duplicate_binding_selects_the_first_constructor_field` (41) | Verified |

## Corpus Matrix

The issue-85 baseline was measured on 2026-10-02 at clean revision `5298aad`
with `PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test suite
runtime::l6_runtime_scoreboard -- --ignored --nocapture`. It reported 0/413
runtime matches and 61 P2 blockers. The 34-path fixed pattern set is maintained
in `ISSUE_85_P2_BASELINE`; issue-named passing cases and focused shape cases
extend the P2 acceptance test to 38 unique sources. The final L6 run reports
five P2 blockers, all outside that 34-path pattern set. The other 22 P2 exits
from the baseline are additional recoveries in type, kind, instance, and
declaration forms; they are recorded separately from the 34 pattern blockers.

| Pattern shape | Source evidence |
| --- | --- |
| Array and nested constructor/array patterns | `tests/upstream/passing/1991.purs`, `BindersInFunctions.purs`, `EmptyDataDecls.purs`, `Patterns.purs`, `TopLevelCase.purs` |
| Integer, character, Boolean, and negative literal patterns | `tests/upstream/passing/IntAndChar.purs`, `NegativeBinder.purs`, `Let2.purs`, `Patterns.purs`, `ReservedWords.purs` |
| Named patterns | `tests/upstream/passing/2049.purs`, `LetPattern.purs`, `NamedPatterns.purs`, `Patterns.purs` |
| Typed patterns and typed lambda binders | `tests/upstream/passing/1570.purs`, `1664.purs`, `ParensInTypedBinder.purs`, `TypedBinders.purs` |
| Constructor containing a record pattern | `tests/upstream/passing/Rank2Object.purs` |
| Multi-field constructor patterns | `tests/upstream/passing/1185.purs`; value-sensitive execution is `psrs-driver/tests/pattern_matrix_execution.rs::upstream_1185_multifield_constructor_and_boolean_pattern_select_the_name` (85) |
| Nested named record under a constructor pattern | `tests/upstream/passing/2049.purs`; value-sensitive execution is `psrs-driver/tests/pattern_matrix_execution.rs::upstream_2049_nested_named_record_pattern_returns_the_selected_record_value` (84) |
| Multi-field record patterns | `tests/upstream/passing/2049.purs`, `LetPattern.purs`, `Patterns.purs`, `Stream.purs` |
| Nested multi-field record with named, array, and literal subpatterns | [local source fixture](../../../crates/psrs-driver/tests/fixtures/patterns/nested_records.purs) |
| Repeated argument binders and local declaration groups | `tests/upstream/failing/OverlappingArguments.purs`, `OverlappingBinders.purs`, `DuplicateDeclarationsInLet.purs`, `DuplicateDeclarationsInLet2.purs`, `DuplicateDeclarationsInLet3.purs` |
| Pattern guard with repeated name and first-binding type | [local source fixture](../../../crates/psrs-driver/tests/fixtures/patterns/guard_first_wins.purs) |

The official `purs 0.15.16` accepts the repeated name in the pattern-guard
fixture. Its two `Pair` fields have different types, so checking the function's
declared `Int` result verifies that the first `x` supplies the binding. Ordinary
lambda and case patterns with the same repeated names are rejected under
`OverlappingArgNames`; the separate as-pattern rule is covered by the named
lambda diagnostic fixture.

The upstream `1185.purs` and `2049.purs` programs only print `Done`, so the two
runtime fixtures retain their pattern shapes and replace `main` with an
observable numeric result. The first selects `Person name true` and confirms
the selected string; the second calls the upstream `f` over a `Cons` value and
checks both `x` and the aliased record's `y` field.
