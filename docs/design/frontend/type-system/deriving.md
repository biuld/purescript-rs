# Deriving

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [Classes and evidence](classes-and-evidence.md),
[kinds](kinds.md), [primitives](prim.md), and
[type inference](type-inference.md); PureScript's standard class library,
algebraic data types, newtypes, and compiler code generation.

**Summary:** A `derive instance` or `derive newtype instance` declaration is
elaborated at P5 into ordinary instance members and a concrete instance head.
The rule for a supported class is chosen by the resolved identity of that class
in one checked deriving registry, never by the name a use spelled. Generated
members then flow through the same instance checking and evidence selection as
written members, so a derivation creates no second evidence path.

## Scope

This document owns the compiler's deriving rules: which class identities have a
rule, the structural traversal each rule performs, the variance and
instance-existence checks those traversals require, the diagnostic for an
underivable field, and the adaptation a `derive newtype` method receives at each
function boundary.

It does not own the class solver, instance search, coherence, evidence
elaboration, or superclass projection; those are
[classes and evidence](classes-and-evidence.md). It does not own the
`Coercible` proof; [primitives](prim.md) owns that rule and this document only
consumes its proof boundary. It does not own roles or the checked kind
environment ([kinds](kinds.md)), the constraint and `TypeTemplate`
representation, or the surface grammar of `derive` declarations
([parsing and CST](../syntax/parsing-and-cst.md)). Deriving is a distinct topic
from the [roles and coercions](../../../implementation/frontend/roles-and-coercions.md)
concerns that the two share a feature matrix row with.

## Background

PureScript's `derive instance` asks the compiler to synthesize a class instance
from a locally declared data type's constructors and fields. Two families exist,
matching the official compiler:

- **Structural classes** derive each method by walking the declared fields.
  `Eq` and `Ord` compare or order fields; `Functor`, `Bifunctor`,
  `Contravariant`, and `Profunctor` map over fields; `Foldable`, `Bifoldable`,
  `Traversable`, and `Bitraversable` fold or traverse them. `Eq1`/`Ord1` are
  delegated to their monomorphic counterpart.
- **Head-shape classes** `Newtype` and `Generic` do not derive a method body
  from fields so much as a *type argument*: the newtype's wrapped type, or the
  `Data.Generic.Rep` representation of the data type. `Generic` also generates
  its `to` and `from` methods from that representation.

The official compiler splits the two families across stages for a reason:
`Newtype` and `Generic` depend only on the declaration's structure and replace a
user-written type wildcard, so `Sugar/TypeClasses/Deriving.hs` elaborates them
before type checking. The structural classes need types and the instance
environment to decide how a field is used, so
`TypeChecker/Deriving.hs` elaborates them during type checking. Both stages
synthesize ordinary expressions that the normal type checker then checks; neither
introduces a special evidence form.

`derive newtype instance` is a separate strategy. It does not walk the wrapped
type's fields at all: it selects the dictionary for the wrapped class head and
adapts each method at function boundaries through checked `Coercible` evidence.
The wrapped head is a class constraint on the same constraint spine every other
instance context uses.

## Model

```text
Strategy      = KnownClass | Newtype
KnownClass    = Eq | Eq1 | Ord | Ord1
              | Functor | Bifunctor | Contravariant | Profunctor
              | Foldable | Bifoldable | Traversable | Bitraversable
              | Generic | Newtype
CovariantPair = { mono: class, bi: class }        -- Functor/Bifunctor, Foldable/Bifoldable, ...
ContraPair    = { contra: class, pro: class }      -- Contravariant/Profunctor

ParamUsage    = IsParam
              | IsLParam
              | MentionsParam(ParamUsage)
              | MentionsParamBi(These(ParamUsage, ParamUsage))
              | MentionsParamContravariantly(ParamUsage)
              | IsRecord(labels)

DerivingRegistry = {
    class:  TypeId -> KnownClass,
    method: (KnownClass, name) -> SymbolId,
    ordering: Option<{ ty: TypeId, lt: SymbolId, eq: SymbolId, gt: SymbolId }>,
    generic:  Option<GenericRep>,
}

GenericRep = {
    no_constructors: TypeId, no_arguments: TypeId, argument: TypeId,
    product: TypeId, sum: TypeId,
    constructor: SymbolId, inl: SymbolId, inr: SymbolId,
}
```

