# Type Inference and THIR

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [kinds](kinds.md), [modules and resolution](../semantics/modules-and-resolution.md), type schemes, subsumption, and skolemization.

**Summary:** P5 infers ordinary expressions and checks annotated polymorphic expressions using PureScript's quantified and constrained types. It performs higher-rank subsumption, row unification, and class entailment, then emits verified THIR with checked types and explicit evidence.

## Scope

This document owns schemes, instantiation, generalization, bidirectional checking, signatures, term and binder typing, subsumption, and THIR. [Kinds](kinds.md) owns kind legality; [rows](rows-and-records.md) owns row equations; [classes](classes-and-evidence.md) owns class entailment and evidence. Runtime erasure belongs to the backend.

## Background

HM inference explains unannotated let polymorphism, but PureScript additionally supports `forall` beneath arrows, constrained types, explicit type application, and row-polymorphic records. A use of a polymorphic value instantiates its `forall`; checking against an expected `forall` skolemizes it and rejects escaping skolems. Subsumption handles function variance and inserts dictionary evidence at permitted expression boundaries. Recursive declarations are checked as dependency groups; signatures supply polymorphic recursion where accepted by the source language.

## Model

```text
CheckedType = Var | Constructor | Application | KindApplication | ForAll
            | Constrained | RowEmpty | RowExtend | TypeLevelString
            | TypeLevelInt | Skolem
InferType = CheckedType + Unknown(InferVarId) + Wildcard
Scheme = { quantified_type_and_kind_vars, constraints, body }
THIR = { declarations, typed expressions, typed binders,
         explicit type and dictionary evidence, source_ranges }
```

Inference unknowns have levels and substitutions private to P5. Quantified variables and skolems have distinct identities and scopes. `Effect a` is an ordinary imported type application for P5: no compiler-native effect row, handler, private constructor, or special inference rule is introduced, and unification does not identify it with a function. Its runtime closure is produced later by one representation lowering ([effects](../../backend/fp/effects.md)). THIR keeps this same spine as `CheckedType`: an arrow is `Application(Application(Constructor(Function), a), b)`, a record is `Application(Constructor(Record), row)`, and Core carries the same representation ([DEC-15](../../../decision/DEC-15-unified-type-representation.md)).

## Design

Infer synthesizable expressions and check expressions with expected types. Instantiate `forall` and solve constrained uses through class entailment. When checking a signature or higher-rank argument, skolemize expected quantifiers, perform structural subsumption, and check that skolems do not escape. Function parameter comparison is contravariant and result comparison covariant; record subsumption compares common labels and checks closed-row extras and omissions. Evidence can be inserted at elaboration sites, while comparison under a type constructor cannot invent term-level dictionaries.

Infer a recursive SCC with shared placeholders, respecting explicit signatures, then solve and generalize only variables permitted by the environment and remaining constraints. Use kind-correct constructor and pattern types; type-check case alternatives, literals, arrays, record operations, newtypes, and foreign imports. Explicit type/kind applications and typed holes follow the official source rules. Build THIR only after zonking, ambiguity checks, and evidence elaboration.

Preserve `ForAll` beneath arrows and inside fields during signature
elaboration. A lambda parameter with a quantified expected type enters the
local environment as a polymorphic value; each reference opens only its
leading quantifiers with fresh unknowns. Constructor patterns and record
projections retain the same field quantifiers. Checking a record literal or
constructor argument propagates the expected field type to its value, rather
than first inferring all values as monotypes.

Checking a leading `forall` allocates a fresh skolem scope and checks the body
with rigid representatives of its binders. Substitutions of unknowns that
outlive that scope must not contain its skolems. Restore quantified binders in
the checked result after the escape audit; do not export solver skolems as free
Core variables. Generalization excludes locally bound variables and never
captures a skolem. Substitution below `ForAll` respects binder shadowing and
uses fresh identities to avoid capture.

Higher-rank checking does not imply arbitrary impredicative inference. When
an expected type is an unconstrained unknown, instantiate an inferred
polymorphic expression before solving the unknown, following the official
checker's rule. Explicit polymorphic fields and annotations supply the
boundaries at which a polymorphic value may be retained.

