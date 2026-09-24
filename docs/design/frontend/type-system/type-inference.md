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

Inference unknowns have levels and substitutions private to P5. Quantified variables and skolems have distinct identities and scopes. `Effect a` is an ordinary imported type application for P5: no compiler-native effect row, handler, or special inference rule is introduced.

## Design

Infer synthesizable expressions and check expressions with expected types. Instantiate `forall` and solve constrained uses through class entailment. When checking a signature or higher-rank argument, skolemize expected quantifiers, perform structural subsumption, and check that skolems do not escape. Function parameter comparison is contravariant and result comparison covariant; record subsumption compares common labels and checks closed-row extras and omissions. Evidence can be inserted at elaboration sites, while comparison under a type constructor cannot invent term-level dictionaries.

Infer a recursive SCC with shared placeholders, respecting explicit signatures, then solve and generalize only variables permitted by the environment and remaining constraints. Use kind-correct constructor and pattern types; type-check case alternatives, literals, arrays, record operations, newtypes, and foreign imports. Explicit type/kind applications and typed holes follow the official source rules. Build THIR only after zonking, ambiguity checks, and evidence elaboration.

Rejected alternatives: pure HM cannot check higher-rank signatures; unifying a `forall` as though it were a monotype is unsound; generalizing recursive uses before group checking admits unsound polymorphic recursion; and carrying solver cells into THIR breaks the P5 boundary.

## Algorithms

```text
check(expr, expected):
    if expected begins with forall: skolemize binder, check body, reject escape
    otherwise synthesize expr and subsume synthesized type against expected

subsume(actual, expected):
    instantiate leading forall in actual
    skolemize leading forall in expected
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

## Code map

`crates/psrs-typecheck/src/typecheck/` owns `infer.rs`, `unify.rs`, `subsumption.rs`, `skolems.rs`, and `groups.rs`; `classes.rs` and `rows.rs` provide the adjacent solvers. `typecheck(program: &hir::Program, kinds: &CheckedKindEnv) -> Result<thir::Program, Vec<Diagnostic>>` is the P5 entry point. `crates/psrs-thir/src/` owns checked types, expressions, evidence, and `verify_module(&Module) -> Result<(), Vec<Diagnostic>>`.

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
