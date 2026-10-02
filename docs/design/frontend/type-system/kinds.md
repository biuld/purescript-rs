# Kinds and Type Constructors

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [modules and resolution](../semantics/modules-and-resolution.md), [frontend boundaries](../00-ir-boundaries.md), and PureScript kind polymorphism.

**Summary:** P5 checks kind-correct type, class, data, newtype, and synonym declarations before term checking. Its checked constructor environment also carries representation roles used by the class solver for `Coercible`. The design follows PureScript's shared type-and-kind language, including polymorphic kinds, explicit kind application, type-level `Symbol` and `Int`, and rows.

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

Roles are ordered from most restrictive to most permissive: `Nominal < Representational < Phantom`. At a nominal parameter, coercion requires type equality; at a representational parameter, it requires a recursive `Coercible` proof; at a phantom parameter, the argument is irrelevant. A role annotation may make an inferred role more restrictive, never more permissive. Role metadata is keyed by resolved type identity in the checked kind environment, including imported declarations.

## Design

Register primitive kinds and declaration heads before checking bodies. Resolve kind signatures, infer missing parameter kinds, instantiate polymorphic kinds at uses, and skolemize expected `forall` kinds when checking annotations. Explicit kind applications select quantified kind arguments; ordinary type application consumes an arrow kind. Generalize undetermined kind variables at declaration boundaries according to the official compiler's scope rules. Check class heads against `Constraint`, value types against `Type`, row entries against their row parameter, and type-level literals against `Symbol` or `Int`.

An instance head is checked against its class's kind scheme. That is the only
place a standalone kind signature on a class is enforced: a class without one is
inferred as its parameters at `Type` returning `Constraint`, which accepts any
argument kind, so `class C :: Constraint -> Constraint` would otherwise accept
`instance C Int`. Each head argument is elaborated against the parameter kind its
class declares rather than read on its own, which is what makes `((->) r)` a
higher-kinded class parameter in `Functor ((->) r)` rather than the partially
applied function synonym it is elsewhere. A head that applies its class to the
wrong number of arguments is an arity error owned by the class environment, so
the kind check leaves it alone rather than reporting `KindsDoNotUnify` where
`ClassInstanceArityMismatch` is the official code.

Reject cyclic synonyms and unsaturated synonym use, including a partial synonym in a higher-kinded position. Ordinary data/newtype constructors may be partially applied when the expected kind allows it. Infer data/newtype roles from constructor fields to a fixed point across the resolved module graph, then check explicit annotations. Foreign data has no constructor fields from which to infer roles, so it is nominal by default; an explicit role annotation is its trusted interface contract. Keep source ranges on all kind uses and role annotations.

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
    infer_roles(program):
        initialize data/newtype parameters to Phantom and foreign parameters
            to Nominal unless an explicit role signature is present
        repeat over all modules until no inferred role becomes more restrictive:
            expand saturated type synonyms in every constructor field,
                substituting arguments with capture-avoiding renaming
            walk the expanded field using the current constructor-role map
            standalone free parameters are Representational
            nominal application arguments mark their free parameters Nominal
            representational arguments recurse; phantom arguments stop
            unknown application heads are walked and their arguments are Nominal
        validate role arity and reject annotations more permissive than inference
    zonk and return CheckedKindEnv
```

Inference traverses type and kind applications separately. Unification compares constructors by resolved identity and decomposes applications; `forall` comparison uses instantiation or skolemization according to direction. Before role walking, synonym applications are expanded with capture-avoiding substitution: aliases are transparent and cannot add nominality, including when an imported alias occurs inside a data or newtype field. Synonym cycles are rejected by kind checking; role analysis must also terminate conservatively if it is invoked while such an error is present. Role walking respects quantified binders, marks constraint arguments nominal, and consults the current fixed-point role map for known constructors. Missing role metadata on an unknown constructor is treated as nominal by the walker so an absent interface cannot make a coercion more permissive. Errors identify the smallest offending application, binder, or annotation.

## Code map

The kind-checking API accepts the resolved module set and returns a `CheckedKindEnv` containing both kind schemes and roles:

- `check_program(modules: &[ResolvedModule]) -> Result<CheckedKindEnv, Vec<KindDiagnostic>>` owns kind registration, synonym checks, role inference, and annotation validation.
- `infer_roles_fixed_point(modules, foreign_role_signatures) -> (RoleEnv, Vec<KindDiagnostic>)` walks constructor fields across modules and validates annotations.
- `CheckedKindEnv::role_of(TypeId) -> Option<&[Role]>` is the only role lookup consumed by class entailment; the solver does not reconstruct roles from surface syntax.
- HIR owns role annotation names and source ranges. The checked environment owns resolved identities and final roles.

Kind unification and role inference are separate submodules under the kind checker; callers pass the resulting environment onward instead of importing inference internals.

## Invariants and verification

Every checked type has one kind; every value signature ends at `Type`, every class constraint at `Constraint`, and every row at `Row k`. No unknown kind, escaped skolem, cyclic synonym, or partial synonym crosses into THIR. Every role vector has the declaration's parameter arity; data/newtype roles are no more permissive than their fields allow; foreign role signatures are preserved as trusted metadata; and imported types use the declaring module's roles. Verify role inference through nested and mutually recursive declarations, all three role levels, invalid weakening, foreign defaults/signatures, and source spans against official `purs` cases.

## Worked example

`Record (x :: Int | r)` has kind `Type` when `r :: Row Type`: the field `x` has kind `Type`, the extended row has kind `Row Type`, and `Record` consumes that row. A type-level label literal has kind `Symbol`; using it as the field's value type is a kind error. A synonym `type R a = { x :: a }` cannot be passed as a bare constructor where `Type -> Type` is expected until its parameter is supplied, matching PureScript's saturation rule.

## Boundaries and interfaces

P5 consumes normalized resolved HIR and emits a checked kind environment for term inference. THIR retains checked type structure and kind information needed to verify polymorphic uses, but no mutable kind solver state. Core lowering may erase kind abstractions after type checking.

## Open questions and future work

Full kind polymorphism, visible kind application, and additional primitive kinds remain future work. Role implementation coverage belongs in [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md) and the [roles and coercions acceptance record](../../../implementation/frontend/roles-and-coercions.md); `Coercible` constraint solving and evidence are specified in [classes and evidence](classes-and-evidence.md).

## References

- [PureScript `Types.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Types.hs), [`Kinds.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Kinds.hs), and [`Environment.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Environment.hs).
- [PureScript role inference](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Roles.hs).
- [PureScript synonym checking](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Synonyms.hs).

## Implementation notes

The current implementation builds the program-wide role table through
`check_roles(&[hir::Module])` before per-module kind checks. It expands known
synonyms before walking fields and uses nominal roles as a conservative
fallback if expansion encounters a cycle. Kind schemes remain private to each
kind-checking run; a unified public program kind environment remains the
code-map target.
