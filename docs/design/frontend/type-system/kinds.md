# Kinds and Type Constructors

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [modules and resolution](../semantics/modules-and-resolution.md), [frontend boundaries](../00-ir-boundaries.md), and PureScript kind polymorphism.

**Summary:** P5 checks kind-correct type, class, data, newtype, and synonym declarations before term checking. Its checked constructor environment also carries representation roles used by the class solver for `Coercible`. The design follows PureScript's shared type-and-kind language, including polymorphic kinds, explicit kind application, type-level `Symbol` and `Int`, and rows. Because kinds are types, one denotation, one primitive kind table, and one kind solver own every kind in the compiler: declaration annotations, instance heads, expression-level type applications, and the kind of every type unknown all resolve through them. The checked environment those operations produce is program-level, and an imported declaration's kind is the one its declaring module checked.

## Scope

This document owns kind inference, kind signatures, constructor and synonym legality, and type roles. It also owns the only denotation of a type expression read as a kind, the only primitive constructor kind table, the only kind unifier, the kind recorded for every type unknown, and the checked program-wide kind environment that kind inference and role inference produce together. [Type inference](type-inference.md) owns term checking and consumes those operations; [rows](rows-and-records.md) owns row equations; [classes](classes-and-evidence.md) owns constraint evidence; [primitives](prim.md) owns the `Prim.*` members and their rules, whose kinds are checked here from the registry exactly as a source declaration's are.

A feature module may not keep a private kind table, a private translation from types to kinds, or a private kind compatibility test. Whether a type operation checks kinds must not depend on which module the caller entered through; the kind of a type is one fact with one owner.

## Background

PureScript kinds are types: `Type`, `Constraint`, `Symbol`, `Row Type`, and higher-kinded arrows are written in the same type language. Primitive constructors include `Row`, `Record`, `Function`, and `Array`; the primitive modules also provide `RowList` and type-level `Int` operations. A polymorphic constructor may quantify a kind variable and accept an explicit kind application. `Symbol` is the kind of type-level strings, while type-level integer literals have kind `Int`. Kind inference therefore needs quantification and subsumption, beyond first-order HM kind unification.

## Model

```text
TypeExpr = Constructor | Variable | Application | KindApplication
         | ForAll | Constrained | RowEmpty | RowExtend | TypeLevelString
         | TypeLevelInt | KindAnnotation | Skolem | Wildcard

Kind = Builtin(BuiltinType) | Named(TypeId)    # a constructor read as a kind
     | App(Kind, Kind) | Function(Kind, Kind) | Variable(KindVarId)
KindScheme = { quantified_kind_vars, kind }
CheckedKindEnv = { kind_schemes: TypeId -> KindScheme, roles: TypeId -> [Role],
                   declaring_module: TypeId -> ModuleId }
KindDiagnostic = { code, span, message, origin: ModuleId }
CheckedTypeConstructor = { id, kind_scheme, arity, roles, origin }
```

An arrow is application of the function kind constructor. `Row k` admits entries of kind `k`; `Record` consumes `Row Type` and produces `Type`. Class constraints have kind `Constraint`. Kind unknowns and skolems remain private to P5. A declared synonym is a saturated, acyclic abbreviation, never a nominal constructor. Roles of data/newtype parameters constrain representational coercion; they do not change ordinary type equality.

Kinds are types, so a kind is a `TypeExpr` read in a kind position and the two grammars cannot drift apart. Official PureScript resolves a kind the same way: `elaborateKind` looks a constructor up and returns the constructor itself. There is therefore no dedicated kind constant: `Builtin(Type)`, `Builtin(Constraint)`, and `Builtin(Symbol)` are the primitive kinds, `Builtin(Int)` is the kind of a type-level integer, and `Builtin(Row)` is the row kind constructor. One table gives every primitive constructor its kind.

```text
Type | Constraint | Symbol                     :: Type
Row                                          :: Type -> Type
Record                                       :: Row Type -> Type
Function                                     :: Type -> Type -> Type
Array                                        :: Type -> Type
Int | Number | Boolean | String | Char | Unit :: Type
```

The denotation then has no special case at all: a primitive constructor denotes itself, a user declaration denotes its resolved identity, and application, function, `forall`, and type-level literals follow the same spine. `Row k` is `App(Builtin(Row), k)`, a type-level string literal has kind `Builtin(Symbol)`, and a type-level integer literal has kind `Builtin(Int)`. A private head or a reserved constant for one primitive is rejected, because it makes the same kind expression denote two different things depending on which module reads it, and the two readings do not unify.