`These(a, b)` is the shared three-way shape `This a`, `That b`, or `These a b`:
a field can mention the left parameter, the right, or both, and a bipartite rule
needs that distinction rather than a single "mentions the parameter" bit.

The **deriving registry** is the single owner of "which classes the compiler
knows and what their methods and representation types are." It is built once
when the semantic environment is constructed, from the resolved declarations of
the program plus the compiler's trusted core identity. Every rule reads it by
`TypeId`; the module and class names are inputs to its construction and never a
runtime predicate. A class with no entry is not derivable, whatever its name.

`ParamUsage` records how one occurrence of a declared type parameter is used in
a field. `IsParam`/`IsLParam` are the monomorphic/bipartite parameter itself;
`MentionsParam` is a monotone nested occurrence; `MentionsParamBi` is a
bipartite occurrence carrying left and right usages independently;
`MentionsParamContravariantly` is an occurrence that flips polarity;
`IsRecord` records a record whose fields are used independently. This is the
official `ParamUsage` model; it is what lets one traversal serve `Functor`,
`Foldable`, and `Traversable` and their bipartite counterparts instead of
hand-writing one shape test per class.

The generated method terms are ordinary resolved HIR. They reference the
class's own method symbols, the registry's representation constructor symbols,
and freshly allocated locals, and they convert to THIR through the ordinary
inference entry point.

## Design

### One registry, selected by identity

Selecting a rule is a lookup `registry.class[class_id]`, not a match on a
qualified name. `KnownClass` is therefore a closed enum with one constructor per
supported class, and `registry.method` supplies each method symbol from the same
resolved class declaration the solver uses. The registry also supplies the
`Data.Generic.Rep` type and constructor symbols and the `Data.Ordering`
constructors, so no rule re-derives a symbol by scanning the type namespace or
comparing module strings.

The registry is built from the checked declarations, so a re-export preserves
the defining identity and a same-named user class in another module does not
gain a rule. A class is known because the compiler put it in the registry, not
because a string matched.

### Head shape, not head spelling

For a structural class, the instance head's final argument must be a locally
declared data or newtype constructor, fully applied for `Eq`/`Ord` (which
traverse every parameter), or applied to all but the final one for
`Functor`/`Contravariant` (one parameter), or to all but the final two for
`Bifunctor`/`Profunctor` (two parameters). The check is on the flattened spine
of the resolved type, not on its printed form.

Before any field is inspected, the field types are synonym-expanded through the
shared normalizer, so a synonym that expands to `f a` is treated as an applied
variable exactly as `purs` treats it. Eq and Ord must expand fields too; a rule
that only inspects the raw field type diverges from `purs` on a synonym-to-`f a`
field.

### Field usage and variance

A structural rule does not decide per field whether to direct-map, recurse, or
reject. It first computes a `ParamUsage` for every constructor field and
validates it against the class's variance. The official rule is:

- A field that does not mention any parameter is inert.
- The parameter itself is `IsParam` (or `IsLParam`).
- A field whose head is a class for which the required instance exists is a
  monotone/bipartite mention and is mapped through that class.
- A parameter under a function input, or any occurrence the class cannot map,
  is rejected with `CannotDeriveInvalidConstructorArg`, naming the related
  source positions.

The existence check consults the same visible instance environment the solver
uses. This is what makes the rule agree with the solver: a derivation is
rejected at the declaration when a field head has no instance for the mapping
class, instead of being accepted and failing later with `NoInstanceFound`. It is
also what lets a bipartite class choose between the mono and bi mapping when a
field head has an instance for one and not the other.

### Generated terms and one builder

Every rule emits terms through one small builder that owns fresh local
allocation, lambda/case construction, application, and the boolean and ordering
literals. A rule states only its traversal — the two-lambda/case skeleton is
shared, not re-written per class. The builder's output is resolved HIR handed to
`infer_derived_method`, which elaborates the method signature at the instance's
class arguments and infers the body with that expected type. Dictionary
selection for each mapped field therefore happens in the ordinary solver, not in
the rule.

`Eq1` and `Ord1` are the trivial delegations the official rule uses: `eq1 = eq`
and `compare1 = compare`, typed at the applied argument, so the matching
monomorphic instance supplies the dictionary.

### Newtype deriving

