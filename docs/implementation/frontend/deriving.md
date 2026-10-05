# Deriving Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Deriving](../../design/frontend/type-system/deriving.md), with
[classes and evidence](../../design/frontend/type-system/classes-and-evidence.md),
[kinds](../../design/frontend/type-system/kinds.md), and
[primitives](../../design/frontend/type-system/prim.md).

**Progress:** The seven deriving handoff items are implemented, with expanded
source, differential, and runtime evidence. The shared usage tree now drives
all mapping families and parameter-bearing record fields. Generic round trips,
Traversable/Bitraversable, function Contravariant adapters, and polymorphic
newtype adapters execute. Whole-workspace validation retains the unrelated
failures listed below; FE-22 remains partial for corpus mismatches in the
shared instance-declaration checks rather than a missing structural rule.

## Acceptance matrix

| ID | Requirement | Required evidence | State |
| --- | --- | --- | --- |
| DR-01 | AST/HIR retain the derivation strategy and distinguish `derive instance` from `derive newtype instance`. | AST lowering and HIR derivation field; source and runtime cases for both strategies. | Verified |
| DR-02 | A rule is selected by resolved class identity: a re-exported class keeps its rule, and an unrelated same-name class does not gain one. | Differential cases `deriving-recognizes-the-reexported-data-eq-identity`, `deriving-custom-prelude-eq-is-not-a-known-class`, `deriving-user-eq-is-not-selected-by-unqualified-name`. | Verified for covered cases |
| DR-03 | One deriving registry, read by `TypeId`, selects rules; no rule compares a class module or name at use time. | `DerivingRegistry::build` maps resolved class identities and their method symbols once; `known_class`/`method` read it by `TypeId`. No deriving rule, and no wildcard check, compares a module or name at use time. | Verified |
| DR-04 | A structural head is a locally declared data or newtype constructor applied at the class-appropriate arity, and a newtype head is a newtype. | Source cases `derives_newtype_for_a_partially_applied_type_constructor`, `rejects_newtype_deriving_for_a_data_declaration`, `derives_a_higher_kinded_newtype_instance_through_the_wrapped_type`; differential newtype cases. | Verified for covered cases |
| DR-05 | Every structural rule synonym-expands field types before deciding how a field is used. | Every implemented family (`Eq`, `Ord`, `Functor`, `Bifunctor`, `Contravariant`) normalizes its field types before the usage test; the alias case is exercised for `Functor` (`derives_nested_functor_mapping_through_an_imported_dictionary`). Runtime `eq_and_ord_expand_applied_variable_aliases_and_use_higher_kinded_dictionaries` checks both alias expansion and Eq1/Ord1 field dispatch. | Verified for covered cases |
| DR-06 | Field usage and variance are validated against the visible instance environment, and an unmappable field is `CannotDeriveInvalidConstructorArg` at its span. | `usage/` walks every field with polarity and instance existence; `deriving_failures_report_their_official_error_codes` covers a contravariant field and a field head with no mapping instance. | Verified for covered cases |
| DR-07 | Generated members are ordinary expressions: inference elaborates the method at the instance head and selects each field's dictionary. | Runtime cases execute derived methods through ordinary dictionaries. | Verified |
| DR-08 | Structural `Eq` compares constructor tags and fields and recurses through the instance dictionary. | Runtime `imported_structural_eq_instance_executes_constructor_and_field_comparisons`, `derives_recursive_eq_through_its_instance_dictionary`; differential `deriving-structural-eq`. | Verified |
| DR-09 | `derive Eq1`/`Ord1` delegate to their monomorphic counterpart, and a structural `Eq`/`Ord` field of type `f a` is compared through `eq1`/`compare1`. | Source `derives_eq1_and_ord1_by_delegating_to_the_monomorphic_method`; Runtime `eq_and_ord_expand_applied_variable_aliases_and_use_higher_kinded_dictionaries` executes the applied-variable field, derived Eq1/Ord1 dictionaries, and unequal/order cases. | Verified for covered cases |
| DR-10 | Structural `Ord` orders constructors by declaration order and fields lexicographically. | Runtime `derived_ord_executes_constructor_and_lexicographic_field_order`; differential `deriving-structural-ord`. | Verified |
| DR-11 | `Functor` maps nested applications and function results, and rejects a parameter under a function input. | Runtime `derives_nested_functor_mapping_through_an_imported_dictionary`; differential function-result and nested-function cases and `deriving-functor-rejects-contravariant-field`. | Verified |
| DR-12 | `Bifunctor` maps the final two parameters and rejects a negative parameter position. | Runtime `derives_bifunctor_mapping_for_both_type_parameters`; differential `deriving-bifunctor-final-two-parameters` and `deriving-bifunctor-rejects-negative-parameter`. | Verified |
| DR-13 | `Contravariant` maps function inputs through the `Profunctor.lcmap` dictionary and rejects a positive parameter. | Differential `deriving-contravariant-function-input` and `deriving-contravariant-rejects-positive-parameter`; runtime `function_contravariant_deriving_executes_its_adapter` and its record-field counterpart check both predicate outcomes and retained inert fields. | Verified |
| DR-14 | `derive newtype` selects the wrapped class dictionary and adapts each method by a representation cast authorized by the checked newtype and that dictionary. It does not prove ordinary `Coercible`, which still refuses to lift through an unknown constructor. Quantifiers on the method stay in scope across the cast. | Runtime `imported_newtype_derived_dictionary_executes_its_coercion_adapter`, `derives_a_higher_kinded_newtype_instance_through_the_wrapped_type`, `newtype_deriving_reuses_a_dictionary_under_an_unknown_constructor`; source `newtype_deriving_reuses_the_dictionary_under_an_unknown_type_constructor` and `ordinary_coercible_does_not_lift_through_an_unknown_type_constructor`; differential empty-class cases; source `derives_newtype_methods_from_the_wrapped_instance`. | Verified for covered cases |
| DR-15 | A rank-1 method `forall` is instantiated once and shared across the adapter's two heads. | Source `newtype_deriving_accepts_a_polymorphic_class_method`; runtime `polymorphic_newtype_deriving_executes_its_adapter` executes the same method at Boolean and Array argument instantiations. | Verified |
| DR-16 | A derivable `Newtype`/`Generic` wildcard is resolved before the instance is recorded. | `record_instance` resolves the wildcard through the registry and stores the concrete wrapped/representation type in the recorded head; a non-wildcard final argument is `ExpectedWildcard`. `derives_newtype_class_for_a_newtype_with_a_wildcard` covers the resolved path. | Verified |
| DR-17 | `Generic` derives the `Data.Generic.Rep` representation and its `to`/`from` methods. | Source `derives_generic_representation_for_a_data_type`; Runtime `generic_deriving_round_trips_constructor_tags_and_fields` checks nullary, unary, and multi-field constructors. | Verified for covered cases |
| DR-18 | `Profunctor`, `Foldable`, `Bifoldable`, `Traversable`, and `Bitraversable` have their structural rules. | Source `derives_profunctor_through_a_contravariant_field`, `derives_foldable_for_single_field_constructors`, `derives_bifoldable_for_a_two_parameter_type`, `derives_traversable_for_single_field_constructors`, `derives_bitraversable_for_a_two_parameter_type`; runtime `derived_foldable_executes_through_its_instances`. Runtime modules `adapters`, `folds`, `records`, and `traversals` execute every listed class, both fold directions, sequencing, multi-field effects, and preserved inert fields. `differential_remaining_deriving_rules_against_purs` checks every remaining family, record fields, rejection polarity, given dictionaries, and scoped forall parameters. | Verified for covered cases |
| DR-19 | Each deriving failure carries its official `errorCode` (`CannotDerive`, `ExpectedTypeConstructor`, `InvalidDerivedInstance`, `InvalidNewtypeInstance`, `CannotDeriveNewtypeForData`, `CannotDeriveInvalidConstructorArg`, `CannotFindDerivingType`, `ExpectedWildcard`). | `deriving_failures_report_their_official_error_codes` asserts `CannotDerive`, `CannotDeriveNewtypeForData`, `CannotDeriveInvalidConstructorArg`, `InvalidNewtypeInstance`, and `ExpectedWildcard`; `differential_deriving_error_codes_against_purs` confirms the code agrees with `purs` on rejected cases. `fold_deriving_reports_missing_core_values_at_the_declaration` covers `CannotFindDerivingType` for both fold classes: multiple contributing fields require `append`, and constructors without contributions require `mempty`. `deriving_rejects_non_constructor_heads_and_invalid_class_arity` directly checks `ExpectedTypeConstructor` and `InvalidDerivedInstance`, also confirmed by the differential diagnostic battery. The common recorder reports `ClassInstanceArityMismatch` before deriving when the declaration does not supply one argument per class parameter. | Verified for covered conditions |
| DR-20 | Source behavior agrees with official PureScript across the deriving surface, including diagnostics. | Differential batteries cover 18 original rules, 17 remaining-family/context/scoped-parameter cases, and six rejected diagnostic cases; a separate case agrees with purs on record postfix precedence. | Verified for covered cases |