Rejected alternatives: pure HM cannot check higher-rank signatures; unifying a `forall` as though it were a monotype is unsound; generalizing recursive uses before group checking admits unsound polymorphic recursion; and carrying solver cells into THIR breaks the P5 boundary.

## Algorithms

```text
check(expr, expected):
    if expected begins with forall: skolemize binder, check body, reject escape
    if expected begins with constraint: bind given evidence, check body
    if expr is lambda and expected is arrow: bind parameter, check result
    if expr is record: check each field against its expected field type
    otherwise synthesize expr and subsume synthesized type against expected

infer_application(function, argument):
    synthesize function; instantiate only its leading quantifiers/constraints
    if its arrow is known: check argument against the parameter type
    otherwise infer a monomorphic argument and solve an arrow with fresh result

subsume(actual, expected):
    if expected begins with forall:
        enter a fresh scope; skolemize expected; recurse and reject escape
    if actual begins with forall:
        instantiate its leading binders inside the current comparison scope
    elaborate actual constraints where expression evidence can be inserted
    compare arrows contravariantly/covariantly and records by row rules
    otherwise unify kind-correct monotypes

infer_group(group, environment):
    allocate placeholders; expose declared signatures at recursive uses
    infer/check all bodies and patterns; solve row and class obligations
    reject ambiguous or escaping variables; generalize permitted unknowns
    zonk types and emit typed declarations with explicit evidence
```

Occurs checks traverse applications, rows, quantifiers, and constraints. Type errors retain both expected and actual types and the originating source range.
Comparison-local unknowns may refer to the comparison's skolems; unknowns
created outside that scope may not. This distinction accepts two equivalent
quantified signatures without allowing an inferred monomorphic parameter to
escape into a higher-rank signature.

## Code map

`crates/psrs-typecheck/src/typecheck/` owns `infer/` for synthesis and expected
type propagation, `signature.rs` for scoped signature elaboration, `unify.rs`
for equality, and a focused `rank_n/` module for subsumption, quantified
instantiation, and skolem scopes. Dependency-group checking and generalization
respect the same scope contract. `classes/` and `rows.rs` provide the adjacent
solvers. The semantic entry consumes HIR and checked kinds and produces THIR.
`crates/psrs-thir/src/` owns checked types, expressions, evidence, and
`verify_module(&Module) -> Result<(), Vec<VerifyError>>`.

The higher-rank owner provides `check_expr(expr, expected)`,
`subsume(actual, expected, evidence_mode)`, and scoped
`instantiate_forall`/`skolemize_forall` operations. Evidence mode distinguishes
expression boundaries that can insert dictionaries from structural comparison
inside a type. Both THIR and Core retain `ForAll` with bound identities and
body references, including at local binder and field types.

## Invariants and verification

Every THIR expression and binder has a kind-valid type; every reference is resolved; every required dictionary is supplied or abstracted; and no unknown, wildcard, or escaping skolem remains. Quantified variables have valid scope. Accept and reject cases cover rank-N arguments, subsumption direction, explicit type application, recursive signatures, constrained polymorphism, rows, ADTs, and ambiguous constraints; compare them with official `purs`.

## Worked example

`apply :: (forall a. a -> a) -> Int` requires an argument polymorphic at the call site. `apply (\x -> x)` checks the lambda against a skolemized `forall a. a -> a`; a monomorphic `Int -> Int` argument fails. By contrast, `let id = \x -> x in id id` generalizes `id` and instantiates its two uses independently.

## Boundaries and interfaces

P5 consumes normalized HIR and a checked kind environment. It emits verified THIR for [Core lowering](../semantics/core-lowering.md). No inference substitutions, class search state, or runtime layouts cross the boundary. WIT-bound foreign declarations receive ordinary checked PureScript types; ABI validation occurs later.

## Open questions and future work

Track official behavior for partial signatures, visible type applications, and ambiguity/defaulting in executable compatibility cases. [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md) records implementation coverage, not changes to this semantic target.

## References

- [PureScript type checker](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Types.hs), [subsumption](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Subsumption.hs), and [skolems](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Skolems.hs).
- [Kinds](kinds.md), [rows](rows-and-records.md), and [classes](classes-and-evidence.md).
