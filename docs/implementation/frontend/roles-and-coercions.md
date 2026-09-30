# Roles and Coercible Acceptance

**Feature:** [FE-16](../../design/D-04-suite-roadmap.md#frontend-feature-matrix)

**Design:** [Kinds and type constructors](../../design/frontend/type-system/kinds.md), [classes and evidence](../../design/frontend/type-system/classes-and-evidence.md), and [polymorphism and erasure](../../design/backend/fp/polymorphism-and-erasure.md).

**Progress:** Partial. Role inference/checking and a bounded, role-aware
`Coercible` path are implemented through source, Typed Core, and CC. Deriving
and broader official coercion interactions remain incomplete.

## Scope

This record tracks source role declarations, checked role metadata, primitive
`Coercible` solving, evidence validation, and the backend conversion boundary.
It does not claim that deriving or all official PureScript coercion rules are
implemented. Roles are checked by the kind layer; the class solver consumes
the checked environment; backend value adaptation follows DEC-07 and the
polymorphism-and-erasure design.

## Acceptance matrix

| ID | Requirement | Evidence | Status |
| --- | --- | --- | --- |
| RC-01 | Role declarations preserve names, roles, and spans from CST through AST/HIR. Orphan, duplicate, and unsupported declarations receive source diagnostics. | AST lowering and HIR role fields; driver source tests for role diagnostics. | Verified |
| RC-02 | Data/newtype roles are inferred to a fixed point across the resolved module set. Standalone occurrences are representational, nominal contexts restrict free variables, phantom contexts stop traversal, synonyms are expanded capture-safely, and unknown application heads are conservative. | `psrs_kind::tests::infers_representational_phantom_and_nominal_roles`, `expands_type_synonyms_inside_data_fields_before_inferring_roles`, and `synonym_expansion_is_capture_avoiding_and_cycle_safe`; imported-alias data-field source and Wasmtime regressions; three-module runtime test for `Layer (Box Age)` to `Layer (Box Int)`. | Verified for covered cases |
| RC-03 | An explicit role signature has the declaration's arity and cannot be more permissive than inferred roles. Imported role metadata is used at a consuming call site. | `explicit_roles_may_restrict_but_not_weaken_inferred_roles`, `checks_foreign_role_arity_from_the_declared_kind`, source tests `rejects_a_role_annotation_that_weakens_inference` and `imported_role_metadata_restricts_coercion`. | Verified |
| RC-04 | Foreign parameters default to nominal; an explicit foreign role declaration supplies trusted interface metadata. | `unannotated_foreign_roles_are_conservative`; `checks_foreign_role_arity_from_the_declared_kind`. | Verified |
| RC-05 | `Prim.Coerce.Coercible` is compiler-owned and cannot be extended by user instances. The solver uses role metadata, transitively connected given constraints, equalities, row structure, and visible newtype unwrapping. | `rejects_user_defined_coercible_instances`, `composes_given_coercible_constraints_transitively`, `composes_imported_coercible_givens_in_a_generic_function_when_wasmtime_is_available`, and the `coercible-transitive-givens` official differential case. | Verified for covered rules |
| RC-06 | Newtype unwrapping requires its constructor to be visible. Nested visible newtypes and imported constructors are supported; visible nominal newtypes unwrap before parameter roles are compared. | `an_imported_newtype_requires_its_constructor_for_unwrapping`; Wasmtime tests `unwraps_nested_visible_newtypes_when_wasmtime_is_available`, `coerces_through_an_imported_visible_newtype_when_wasmtime_is_available`, and `unwraps_both_sides_before_applying_a_nominal_newtype_role_when_wasmtime_is_available`. | Verified for covered cases |
| RC-07 | Checked coercion evidence retains source/target types through THIR and Core; malformed evidence cannot authorize a different boundary. | THIR `verifier_rejects_coercion_evidence_for_a_different_boundary`; Core verifier checks cast source and target against its value and result. | Verified |
| RC-08 | Lowering uses the existing typed value-conversion protocol, including nested ADT fields, arrays, and function adapters; a source proof never becomes an arbitrary Wasm reference cast. | Wasmtime tests cover coercing parameterized newtypes to scalar, function, and array payloads; visible nested newtypes; phantom sums with constructor-tag preservation; representational data fields and imported aliases; imported generic constrained functions; and cross-module given transitivity. | Verified for listed shapes |
| RC-09 | FE-16 deriving generates checked instance evidence and handles newtype deriving rules. | No implementation or acceptance evidence yet. | Unverified |

## Source and runtime coverage

The driver has 15 focused source tests for role inference and declaration
diagnostics, representational and phantom coercion, local and imported nominal
rejection, weakened roles, hidden newtype constructors, imported aliases in
data fields, transitive givens, and compiler-owned evidence. Fourteen
value-sensitive Wasmtime tests execute the corresponding conversions, including
parameterized newtype payloads represented as scalars, functions, and arrays;
a multi-constructor phantom value verifies its tag survives coercion. Required
runtime command:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver tests::wasi::coercion
```

The compiler-provided value shim follows upstream's `Safe.Coerce.coerce`
source API; `Prim.Coerce.Coercible` remains the compiler-owned class. The
durable `differential_role_and_coercible_rules_against_purs` test compares 12
fixtures with `purs 0.15.16` from `/Users/biu/Projects/purescript`: local and
imported nominal data rejection, inferred representational data and phantom
acceptance, visible nominal-newtype unwrapping, hidden-newtype rejection,
weakened-role rejection, imported alias fields, transitive givens, and
parameterized newtype scalar, function, and array payloads. The passing run is
retained at `/tmp/psrs-fe16-purs-differential-final.log`; the value-sensitive
Wasmtime run is retained at `/tmp/psrs-fe16-wasi-coercion-final.log`.

## Known boundaries

The current solver is deliberately conservative for unknown heads and bounded
recursive decomposition. It does not yet implement all upstream higher-kinded
constraint rewriting, kind unification during coercion, canonical open-row
alignment, or all recursive given interactions. These cases remain unverified;
the feature stays Partial until their source diagnostics and runtime-safe
lowering are covered. `derive` and generated instances are also unimplemented.

## Completion rule

Do not mark FE-16 complete until RC-01..RC-09 have implementation and
source-level acceptance/rejection evidence, every runtime requirement executes
with `PSRS_REQUIRE_WASMTIME=1`, and the official compatibility set includes the
remaining coercion solver cases and deriving behavior. Run the workspace test,
format, and Clippy gates after code changes.