Roles are ordered from most restrictive to most permissive: `Nominal < Representational < Phantom`. At a nominal parameter, coercion requires type equality; at a representational parameter, it requires a recursive `Coercible` proof; at a phantom parameter, the argument is irrelevant. A role annotation may make an inferred role more restrictive, never more permissive. Role metadata is keyed by resolved type identity in the checked kind environment, including imported declarations.

## Design

Register primitive kinds and declaration heads before checking bodies. Resolve kind signatures, infer missing parameter kinds, instantiate polymorphic kinds at uses, and skolemize expected `forall` kinds when checking annotations. Explicit kind applications select quantified kind arguments; ordinary type application consumes an arrow kind. Check class heads against `Constraint`, value types against `Type`, row entries against their row parameter, and type-level literals against `Symbol` or `Int`.

A declaration's kind scheme quantifies the kind unknowns its own definition leaves undetermined and keeps the kinds that definition determines. `data Tree m = Tree (m Tree)` determines `m`'s kind through its own field, so generalizing `m` instead of inferring it loses the occurs check that rejects the declaration. Quantification must also be well scoped: an implicitly generalized kind variable that mentions a type variable requires that type variable to be quantified explicitly, which is the official `QuantificationCheckFailureInKind` case.

Kind checking runs once per program and produces one environment. `check_program` registers the primitive table and every declaration head, checks synonyms and cycles, infers and generalizes declaration kinds, infers roles to a fixed point, and returns the environment together with its diagnostics. Each diagnostic records the module that declares the offending type, so a conflict in module A is reported against A's source even though B's use exposed it. A per-module kind check consumes the imported schemes from that environment and re-checks only the module's own declarations, annotations, and instance heads; it never re-derives an imported declaration's kind.

Absence of kind metadata is an error, not freedom. A referenced declaration with no checked scheme means the program is not closed over its type declarations, and official PureScript reports the same way: `elaborateKind` raises `UnknownName` for a constructor it cannot resolve instead of inventing a variable. Fabricating a fresh kind variable for a missing imported type turns absent interface metadata into an inferable parameter, and a later use of that type is then accepted for an unknown reason. A lenient or partial-program mode may still continue past a missing scheme, but it must request that behaviour explicitly and record the affected declaration as unverified.

Every type unknown carries a kind, and binding one maintains it. Kind inference has a single substitution over kinds, one occurs check, and one escape rule, and `bind_type_variable` runs the kind compatibility check for both sides of a binding in addition to the occurs, level, and skolem-escape rules. A record's row has kind `Row Type`, a row-polymorphic variable has kind `Row k` with `k` tracked alongside it, and a `forall` binder's kind annotation is checked against the kind recorded for that variable. Coercion checking therefore adds no kind machinery of its own: it consults this solver, and a binding that would violate a kind invariant is a kind diagnostic rather than a silently accepted type.

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

A `Type | Row(Type) | Arrow` enum is rejected because it cannot express `Symbol`, polymorphic kinds, or explicit kind application. Eager synonym expansion before cycle checking is rejected because it may diverge. Two kind checkers, one per module, are rejected because a program-level conflict found by one run is not guaranteed to be found again by the other, so the diagnostic that matters depends on which entry point the driver chose.

## Algorithms

```text
check_program(modules):
    register the primitive kind table, every declaration head, and kind signatures
    reject synonym cycles and invalid kind-signature dependencies
    for each declaration dependency component:
        allocate unknown kinds for unannotated parameters
        infer applications, quantified kinds, rows, literals, and constraints
        check annotations by kind subsumption and skolem escape checks
        solve kind equations with occurs checks
        quantify the kind variables the definition leaves undetermined and
            reject a quantification that is not well scoped
        check roles and synonym saturation
    infer_roles(modules):
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
    zonk and return (CheckedKindEnv, Vec<KindDiagnostic>) with each diagnostic
        attributed to its declaring module

check_module(module, imported: CheckedKindEnv, local: bool):
    reuse the checked scheme of every referenced imported declaration
    re-check only this module's declarations, annotations, and instance heads
    reject a referenced declaration that has no checked scheme unless the
        caller explicitly asked for lenient partial-program checking
    return this module's diagnostics, each attributed to it

bind_type_variable(variable, ty, span):          # the inference-side rule
    occurs check and skolem-escape check on ty
    adjust the levels of ty to the variable's level
    unify kind_of(variable) with kind_of(ty) through the one kind substitution
    report KindsDoNotUnify at the offending application when they differ
```