## Evidence and commands

Source-only cases are in `crates/psrs-driver/src/tests/deriving/` (18 tests).
Value-level execution is in
`crates/psrs-driver/src/tests/wasi/classes/deriving/` (25 tests), with observable
assertions for every structural class. The official differential battery is
`crates/psrs-driver/tests/upstream/deriving/` (four Rust tests).

```sh
cargo fmt --all --check
cargo test -p psrs-driver --lib tests::deriving
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi::classes::deriving
PURESCRIPT_REPO=/Users/biu/Projects/purescript cargo test -p psrs-driver --test upstream deriving
PSRS_REQUIRE_WASMTIME=1 cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
PSRS_ORACLE=annotations cargo test -p psrs-driver --test suite types::l4_l5_scoreboard_with_annotations -- --ignored --nocapture
```

The fold diagnostic regression reproduced a silently discarded multi-field
instance before the fix. Both fold classes now report `CannotFindDerivingType`
at the derive declaration for required missing `append` or `mempty`; a single
contribution requires neither an unnecessary append nor an unused folding
method. Generated mapping, fold, traversal, and Generic terms use the shared
syntax builder.

Traversal exposed two shared backend representation boundaries. Aggregate
layouts and callable signatures now converge together rather than publishing
partially normalized product identities. Bare polymorphic function slots use
a registered unary erased calling protocol; erasure and recovery adapt every
argument and result instead of casting a concrete callable to a consumer
signature. The independent `erased_function_slots_preserve_partial_application_and_captures`
case checks flattened multi-argument functions and captured partial
applications. The existing 33-case generic aggregate battery also executes.
Record projection/update precedence agrees with the official parser; the
nested aggregate fixture explicitly parenthesizes the call result it projects.

