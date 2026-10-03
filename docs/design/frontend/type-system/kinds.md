# Kinds and Type Constructors

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [modules and resolution](../semantics/modules-and-resolution.md), [frontend boundaries](../00-ir-boundaries.md), and PureScript kind polymorphism.

**Summary:** P5 checks kind-correct type, class, data, newtype, and synonym declarations before term checking. Its checked constructor environment also carries representation roles used by the class solver for `Coercible`. The design follows PureScript's shared type-and-kind language, including polymorphic kinds, type-level `Symbol` and `Int`, and rows. Because kinds are types, one denotation, one primitive kind table, and one kind solver own every kind in the compiler: declaration annotations, instance heads, expression-level type applications, and the kind of every type unknown all resolve through them. The checked environment those operations produce is program-level, and an imported declaration's kind is the one its declaring module checked.

## Scope

This document owns kind inference, kind signatures, constructor and synonym legality, and type roles. It also owns the only denotation of a type expression read as a kind, the only primitive constructor kind table, the only kind unifier, the kind recorded for every type unknown, and the checked program-wide kind environment that kind inference and role inference produce together. [Type inference](type-inference.md) owns term checking and consumes those operations; [rows](rows-and-records.md) owns row equations; [classes](classes-and-evidence.md) owns constraint evidence; [primitives](prim.md) owns the `Prim.*` members and their rules, whose kinds are checked here from the registry exactly as a source declaration's are.

A feature module may not keep a private kind table, a private translation from types to kinds, or a private kind compatibility test. Whether a type operation checks kinds must not depend on which module the caller entered through; the kind of a type is one fact with one owner.

## Background

PureScript kinds are types: `Type`, `Constraint`, `Symbol`, `Row Type`, and higher-kinded arrows are written in the same type language. Primitive constructors include `Row`, `Record`, `Function`, and `Array`; the primitive modules also provide `RowList` and type-level `Int` operations. A polymorphic constructor may quantify a kind variable, which the compiler instantiates from its use. purs 0.15.16 has no source form that applies a kind argument to a type constructor; a quantified kind argument is always supplied implicitly, and the kind checker synthesizes `KindApp` to record it. The forms that do name a kind explicitly are `forall @a b . t` and the kind ascription `t :: k`. `Symbol` is the kind of type-level strings, while type-level integer literals have kind `Int`. Kind inference therefore needs quantification and subsumption, beyond first-order HM kind unification.

## Model

```text
TypeExpr = Constructor | Variable | Application
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

Register primitive kinds and declaration heads before checking bodies. Resolve kind signatures, infer missing parameter kinds, instantiate polymorphic kinds at uses, and skolemize expected `forall` kinds when checking annotations. Ordinary type application consumes an arrow kind, while quantified kind arguments are instantiated implicitly from use, because no source form applies a kind argument to a type constructor. `forall @a b .` supplies a binder's kind explicitly and `t :: k` ascribes one; both are ordinary source forms rather than kind application. Check class heads against `Constraint`, value types against `Type`, row entries against their row parameter, and type-level literals against `Symbol` or `Int`.

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

A `Type | Row(Type) | Arrow` enum is rejected because it cannot express `Symbol` or polymorphic kinds. Eager synonym expansion before cycle checking is rejected because it may diverge. Two kind checkers, one per module, are rejected because a program-level conflict found by one run is not guaranteed to be found again by the other, so the diagnostic that matters depends on which entry point the driver chose.

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

Kind inference traverses source type applications and constructs the separate `Kind::App` structure used inside kinds. It instantiates quantified kind variables implicitly rather than retaining `KindApplication` in the source type spine. Unification compares constructors by resolved identity and decomposes applications; `forall` comparison uses instantiation or skolemization according to direction. Before role walking, synonym applications are expanded with capture-avoiding substitution: aliases are transparent and cannot add nominality, including when an imported alias occurs inside a data or newtype field. Synonym cycles are rejected by kind checking; role analysis must also terminate conservatively if it is invoked while such an error is present. Role walking respects quantified binders, marks constraint arguments nominal, and consults the current fixed-point role map for known constructors. Missing role metadata on an unknown constructor is treated as nominal by the walker so an absent interface cannot make a coercion more permissive. Errors identify the smallest offending application, binder, or annotation.

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

Expression-level kind checking remains future work; it builds on the shared denotation and solver above rather than on a separate kind path. Official's CST has no kind-application node — its kind checker synthesizes `KindApp` while instantiating a polymorphic kind — and this compiler performs the same instantiation in the kind solver without retaining `KindApplication` in the source type spine. The source forms that do name a kind or type explicitly, `forall @a b .`, `t :: k`, and the visible type application `e @T`, are separate nodes with their own lowering; the last of these is frontend coverage under FE-17 and is what still blocks `passing/4500.purs`, `passing/4535.purs`, and `passing/VTAsClassHeads.purs` at P2. The kind requirements of the `Prim.RowList`, `Prim.Symbol`, `Prim.Int`, and `Prim.TypeError` members are specified in [primitives](prim.md) and are checked here from the registry. Where the current type language reaches a form the shared model already defines, the model stays as the target and the gap is recorded as implementation coverage. Role implementation coverage belongs in [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md) and the [roles and coercions acceptance record](../../../implementation/frontend/roles-and-coercions.md); `Coercible` constraint solving and evidence are specified in [classes and evidence](classes-and-evidence.md).

## References

- [PureScript `Types.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Types.hs), [`Kinds.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Kinds.hs), and [`Environment.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Environment.hs). `primTypes` is the reference for the primitive kind table, `elaborateKind` for kind denotation, `generalizeUnknowns` and `checkQuantification` for declaration kind quantification, and `UnknownName` for missing kind metadata.
- [PureScript role inference](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Roles.hs).
- [PureScript synonym checking](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Synonyms.hs).

