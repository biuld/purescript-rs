# Kinds and Type Constructors

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [modules and resolution](../semantics/modules-and-resolution.md), [frontend boundaries](../00-ir-boundaries.md), and PureScript kind polymorphism.

**Summary:** P5 checks kind-correct type, class, data, newtype, and synonym declarations before term checking. The design follows PureScript's shared type-and-kind language, including polymorphic kinds, explicit kind application, type-level `Symbol` and `Int`, and rows.

## Scope

This document owns kind inference, kind signatures, constructor and synonym legality, and type roles. [Type inference](type-inference.md) owns term checking; [rows](rows-and-records.md) owns row equations; [classes](classes-and-evidence.md) owns constraint evidence.

## Background

PureScript kinds are types: `Type`, `Constraint`, `Symbol`, `Row Type`, and higher-kinded arrows are written in the same type language. Primitive constructors include `Row`, `Record`, `Function`, and `Array`; the primitive modules also provide `RowList` and type-level `Int` operations. A polymorphic constructor may quantify a kind variable and accept an explicit kind application. `Symbol` is the kind of type-level strings, while type-level integer literals have kind `Int`. Kind inference therefore needs quantification and subsumption, beyond first-order HM kind unification.

## Model

```text
KindExpr = Constructor(Type | Constraint | Symbol | Int | Row | RowList | ...)
         | Variable(KindVarId) | Application(KindExpr, KindExpr)
         | ForAll(KindVarId, KindExpr) | Unknown(InferKindId)
TypeExpr = Constructor | Variable | Application | KindApplication
         | ForAll | Constrained | RowEmpty | RowExtend | TypeLevelString
         | TypeLevelInt | KindAnnotation | Skolem | Wildcard
CheckedTypeConstructor = { id, kind_scheme, arity, roles, origin }
```

An arrow is application of the function kind constructor. `Row k` admits entries of kind `k`; `Record` consumes `Row Type` and produces `Type`. Class constraints have kind `Constraint`. Kind unknowns and skolems remain private to P5. A declared synonym is a saturated, acyclic abbreviation, never a nominal constructor. Roles of data/newtype parameters constrain representational coercion; they do not change ordinary type equality.

## Design

Register primitive kinds and declaration heads before checking bodies. Resolve kind signatures, infer missing parameter kinds, instantiate polymorphic kinds at uses, and skolemize expected `forall` kinds when checking annotations. Explicit kind applications select quantified kind arguments; ordinary type application consumes an arrow kind. Generalize undetermined kind variables at declaration boundaries according to the official compiler's scope rules. Check class heads against `Constraint`, value types against `Type`, row entries against their row parameter, and type-level literals against `Symbol` or `Int`.

Reject cyclic synonyms and unsaturated synonym use, including a partial synonym in a higher-kinded position. Ordinary data/newtype constructors may be partially applied when the expected kind allows it. Infer and check parameter roles for `Coercible` compatibility. Keep source ranges on all kind uses and annotations.

A `Type | Row(Type) | Arrow` enum is rejected because it cannot express `Symbol`, polymorphic kinds, or explicit kind application. Eager synonym expansion before cycle checking is rejected because it may diverge.

## Algorithms

```text
check_kinds(program):
    register primitive environment, declaration heads, and kind signatures
    reject synonym cycles and invalid kind-signature dependencies
    for each declaration dependency component:
        allocate unknown kinds for unannotated parameters
        infer applications, quantified kinds, rows, literals, and constraints
        check annotations by kind subsumption and skolem escape checks
        solve kind equations with occurs checks
        generalize permitted kind variables; check roles and synonym saturation
    zonk and return CheckedKindEnv
```

Inference traverses type and kind applications separately. Unification compares constructors by resolved identity and decomposes applications; `forall` comparison uses instantiation or skolemization according to direction. Errors identify the smallest offending application, binder, or annotation.

## Code map

`crates/psrs-kind/src/` owns `kind.rs` (checked kinds and schemes), `check/infer.rs` (private unknowns, unification, subsumption), `check/synonyms.rs` (dependency and saturation checks), and `check/roles.rs` (role checks). Its entry point is `check_program(hir: &Program) -> Result<CheckedKindEnv, Vec<Diagnostic>>`. Resolved type IDs and ranges come from HIR; the type checker consumes only the checked environment.

## Invariants and verification

Every checked type has one kind; every value signature ends at `Type`, every class constraint at `Constraint`, and every row at `Row k`. No unknown kind, escaped skolem, cyclic synonym, or partial synonym crosses into THIR. Verify polymorphic kind instantiation, explicit kind applications, `Symbol` and type-level `Int`, row kinds, role checks, and diagnostic spans against official `purs` cases.

## Worked example

`Record (x :: Int | r)` has kind `Type` when `r :: Row Type`: the field `x` has kind `Type`, the extended row has kind `Row Type`, and `Record` consumes that row. A type-level label literal has kind `Symbol`; using it as the field's value type is a kind error. A synonym `type R a = { x :: a }` cannot be passed as a bare constructor where `Type -> Type` is expected until its parameter is supplied, matching PureScript's saturation rule.

## Boundaries and interfaces

P5 consumes normalized resolved HIR and emits a checked kind environment for term inference. THIR retains checked type structure and kind information needed to verify polymorphic uses, but no mutable kind solver state. Core lowering may erase kind abstractions after type checking.

## Open questions and future work

Track official compiler changes to role inference, visible kind application, and primitive kind declarations as compatibility tests; implementation coverage belongs in [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md).

## References

- [PureScript `Types.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Types.hs), [`Kinds.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Kinds.hs), and [`Environment.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Environment.hs).
- [PureScript synonym checking](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Synonyms.hs).