Inference traverses type and kind applications separately. Unification compares constructors by resolved identity and decomposes applications; `forall` comparison uses instantiation or skolemization according to direction. Before role walking, synonym applications are expanded with capture-avoiding substitution: aliases are transparent and cannot add nominality, including when an imported alias occurs inside a data or newtype field. Synonym cycles are rejected by kind checking; role analysis must also terminate conservatively if it is invoked while such an error is present. Role walking respects quantified binders, marks constraint arguments nominal, and consults the current fixed-point role map for known constructors. Missing role metadata on an unknown constructor is treated as nominal by the walker so an absent interface cannot make a coercion more permissive. Errors identify the smallest offending application, binder, or annotation.

The same unifier serves annotations, instance heads, and inference bindings, so a kind that is accepted in one position cannot be rejected in another. `kind_of_type` reads the kind of a resolved inference type through that substitution; it never falls back to a per-module table. Where a primitive is concerned, the denotation and the primitive kind table are read from the same place, which is what keeps `Row`, `Record`, `Array`, and `Function` meaning one thing in an annotation, in a constraint argument, and in a coerced boundary.

## Code map

Kind solving is part of the type checker, because every type unknown it maintains is owned by inference and the kind of a binding is checked at the binding. The kind module sits beside `unify.rs` and the class solvers and exposes:

- `check_program(modules: &[hir::Module]) -> (CheckedKindEnv, Vec<KindDiagnostic>)` owns kind registration, synonym checks, declaration kind inference, role inference, and annotation validation. `KindDiagnostic` carries the declaring `ModuleId`, so the driver attributes it to that module's source.
- `check_module(module: &hir::Module, imported: &CheckedKindEnv) -> Vec<KindDiagnostic>` re-checks one module's own declarations and instance heads against the already checked imported schemes, and reports a missing scheme rather than inventing one.
- `denote_kind(&hir::Type, &mut KindScope) -> Kind` is the only translation from a type expression to a kind, and `primitive_kind(BuiltinType) -> Kind` is the only primitive kind table; both are reachable from every other module in the checker.
- `unify_kind(&mut KindState, Kind, Kind, span)` and `bind_kind_variable` are the only kind equation operations. `KindState` holds the kind substitution, the next kind variable, and the rigid set.
- `kind_of_type(&InferType, &mut KindState) -> Option<Kind>` returns the kind of an inference type for the binding check and for coercion.
- `infer_roles_fixed_point(modules, foreign_role_signatures) -> (RoleEnv, Vec<KindDiagnostic>)` walks constructor fields across modules and validates annotations.
- `CheckedKindEnv::kind_scheme(TypeId) -> Option<&KindScheme>` and `CheckedKindEnv::role_of(TypeId) -> Option<&[Role]>` are the only lookups consumed by inference and class entailment; the solver does not reconstruct roles or kinds from surface syntax.
- HIR owns role annotation names and source ranges. The checked environment owns resolved identities, final roles, kind schemes, and the declaring module of each entry.

Role inference stays a separate submodule because it is a fixed-point analysis over fields rather than an equation solver; it consumes the same synonyms and identities.

Merging the kind crate into the type checker is a packaging consequence of this ownership, not a fix for it. While kind inference runs as a separate program-level pass with no type unknowns to maintain, the type checker has to re-derive kinds for its own variables; the semantic seams above are the ones that matter, and they are closed by the single denotation, the single solver, and the program-level environment rather than by where the code is compiled.

## Invariants and verification

Every checked type has one kind; every value signature ends at `Type`, every class constraint at `Constraint`, and every row at `Row k`. Every type unknown has a recorded kind, and every recorded kind is maintained by the single substitution, so a type and its kind cannot disagree after a binding. One denotation and one primitive table define every kind in the compiler, so no two modules can read the same kind expression differently. No referenced declaration is checked against a fabricated kind: an absent scheme is a diagnostic. Every kind diagnostic names the module that declares the offending type. No unknown kind, escaped skolem, cyclic synonym, or partial synonym crosses into THIR. Every role vector has the declaration's parameter arity; data/newtype roles are no more permissive than their fields allow; foreign role signatures are preserved as trusted metadata; and imported types use the declaring module's roles. Verify role inference through nested and mutually recursive declarations, all three role levels, invalid weakening, foreign defaults/signatures, and source spans against official `purs` cases; verify the program-level environment through a cross-module kind conflict attributed to the declaring module, a kind annotation on an imported type, and a missing scheme.

