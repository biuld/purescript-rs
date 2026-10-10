# Roles and Coercible Acceptance

**Feature:** [FE-16](../../design/D-04-suite-roadmap.md#frontend-feature-matrix)

**Design:** [Kinds and type constructors](../../design/frontend/type-system/kinds.md), [classes and evidence](../../design/frontend/type-system/classes-and-evidence.md), [primitives](../../design/frontend/type-system/prim.md), and [polymorphism and erasure](../../design/backend/fp/polymorphism-and-erasure.md).

**Progress:** Partial. Role inference/checking and the covered role-aware
`Coercible` rules are implemented through source, Typed Core, and CC. Deriving is
a distinct topic: its selected rules, coverage, and requirement IDs are recorded
in the [deriving acceptance record](deriving.md). Rank-1 method `forall`
signatures and scoped method-local constraints are checked and instantiated
independently at use sites, including quantifiers that shadow class parameters.
The roles and `Coercible` acceptance evidence and remaining runtime limits are
recorded below.

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
| RC-05 | `Prim.Coerce.Coercible` is compiler-owned and cannot be extended by user instances. The solver uses checked kinds and roles, explicit given-proof composition, canonical role-aware rewriting, open-row alignment, and visible newtype unwrapping. | Source tests `rejects_user_defined_coercible_instances`, `composes_given_coercible_constraints_transitively`, `rewrites_canonical_given_constraints_through_representational_roles`, `rewrites_a_canonical_given_in_a_higher_kinded_application_head`, `aligns_canonical_open_record_rows_before_proving_field_coercions`, `rejects_open_record_rows_with_different_known_labels`, `does_not_rewrite_a_noncanonical_recursive_given`, `interacts_canonical_givens_with_a_shared_left_variable`, and the 20-case `differential_role_and_coercible_rules_against_purs`. | Verified for covered rules |
| RC-06 | Newtype unwrapping requires its constructor to be visible. Nested visible newtypes and imported constructors are supported; visible nominal newtypes unwrap before parameter roles are compared. | `an_imported_newtype_requires_its_constructor_for_unwrapping`; Wasmtime tests `unwraps_nested_visible_newtypes_when_wasmtime_is_available`, `coerces_through_an_imported_visible_newtype_when_wasmtime_is_available`, and `unwraps_both_sides_before_applying_a_nominal_newtype_role_when_wasmtime_is_available`. | Verified for covered cases |
| RC-07 | Checked coercion evidence retains source/target types through THIR and Core; malformed evidence cannot authorize a different boundary. | THIR `verifier_rejects_coercion_evidence_for_a_different_boundary`; Core verifier checks cast source and target against its value and result. | Verified |
| RC-08 | Lowering uses the existing typed value-conversion protocol, including nested ADT fields, arrays, and function adapters; a source proof never becomes an arbitrary Wasm reference cast. | Wasmtime tests cover coercing parameterized newtypes to scalar, function, and array payloads; visible nested newtypes; phantom sums with constructor-tag preservation; representational data fields and imported aliases; imported generic constrained functions; and cross-module given transitivity. | Verified for listed shapes |
| RC-09 | Deriving is its own topic with requirement IDs DR-01..DR-20; this record does not own its rules or coverage. | See the [deriving acceptance record](deriving.md). | Moved |

## Source and runtime coverage

Source tests cover role inference and declaration diagnostics, representational
and phantom coercion, local and imported nominal rejection, weakened roles,
hidden newtype constructors, imported aliases in data fields, higher-kinded
given rewriting, kind mismatch, open-row alignment, and recursive given
interactions. Value-sensitive Wasmtime tests execute the conversions, including
parameterized newtype payloads represented as scalars, functions, and arrays;
a multi-constructor phantom value verifies its tag survives coercion. Derived
instance tests are owned by the [deriving acceptance record](deriving.md).
Focused runtime commands:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib tests::wasi::coercion
```

Deriving runtime commands are in the [deriving acceptance record](deriving.md).

The compiler-provided value shim follows upstream's `Safe.Coerce.coerce`
source API; `Prim.Coerce.Coercible` remains the compiler-owned class, and
`Unsafe.Coerce.unsafeCoerce` is the compiler-owned intrinsic that `coerce` is
defined from upstream. A focused execution case
(`tests::coercion::unsafe_coerce_is_a_compiler_primitive_identity_cast`) lowers a
newtype through `unsafeCoerce` and runs to its value under mandatory Wasmtime. The
durable `differential_role_and_coercible_rules_against_purs` test compares 20
accepted and rejected fixtures with `purs 0.15.16`, including role
decomposition, higher-kinded given rewriting, checked kind compatibility,
open-row label alignment, and recursive-given behavior. The deriving
differential battery is recorded in the
[deriving acceptance record](deriving.md).

## Known boundaries

The solver implements higher-kinded application-head rewriting, checked kind
compatibility, role-aware canonical given interactions, and aligned open rows
for the covered source cases. Its kind reading is private to the coercion module
and disagrees with the kind checker's on `Row` and `Record`, and its entry is a
call-site special case rather than the primitive rule table the [primitives
design](../../design/frontend/type-system/prim.md) specifies; both are recorded
there as the deviations they are. Open-row values still have no runtime layout in
the current CC path, while closed reordered records execute through Wasmtime.
Class-method `forall` and scoped method-local constraints are supported, with
evidence in the [rank-N acceptance record](rank-n.md). Deriving boundaries,
including its variance checks, `Eq1`/`Ord1`, and the remaining upstream classes,
are owned by the [deriving acceptance record](deriving.md). FE-16 remains Partial
until the remaining official coercion cases have source and runtime evidence.

## Completion rule

Do not mark FE-16 complete until RC-01..RC-08 have implementation and
source-level acceptance/rejection evidence, every runtime requirement executes
with `PSRS_REQUIRE_WASMTIME=1`, and the official compatibility set includes the
remaining coercion solver cases. Deriving has its own completion rule in the
[deriving acceptance record](deriving.md). Run the workspace test, format, and
Clippy gates after code changes.