`derive newtype instance C ... T` resolves `T` to a locally declared newtype,
computes its wrapped type, and pushes the wrapped constraint `C ... wrapped` as
an ordinary wanted. For each method it selects the wrapped dictionary's method
and adapts it to the derived head: it peels shared method quantifiers, then
inserts a `Coercible` conversion at each arrow boundary; a method whose type is
already equal needs no conversion. The proof is the compiler-owned `Coercible`
relation from [primitives](prim.md); deriving produces no coercion proof of its
own and cannot forge one.

Method-level `forall` binders are instantiated once with stable placeholders and
substituted separately at the two heads, so a quantifier shared by the method
signature has one identity on both sides of the adapter, while a quantifier that
shadows a class parameter stays scoped to the body.

### Wildcard handling and Newtype/Generic

`derive instance newtypeX :: Newtype T _` and
`derive instance genericX :: Generic T _` carry a trailing type wildcard: the
wrapped type or representation determines that argument. The wildcard is
resolved by the rule and the instance is recorded with a concrete final
argument, so instance-head validation and the solver never see a derivable
wildcard. Instance recording must not special-case a class identity to tolerate
one.

### Diagnostics

Each failure maps to its official `errorCode`, and there is one variant per
official error rather than one catch-all:

| Condition | Code |
| --- | --- |
| Class has no registry rule | `CannotDerive` |
| Head is not the expected local constructor | `ExpectedTypeConstructor` |
| Structural class head has the wrong arity | `InvalidDerivedInstance` |
| `derive newtype` head is not a newtype | `InvalidNewtypeInstance` |
| `Newtype` class derived for a data type | `CannotDeriveNewtypeForData` |
| Field cannot be mapped under the class's variance | `CannotDeriveInvalidConstructorArg` |
| Deriving type cannot be found | `CannotFindDerivingType` |
| Newtype/Generic wildcard missing | `ExpectedWildcard` |

Rejected alternatives: a single `UnsupportedClass`-style kind loses the code the
official suite matches on and collapses unrelated failures into one bucket.

### Rejected: name-based selection and per-rule scaffolding

Matching a class by its declarer's module string and short name re-derives an
identity the resolver already owns, lets a user module that reuses a core
module name claim a rule, and scatters the same table across every rule. Writing
a separate field walk per class, and a separate pattern/lambda skeleton per
rule, duplicates the variance decision and lets the rules drift apart. Both are
rejected in favor of one registry and one usage analysis feeding one traversal
builder.

## Algorithms

```text
derive_instance(instance):
    strategy = instance.derivation or return instance unchanged
    match strategy:
      Newtype  -> derive_newtype(instance)
      KnownClass -> known_class = registry.class[instance.class_id]
                    or fail CannotDerive
                    match known_class:
                      Newtype -> derive_newtype_class(instance)   # head-shape only
                      Generic -> derive_generic(instance)
                      _       -> derive_structural(known_class, instance)

derive_structural(class, instance):
    utc = require_local_constructor(instance.head, class)   # arity per class
    usages = validate_params(class, utc)                    # ParamUsage per field
    for each method of the class:
        body = mk_traversal(class, usages, utc)
        emit Member(method, infer_derived_method(method, head_args, body))

validate_params(class, utc):
    expand all field type synonyms
    for each field:
        usage = usage_of(class, field, polarity = covariant)
        if usage is unsupported under class.variance:
            fail CannotDeriveInvalidConstructorArg at the offending span
    return usages

usage_of(class, ty, polarity):
    if ty does not mention any parameter: return inert
    if ty is the parameter:              return IsParam / IsLParam
    if ty is an application f arg and f is a type constructor:
        if registry instance env proves class.mapping_class at f:
            return MentionsParam(usage_of(class, arg, polarity))
        elif contra/pro mapping class proves at f:
            return MentionsParamContravariantly(usage_of(class, arg, flipped polarity))
        else: fail
    if ty is a function input:           return fail at this span
    ... record, forall, constrained cases follow the model ...

mk_traversal(class, usages, utc):
    f, g = fresh locals; v = fresh local
    branches = for each constructor:
        bind fields to fresh locals
        argument = for each field usage: map_field(usage, local)
        reconstruct constructor with mapped arguments
    return \f [\g] v -> case v of branches

map_field(IsParam, x)                        = f x
map_field(MentionsParam(u), x)               = map   (map_field(u)) x
map_field(MentionsParamBi(These(uL,uR)), x)  = bimap (map_field(uL)) (map_field(uR)) x
map_field(MentionsParamContravariantly(u),x) = cmap  (map_field(u)) x   # or lmap/dimap via Profunctor
map_field(IsRecord(fields), x)               = record update applying map_field per field

derive_newtype(instance):
    wrapped = wrapped_type_of(instance head's final argument)   # or fail InvalidNewtypeInstance
    push_wanted(class, init(head_args) ++ [wrapped])
    for each method:
        template = elaborate method signature with one placeholder per class parameter
        source   = template[wrapped]; target = template[head_args]
        body     = adapt(select(source), source, target)         # Coercible at each arrow
        emit Member(method, body)

adapt(value, source, target):
    peel matching shared foralls
    if source == target: return value
    if source = a -> b and target = a' -> b':
        return \x -> adapt(value (coerce x), b, b')
    return coerce value          # checked Coercible source -> target
```