## Worked example

`Record (x :: Int | r)` has kind `Type` when `r :: Row Type`: the field `x` has kind `Type`, the extended row has kind `Row Type`, and `Record` consumes that row. A type-level label literal has kind `Symbol`; using it as the field's value type is a kind error. A synonym `type R a = { x :: a }` cannot be passed as a bare constructor where `Type -> Type` is expected until its parameter is supplied, matching PureScript's saturation rule.

In a program where `ModuleA` declares `data Box :: Type -> Type` and `ModuleB` uses it, `check_program` records that scheme once. `ModuleB`'s own check reuses it, so `ModuleB`'s use of `Box` at a kind `ModuleA` never declared is rejected against `ModuleA`'s declaration and reported against `ModuleA`'s source. If `ModuleB` were checked first against no environment, the same use would have to invent a kind for `Box`; the two runs would then disagree about which module owns the diagnostic.

## Boundaries and interfaces

P5 consumes normalized resolved HIR and emits a checked kind environment for term inference. The environment is the only source of kind and role metadata for a declaration that another module uses, and it is not rebuilt per module. THIR retains checked type structure and kind information needed to verify polymorphic uses, but no mutable kind solver state. Core lowering may erase kind abstractions after type checking.

## Open questions and future work

Expression-level kind checking and explicit kind application elaboration remain future work; they build on the shared denotation and solver above rather than on a separate kind path. The kind requirements of the `Prim.RowList`, `Prim.Symbol`, `Prim.Int`, and `Prim.TypeError` members are specified in [primitives](prim.md) and are checked here from the registry. Where the current type language reaches a form the shared model already defines, the model stays as the target and the gap is recorded as implementation coverage. Role implementation coverage belongs in [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md) and the [roles and coercions acceptance record](../../../implementation/frontend/roles-and-coercions.md); `Coercible` constraint solving and evidence are specified in [classes and evidence](classes-and-evidence.md).

## References

- [PureScript `Types.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Types.hs), [`Kinds.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Kinds.hs), and [`Environment.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Environment.hs). `primTypes` is the reference for the primitive kind table, `elaborateKind` for kind denotation, `generalizeUnknowns` and `checkQuantification` for declaration kind quantification, and `UnknownName` for missing kind metadata.
- [PureScript role inference](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Roles.hs).
- [PureScript synonym checking](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Synonyms.hs).

## Implementation notes

The current implementation has two kind paths and does not yet have the single one
this document specifies. `psrs_kind::check_roles` runs a program-level pass to build
the role table and keeps only its kind schemes; the diagnostics that run produced are
dropped, and the driver then calls `psrs_kind::check_module` per module, which has no
imported schemes and gives an unknown imported type a fresh kind variable so repeated
uses agree. Roles are expanded and walked as specified, with nominal as the
conservative fallback when synonym expansion meets a cycle.

Kind semantics are also currently duplicated and already disagree. Within
`psrs-kind` itself, `denote_kind` reads the primitive `Row` as a dedicated head,
`Record` and `Int` as builtin constructors, and `builtin_type_kind` reads the same
three as `Type -> Type`, `Row Type -> Type`, and `Type`; the dedicated head and the
builtin reading of `Record` are two spellings that the unifier cannot reconcile. The
coercion module under
`crates/psrs-typecheck/src/typecheck/classes/coercion/kinds.rs` carries a third
denotation, substitution, and unifier, in which `Row` is `Type -> Type` and `Record`
is `Row Type -> Type` — official PureScript's readings, which a dedicated `Row` head
cannot express. Ordinary `bind_variable` performs the occurs, skolem-escape, and
level steps only, so a type binding is checked for kinds only when it is reached
through coercion. Unifying these with one spine requires every primitive to denote
itself, retiring the dedicated kind constants and the `Row` head, and moving the kind
state onto the inference variables.

Consolidating `psrs-kind` into the type checker is a consequence of that move rather
than a prerequisite: kind checking's main consumer is still P5, and the crate split
does not cause the drift above.