## Implementation notes

Kind checking runs once per program now. `psrs_kind::check_program` registers the
primitive table and every declaration head, rejects synonym and kind-declaration
cycles, infers and generalizes declaration kinds, infers roles to a fixed point, and
returns one zonked `CheckedKindEnv` together with its diagnostics. The driver reports
those diagnostics against the module each names and hands the same environment to
every module's type check, so the program-wide conflict a use in one module exposes
in another is found by the run that owns the declaration. `psrs_kind::check_module`
re-checks one module's own declarations, annotations, and instance heads against the
schemes that environment published, and reports a referenced declaration with no
checked scheme instead of giving it a fresh kind variable. Roles are expanded and
walked as specified, with nominal as the conservative fallback when synonym expansion
meets a cycle.

`Kind` has no reserved constant and no dedicated `Row` head: a primitive read as a
kind is `Builtin`, `Row k` is `App(Builtin(Row), k)`, and `primitive_kind` is the one
table of what kind a primitive has when it is used as a type. `denote_kind`,
`primitive_kind`, `unify_kind`, `bind_kind_variable`, and `KindState` are exported from
`psrs-kind` so a module cannot reach for a private table or a private unifier.

Inference reads those operations through one module,
`crates/psrs-typecheck/src/typecheck/kind.rs`. `kind_of_type` is the one reading of an
`InferType`'s kind, `kind_from_hir` is a thin owner over `denote_kind` that reports an
unlowered operator chain against its own range, and `unify_kind` is the only way
inference solves a kind equation. The kind state is a single `psrs_kind::KindState` in
`InferState` beside the type substitutions, with the recorded kind of each inference
type variable in `variable_kinds` next to it, so `speculate` rolls a kind binding back
with the type binding that produced it. The coercion module's private denotation,
primitive table, substitution, and unifier are gone; `coercion_kinds_compatible` now
reads both kinds through `kind_of_type` and solves the equation speculatively, so a
conversion that turns out not to hold leaves no kind binding behind. `bind_variable`
runs the kind check alongside the occurs, escape, and level rules, and a row tail
solved during row unification keeps the kind its row admits through that same
binding.

What remains, and what the next wave owns:

- The well-scoped-quantification rule is not implemented. A scheme quantifies the kind
  unknowns its own definition leaves undetermined and keeps the kinds the definition
  determines, and `data Branch m = Branch (m Branch)` is rejected by the occurs check
  rather than generalized, but no `QuantificationCheckFailureInKind` is reported when
  an implicitly generalized kind mentions a type variable that is not quantified
  explicitly. Kind variables and type-level `forall` binders live in different state
  here, so the dependency order the official check needs is not recorded yet.
- An instance head whose class is declared in another module is still skipped: the
  arity rule needs the class declaration, and a class the checking module does not
  declare has no arity here. Only the declaring module's own run checks such a head.
- `data Wrap :: Lib.Box Type -> Type` reads `Lib.Box Type` as the kind "`Lib.Box`
  applied to `Type`", because a kind annotation is read as a kind rather than
  elaborated as a type. Official `elaborateKind` eliminates an application of an arrow
  kind instead, so the two differ for a higher-kinded user declaration used in a kind
  signature. The shared model is the target; the divergence is unmeasured.
- Three signatures in the code map above differ from what is exported.
  `denote_kind` returns `Option<Kind>` so an unlowered type operator chain is a
  reported error rather than a fabricated kind; `infer_roles_fixed_point` takes no
  separate foreign-role-signature argument, because a foreign declaration's roles
  come from its own `type role` annotation; and `check_module` has no `local` flag,
  because every caller is checking one module against an environment its declaring
  modules produced. `kind_of_type` over an `InferType` is not implemented in
  `psrs-kind`, because an inference type is not HIR: it lives in the type checker's
  `kind.rs` and reads the same solver.

Consolidating `psrs-kind` into the type checker is now a consequence of packaging
alone: kind checking's main consumer is still P5, and the crate split is not what
caused the drift. The single denotation, the single solver, and the program-level
environment are what close the ownership, and they are now the only readers.