Edge cases: a data type with no constructors derives a `Functor`/`Foldable` body
that is an empty case, and `Generic` uses `NoConstructors`; a zero-field
constructor uses `NoArguments`; `Ord` orders constructors by declaration order
and fields lexicographically; an applied-variable field uses the class's `1`
counterpart when it has one; a nullary `derive` for a class the compiler knows
only as head-shape (`Newtype`) produces a dictionary with superclass fields and
no method.

## Code map

The deriving topic lives inside the P5 owner. The intended organization:

- `typecheck/classes/deriving/mod.rs` — `Strategy`/`KnownClass` entry points and
  `derive_instance`, dispatching to the rule families.
- `typecheck/classes/deriving/registry.rs` — `DerivingRegistry`, its construction from
  the resolved declarations and trusted core identity, and the `TypeId` lookup.
- `typecheck/classes/deriving/usage/` — `ParamUsage`, `usage_of`, and
  `validate_params`, reading the shared instance environment for the
  existence checks.
- `typecheck/classes/deriving/syntax.rs` — the one term builder: fresh locals,
  lambdas, cases, application, literals. No rule builds HIR by hand.
- `typecheck/classes/deriving/eq.rs`, `ord.rs`, `functor.rs`, `bifunctor.rs`,
  `contravariant.rs`, `profunctor.rs` — the `Functor`-shaped rules.
- `typecheck/classes/deriving/foldable/` — `Foldable`/`Bifoldable`, driven by the
  usage tree.
- `typecheck/classes/deriving/traversable.rs` — `Traversable`/`Bitraversable`.
- `typecheck/classes/deriving/newtype.rs` — newtype strategy and the `Coercible`
  adapter.
- `typecheck/classes/deriving/generic.rs` — representation type and `to`/`from`.
- `typecheck/error.rs` — the official deriving diagnostic variants and their
  `errorCode` mapping.

`identity` is defined by `Control.Category` and re-exported by `Data.Function`;
the registry retains that defining SymbolId. Minimal standalone fixtures may
provide `Data.Function.identity` directly, resolved during registry construction.
Consumers do not infer identity from an import spelling.

The registry also holds the trusted core value identities (`append`, `mempty`,
`identity`, `apply`, `pure`) the fold and traversal rules name, resolved from
the program's value declarations by declaring module and name.

The registry is constructed where the semantic environment is, so every rule
reads it as immutable checked metadata. The current file inventory is not the
design; code is expected to conform to this map, not the reverse.

## Invariants and verification

- Every rule is selected by `TypeId`; no rule compares a class module or name at
  use time.
- A generated method proves exactly its class method's type at the instance
  head. The ordinary inference path establishes this; there is no deriving-only
  evidence form.
- Field usage agrees with the class's variance and with the visible instance
  environment: a derivation is accepted exactly when every field occurrence has
  the mapping instance the traversal needs.
- A newtype method conversion is authorized by a checked `Coercible` proof; the
  adapter inserts no unchecked cast.
- A derivable wildcard is resolved before instance recording; no instance is
  recorded with a wildcard it should not have.
- Each failure carries the official `errorCode` for its condition.