Generic representation literals compare by value in inference evidence and
Core's invariant type relation. THIR instance context parameters compare by
semantic equality, retaining rejection of an actually different dictionary.
Core lowering consumes that verified check; P8 uses the shared Core type
relation for alpha-equivalent lambda parameter types. A derived higher-kinded
Traversable context has source differential and runtime execution evidence.

The trusted core-library path also executes `Generic` to/from round trips,
Traversable traverse/sequence, and Bitraversable bitraverse/bisequence with
`Maybe` effects and record fields. `identity` retains its defining
`Control.Category` symbol even when re-exported by `Data.Function`.
`Data.Tuple` supplies its ordinary derived `Functor (Tuple a)` instance,
required by `Data.Traversable`'s existing Tuple instance. The standard-library
loader's trusted-order regression passes.

Validation on 2026-10-06 used Wasmtime 49.0.2 and purs 0.15.16. The 18 source
tests, 25 required-runtime deriving tests, and four differential batteries pass.
The backend (393), Core (79), THIR (25), and typechecker (134) unit tests pass.
Formatting, the local diff check, and changed Markdown link checks pass.

The full required-runtime workspace run retains three failing targets:
`psrs-cli --test source_layout` (the pre-existing 510-line `partial.rs`),
`psrs-driver --test prim_row` (the pre-existing nested Union deferred execution
case), and the driver library. The newly required Tuple dependency first
exposed two isolated source fixtures that omitted `Data.Functor`; those tests
now load the real trusted library graph and both execute. After that test-only
correction, the full driver library rerun reports **581 passed / 10 failed**,
with exactly the handoff's ten pre-existing failures.

Default workspace Clippy fails on the handoff's pre-existing
`unnecessary_to_owned`, `too_many_arguments`, and `result_large_err` findings.
The workspace check with the handoff's known lint categories allowed passes
(including its test-only `field_reassign_with_default`, `manual_contains`, and
`dead_code` allowances). This does not establish a clean default Clippy gate.
No L6/M7 corpus runtime measurement was run; the 25 execution cases are focused
acceptance evidence and are not a replacement for that scoreboard.

## Known boundaries

The L5 scoreboard includes unrelated shared class-rule mismatches, notably
orphan checks and invalid record/synonym instance heads. The annotated L4/L5
run on 2026-10-06 measured L4 39/50 and L5 75/97,
with 11 cases blocked before a mapped diagnostic. L5 decomposes into
`OverlappingInstances` 8/8, `NoInstanceFound` 46/53, `MissingClassMember` 2/2,
`InvalidNewtypeInstance` 6/6, `InvalidInstanceHead` 1/7,
`DuplicateInstance` 1/1, `ClassInstanceArityMismatch` 4/4,
`CannotDeriveInvalidConstructorArg` 7/7, `PossiblyInfiniteInstance` 0/1,
`OrphanInstance` 0/7, and `DuplicateTypeClass` 0/1. Unsupported mapped open rows
remain explicit diagnostics rather than
empty usage results. Structural rule availability is a visible head-identity
check, matching official deriving; ordinary inference owns complete instance
constraint selection.

## Completion rule

Do not mark the deriving topic complete until DR-01..DR-20 have implementation
and source-level acceptance/rejection evidence, the single registry and the
field-usage analysis replace the scattered identity and shape checks, every
deriving failure reports its official `errorCode`, and every runtime requirement
executes with `PSRS_REQUIRE_WASMTIME=1`. The official differential set must then
include diagnostic-code agreement and the remaining structural classes. Run the
workspace test, format, and Clippy gates after code changes.