The THIR verifier trusts the deriving rule's class selection and generated
evidence, on the same basis it trusts the solver's instance choice
([classes and evidence](classes-and-evidence.md#boundaries-and-interfaces)). A
guarantee that must be verified rather than trusted needs its metadata carried
across the P5 boundary; deriving does not change that boundary.

Verification is by the official differential set: for each supported class, an
accepted source case and a rejected field-variance case compared with `purs` on
accept/reject and diagnostic code, plus the runtime cases for the classes whose
methods execute.

## Worked example

```purescript
data Pair a b = Pair a b | Left a | Right b

derive instance bifunctorPair :: Bifunctor Pair
```

The rule is selected from the instance's class,
`Data.Bifunctor.Bifunctor`, through the registry built at P5; the type `Pair`
itself selects nothing. The head is locally declared and applied to all but its
final two parameters. Field usage is computed after synonym expansion:

- `Pair a b`: `IsLParam` for `a`, `IsParam` for `b` — both covariant.
- `Left a`: `IsLParam` for `a`.
- `Right b`: `IsParam` for `b`.

`map_field` maps `a` with `lmap`-style left marker `f` and `b` with right marker
`g`, so the generated body is

```text
\f g v -> case v of
  Pair a b -> Pair (f a) (g b)
  Left  a  -> Left  (f a)
  Right b  -> Right (g b)
```

`infer_derived_method` checks it against `bimap :: forall a b c d. (a -> b) ->
(c -> d) -> Pair a c -> Pair b d`, and the pair of mapper arguments fixes the
same parameter identities the instance head supplies. A field `a -> b` would
fail `validate_params` at its span with `CannotDeriveInvalidConstructorArg`,
because the left parameter appears under a function input.

## Boundaries and interfaces

Deriving consumes resolved HIR instance declarations, the checked kind and role
environment, the class environment, and the visible instance environment. It
produces generated method terms and, for `Newtype`/`Generic`, a concrete final
head argument, handing both to the instance-checking path in
[classes and evidence](classes-and-evidence.md). It consumes the `Coercible`
proof from [primitives](prim.md) but does not implement it. It adds no node to
CST, AST, or HIR; a derived instance is an ordinary instance after elaboration.

## Open questions and future work

- **Stage of `Newtype`/`Generic`.** The official compiler resolves their
  wildcard in sugar. This design keeps all deriving at P5 so the registry and
  the diagnostic surface are one owner. If the wildcard-before-recording rule is
  easier to guarantee at P4, the head-shape rules may move there without moving
  the structural rules; the split is a placement choice, not a semantic one.
- **Trusted core identity.** The registry needs stable identities for the core
  declarations it names. How those identities are pinned (a trusted-declaration
  table, or a checked marked set) is [primitives](prim.md)'s identity question;
  deriving only requires that the registry is built once and read by `TypeId`.
- **Variance and the instance environment.** `validate_params` consults instance
  existence. The exact boundary between "no instance yet" and "no instance
  possible" is the solver's, and deriving must not form a second opinion; the
  interaction belongs with the instance-search contract.
- **Remaining classes.** `Profunctor` and the foldable/traversable bipartite
  classes have no implementation yet; this document specifies their intended
  rules, and the acceptance record tracks which have source and runtime
  evidence.

## References

- PureScript `TypeChecker/Deriving.hs` (structural rules, `ParamUsage`,
  `validateParamsInTypeConstructors`, `mkTraversal`) and
  `Sugar/TypeClasses/Deriving.hs` (`Generic` and `Newtype` elaboration).
- PureScript `Constants/Libs.hs` for the class, method, and representation
  symbols the registry must resolve.
- [Classes and evidence](classes-and-evidence.md) for the solver and evidence
  contract a derived instance uses.
- [Primitives](prim.md) for the `Coercible` proof boundary.
- [Kinds](kinds.md) for roles and the checked environment.

## Implementation notes

These record where the current code deviates from this design. They are not the
design; the sections above are normative.

- **Usage owns structural traversal.** `usage/` computes field variance and
  checks visible instance-head identities. As in official deriving, this is a
  head-constructor availability test; ordinary inference checks the generated
  member's complete instance constraints. All mapping families generate terms
  from that tree, including canonical record fields and scoped `forall`
  occurrences. Fold and traversal families consume the same analysis. An
  unsupported mapped open row remains a diagnostic.
- **Coverage.** Every structural class has a rule. Generic round trips,
  traversal effects, function Contravariant adapters, polymorphic newtype
  adapters, and applied-variable Eq/Ord dictionaries have executed runtime
  evidence. Coverage and remaining verification obligations are tracked in the
  [deriving acceptance record](../../../implementation/frontend/deriving.md).
