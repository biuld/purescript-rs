# Runtime Representation and Checked Boundaries

**Feature:** F-02  
**Status:** Stable (design)  
**Prerequisites:** [functional core](../../frontend/semantics/functional-core.md),
[classes and evidence](../../frontend/type-system/classes-and-evidence.md),
[CC IR](cc-ir.md), [MIR](mir.md); type-erasure semantics and dictionary
passing. Read [IR boundaries](../00-ir-boundaries.md) first.  
**Summary:** A source type does not determine a runtime representation. One
representation model and one checked-boundary contract govern every place the
backend changes or recovers a representation: scalar boxing and unboxing under
erasure, reference cast and recovery, canonical aggregate conversion, callable
adaptation, dictionary projection, effect token threading, and coercions.
Every change is a **conversion plan** built from checked **evidence** and the
producer's **representation policy**, then emitted as explicit CC operations.
A `RuntimeRep` and its target `Layout` are the two levels of one model, joined
by a total mapping per target. This document owns the target-neutral `RuntimeRep`
model and the layout-mapping contract; each topic document owns the concrete
operations that instantiate it, and each target document owns its `Layout`.

## Scope

This document owns the shared representation model (`RuntimeRep`,
`RepresentationPolicy`), the checked-boundary contract (`Boundary`, `Evidence`,
`ConversionPlan`), the ownership rules for who produces and who consumes them,
generic aggregate normalization and the aggregate conversion plan, and the
single conversion-planning algorithm. It also defines the layout obligations a
conversion depends on; it does not own the concrete layout that realizes a
representation. It exists so that no feature introduces a private rule for "how
this value is adapted".

It does not own the concrete Wasm GC layouts (see
[data representation](data-representation.md)); the CC requirement table and
verifier (see [CC IR](cc-ir.md)); the erased protocol for bare variables and
abstract constructors (see [polymorphism and erasure](polymorphism-and-erasure.md));
class validation,
entailment, functional dependencies, and evidence elaboration (see
[classes and evidence](../../frontend/type-system/classes-and-evidence.md)) or
its runtime product form (see
[type classes and dictionaries](type-classes-and-dictionaries.md)); effect
semantics (see [effects](effects.md)); or scalar operator semantics (see
[scalars and primitives](scalars-and-primitives.md)).

## Background

**A `RuntimeRep` is not a function of the source type alone.** The source type
is representation-polymorphic wherever a type variable, an abstract
constructor, a class dictionary, an effect, or a newtype is involved. A
`forall a. a -> a` applied to an `Int`, a value of type `f a` whose `f` is
`Effect`, and a `newtype` over `Int` all have a source type that cannot by
itself say how the value is laid out at a given boundary.

**Industry practice names the pieces separately.** GHC represents a value of
kind `TYPE r` through its `RuntimeRep` `r`; a coercion site carries that
representation, `Any` is the uniform boxed inhabitant, dictionaries are
ordinary values, `IO` is a state token, and `unsafeCoerce#` is the primitive
representation coercion. Swift gives a function value a calling convention and
generates a *reabstraction thunk* when the convention changes, with a witness
table for dictionaries. Java erases generic type arguments and restores call
compatibility with bridge methods. Koka elaborates type-class and effect
operations to explicit *evidence*. These are the same idea: choose a
representation when a value is produced, keep the choice as compiler evidence,
and adapt explicitly where the requirement changes.

**This repository already has the two halves and must join them.** The frontend
produces checked evidence
([classes and evidence](../../frontend/type-system/classes-and-evidence.md),
[type inference](../../frontend/type-system/type-inference.md)); the backend
owns representation ([CC IR](cc-ir.md), [MIR](mir.md)). What was missing is a
single contract for how the two meet, which is why per-feature rules appeared.

## Model

```text
RuntimeRep =                   -- the target-neutral runtime representation
    Scalar(Int | Number | Boolean | Char | Unit)
  | Box(Scalar)              -- one-field struct that lets a scalar inhabit Erased
  | Reference(NominalRef)    -- GC struct/array of a known nominal shape
  | Aggregate(CanonicalKey)  -- canonical array/product layout
  | Closure(Signature)       -- callable value: code ref + capture array
  | Erased                   -- the uniform non-null anyref slot

RepresentationPolicy =       -- the RuntimeRep a producer chose for an erased value
    Boxed(Scalar)
  | Nominal(NominalRef)
  | AggregateLayout(CanonicalKey)
  | Callable(Signature)      -- includes Function, dictionary methods, effects

Boundary   = Call | Return | Field | Element | Capture | Slot
Evidence   = Instantiation { scheme, use, substitutions }
           | Subsumption   { actual, expected }
           | Dictionary    { selected_instance | given }
           | Coercion      { proof }
           | EffectPlan    { import, payload }
ConversionPlan = { boundary, producer, consumer, operations }
Adaptation = Identity | Box | Unbox | Cast | AggregateConvert | Thunk | Coerce

-- Layout is the target-specific realization of a RuntimeRep. One total mapping
-- per target; a RuntimeRep used by a target must have a layout.
HeapLayout = Wasm GC heap layout        -- the language heap, owned by P9
AbiLayout  = Canonical ABI byte layout  -- the boundary, owned by the ABI docs
heap_layout : RuntimeRep -> HeapLayout
abi_layout  : RuntimeRep -> AbiLayout
```

`RuntimeRep` is this compiler's analogue of GHC's `RuntimeRep`. GHC's is a kind
because it has representation polymorphism (`TYPE r`) in the source; this
compiler has none, so `RuntimeRep` is an internal descriptor used by lowering,
not a source or type-system kind. It is target-neutral; a `Layout` is its
target-specific realization. They are two levels of one model, joined by a
total mapping per target.

`Erased` is a *representation*, not a loss of information: it is the uniform
anyref slot. The information the producer had before the value entered that slot
is its `RepresentationPolicy`, carried as evidence. Recovery is valid only
against the policy that stored the value; the consumer's concrete type does not
establish it.

## Design

### Four facts, four owners

1. **Source relation.** Whether a use is a legal instantiation of a scheme, a
   subsumption, a selected dictionary, a `Coercible` proof, or a classified
   effectful import. Owned by checking (P5/P6); it is `Evidence`.
2. **Producer policy.** The representation the value's producer chose. Owned by
   the definition and, for a constructor, by that constructor's **representation
   owner**. It is fixed wherever the value is produced, not at its use.
3. **Consumer representation.** The representation the use requires. Owned by
   the checked type at the boundary.
4. **Layout.** The target-specific realization of a `RuntimeRep`, produced
   by the one representation-to-layout mapping. Owned by P9
   ([MIR](mir.md), [data representation](data-representation.md)).

No stage may substitute for a fact it does not own. In particular, a consumer
never infers the producer policy from its own type, and a representation owner
never inspects a consumer.

### Representation owners

A **representation owner** maps a source type constructor plus its checked
instantiation arguments to a `RepresentationPolicy`. The owners form one
registry, not a set of private special cases:

| Constructor | Policy |
| --- | --- |
| `Function` | `Callable(arrow parameters, result)` |
| `Array` | `AggregateLayout(canonical array key)` |
| a data type | `Nominal(variant)` with declared field templates |
| a newtype | the policy of its declared field template |
| the trusted `Effect` | `Callable([state token], payload)` |
| a bare type variable | `Erased`; the value keeps whatever policy it entered with |

The registry belongs to the Core-to-CC boundary (below). CC itself does not know
a source constructor, key behavior on a HIR identity, or derive a protocol by
searching signatures.

### Generic aggregate RuntimeReps

`Array` and a closed record are representation owners too, but a type argument
can appear inside them. Their `RuntimeRep` is **canonical**: it does not depend
on the instantiation of that argument. A bare variable `a` is `Erased`; an
application `f a` headed by an unresolved variable is `Erased`, because its
storage constructor is unknown.

| Source type | Canonical `RuntimeRep` |
| --- | --- |
| `a` | `Erased` |
| `f a` (head a variable) | `Erased` |
| concrete `Array Int` | the specialized `Aggregate([Integer])` |
| `Array a` | `Aggregate([Erased])`, the canonical generic array |
| `Array (Array a)` | an aggregate whose element references the canonical `Array a` |
| concrete `{ x :: Int }` | the specialized `Aggregate` product |
| `{ x :: a }` | a canonical product with an `Erased` field |
| a parameterized data type | one nominal variant, each field in its declared template's normalized shape |

Normalization is structural and cycle-safe, and distinguishes a declaration
**template** (its bound variables abstract) from an **actual** type (the
boundary substitution applied). A closed actual type is specialized; a variable
the substitution does not resolve stays abstract and uses the template rule.
Canonical keys are: arrays by normalized element shape, closed records by sorted
`(label, normalized field shape)` pairs, and variants by declaration identity.
Equal keys intern to one representation; P9 assigns one `DefinedTypeId` per
reachable representation and does not merge by physical shape. This preserves
[DEC-07](../../../decision/DEC-07-runtime-representation-for-parameterized-adts.md)'s
single erased layout for a parameterized data type. Open rows have no canonical
product and remain unsupported.

### Aggregate conversion

When two normalized shapes meet at a typed boundary, the planner builds one
`ConversionPlan`; the concrete layouts it produces are owned by the target
(see [data representation](data-representation.md)). The aggregate leaves are a
`ProductMap` (field-wise reconstruction) and an `ArrayMap` (element-wise
reconstruction), each recursing into its fields or elements. Reconstruction
allocates a fresh destination and converts each field or element: it never
mutates the source and never uses a nominal `ref.cast` between two different
aggregate layouts. A reference `Cast` recovers an object only to the layout its
producer established.

Conversions are inserted at every boundary where the actual and required shapes
differ: parameterized ADT construction and projection, record construction,
access and update, array construction, read and update, direct-call arguments
and returns, higher-order adapters, and captures. Linked source modules are
merged into one Core program before P8, so they share canonical keys and need no
module ABI adapter. Because arrays and records have no observable identity or
mutation, a conversion may duplicate physical sharing while preserving values;
if the language later exposes pointer identity, mutation, or cyclic aggregates,
the plan must add an identity-preserving memo before those features are enabled.

### One model, two levels: RuntimeRep and Layout

This is one model, not two. It has a **target-neutral level** — the `RuntimeRep`
a value has at a boundary, and the conversion plan between two `RuntimeRep`s —
and a **target-specific level** — the `Layout` that realizes a `RuntimeRep` on a
target. The levels are joined by a **total mapping per target**: every
`RuntimeRep` a target uses has exactly one layout. P9 owns the Wasm GC
`heap_layout`; the ABI documents own the canonical `abi_layout`. A complete
functional backend is the target-neutral model plus a total, verified mapping to
each target it emits.

A `RuntimeRep` classifies how a value is used and converted at a boundary; a
`Layout` fixes the concrete types and bytes that realize it. The target has
exactly two layout domains:

- the **language heap** is Wasm GC. Its concrete types, field offsets and
  mutability, tag encoding, box structs, the closure struct and its capture
  array, and the defined-type table are planned by P9 and fixed in
  [MIR](mir.md) and [data representation](data-representation.md). This is
  **MIR layout**: the `ValueType`/`RefType`/`DefinedType` table and the GC
  layouts built from CC requirements.
- the **Canonical ABI boundary** is linear memory. Its byte layout — computed
  offsets, alignment and padding, `(pointer, length)` pairs, the scratch return
  area, and `cabi_realloc` — is owned by
  [canonical ABI and WIT](../wasm/canonical-abi-and-wit.md) and
  [linear memory and the canonical ABI boundary](../wasm/linear-memory-and-canonical-abi-boundary.md).

A concrete value converted for a WIT call is an instance of this model at the
ABI boundary: the consumer representation is the canonical exchange shape and
the adaptation is the canonical conversion; the ABI documents own the bytes.
Erased values and generic aggregates never cross that boundary — they are
recovered to a concrete representation first.

The layout obligations the model relies on are what let it plan a conversion
without owning a layout:

- the same requirements name the same runtime shape: representation is
  deterministic and independent of source identity;
- aggregates are canonically keyed, so structurally equal aggregates share one
  representation while physically equal but nominally distinct layouts stay
  distinct (see [Generic aggregate RuntimeReps](#generic-aggregate-runtime-reps));
- a `Box` is one field, and `Erased` is a non-null, untagged reference;
- a `Closure` is `{ funref, capture-array }` with one uniform nullable-reference
  capture array, so its type does not depend on its captures; and
- a variant is one representation per source sum, with stable tags.

Offsets, type indices, sizes, alignments, and addresses are layouts, not
representations, and never appear in the model.

### One planner

Every boundary uses one planning operation and one emission step; the model is
defined here and its algorithm is specified under [Algorithms](#algorithms).
Planning chooses an adaptation from the checked relation and the two
representations; it never searches for one.

### Everything is an instance

The contract is one, and each functional topic is one instance of it:

- **Erasure** ([polymorphism and erasure](polymorphism-and-erasure.md)): bare
  variables are `Erased`; a policy is `Boxed`, `Nominal`, `AggregateLayout`, or
  `Callable`. Box, Unbox, Cast, and Thunk implement it.
- **Aggregates** (see [Generic aggregate RuntimeReps](#generic-aggregate-runtime-reps)):
  a canonical array or product; a changed shape is an `AggregateConvert` with a
  recursive `ArrayMap`/`ProductMap` plan.
- **Callables** ([CC IR](cc-ir.md)): a `Closure(Signature)`; a changed
  signature is a `Thunk`. This is Swift's reabstraction thunk.
- **Dictionaries** ([type classes and dictionaries](type-classes-and-dictionaries.md)):
  a `Nominal` product of method closures; method use is `ProductGet`, and the
  `Dictionary` evidence names the selected instance. There is no dictionary
  adaptation because the dictionary is already an ordinary value.
- **Effects** ([effects](effects.md)): the `Effect` representation owner
  supplies `Callable([state token], payload)`; `pure`, `bind`, `runEffect`, and
  `trap` are its operations. There is no Effect-specific conversion path.
- **Coercion and newtypes** ([roles and coercions](../../../implementation/frontend/roles-and-coercions.md)):
  a `Coercion` proof justifies `Identity` or a newtype's field policy. The
  checked coercion is the intrinsic `Safe.Coerce.coerce`; the unchecked cast is
  the primitive `Unsafe.Coerce.unsafeCoerce`, the `unsafeCoerce#` analogue.
- **Partial application** ([CC IR](cc-ir.md#partial-application-and-erased-adapters)):
  a `Callable` value applied to fewer arguments produces a new `Callable` value
  under the same policy; it is the standard partial-application closure.
- **Canonical ABI** ([canonical ABI and WIT](../wasm/canonical-abi-and-wit.md),
  [linear memory](../wasm/linear-memory-and-canonical-abi-boundary.md)): a
  concrete value converted for a WIT call takes the canonical exchange
  representation; the ABI documents own its byte layout.

### Evidence and policies travel as a side table

Checked evidence and representation policies reach P8 through an explicit
Core-to-CC **side table**, the same boundary discipline
[`ExternalBindings`](../00-ir-boundaries.md) uses for WIT bindings. They are not
read ambiently from the Core type arena, and they are not keyed by a HIR
identity inside CC.

- **Produced by** checking and the representation owners.
- **Consumed by** P8's planner, which emits explicit operations and signatures.
- **Never reconstructed** from a consumer type, a declaration arity, or a search
  over signatures. Missing evidence is a reported unsupported boundary.

### What this forbids

- runtime type tags on erased values (dictionary and dictionary-selection
  evidence are compile-time);
- a dedicated IR node, closure kind, or runtime object per feature
  (dictionaries and effects are ordinary products and closures);
- two mechanisms for the same constructor (for example a token table *and* a
  signature-prefix derivation for callables);
- deriving a producer policy from arity or from the consumer's concrete type;
- using a HIR/Core identity as a semantic key inside CC; and
- treating representation equality as nominal identity — structurally equal
  signatures share one `SignatureId`.

## Algorithms

### Planning and emission

```text
plan(evidence, producer_policy, consumer_representation):
    validate boundary ownership, scope, direction and endpoint positions
    if producer_policy and consumer_representation agree:
        Identity
    else:
        recursively plan scalar, reference, callable and aggregate conversions
    reject a recovery whose producer policy cannot be established

emit(plan, value):
    emit the plan's exact source/target representations and signatures
    verify generated adapter bodies, captures and calls
```

### Choosing the adaptation

```text
adapt(value, plan):
    bare variable slot, scalar        -> Box
    bare variable slot, reference     -> Cast(Erased)
    aggregate layouts differ          -> AggregateConvert(plan)
    value is Erased                   -> recover the box, layout or callable
                                         signature the policy names, then adapt
    callable signatures differ        -> Thunk(plan)     -- reabstraction
    representation-preserving proof   -> Identity | Coerce
    otherwise                         -> source-spanned unsupported boundary
```

### Aggregate conversion plans

```text
convert(source, destination):
    normalize both with the boundary's template/actual role
    require the checked boundary to prove them compatible
    if shapes agree: Identity
    if the destination is a bare variable: Box a scalar, or Cast a reference
    if the source is a bare variable: Unbox a scalar, or recover the reference
    if both are arrays with corresponding elements:
        ArrayMap(source, target, convert(elements))
    if both are closed records with the same labels:
        ProductMap(source, target, convert(each field in canonical order))
    otherwise: report an unsupported aggregate conversion at the source span
```

The plan is target-neutral and carries no Wasm type, offset, or unverified
cast. The concrete allocation, loop, and initialization it lowers to are owned
by [data representation](data-representation.md) and [MIR](mir.md).

### Edge cases

- **Direction.** A parameter position is contravariant: the producer plan and
  the consumer plan reverse relative to a result position. Evidence preserves
  direction; it is never reused for the mirrored pair.
- **Nested boundaries.** A plan inside an `ArrayMap` or `ProductMap` retains its
  element or field position and its own scoped evidence.
- **Fixed payloads.** `f Unit` does not make the abstract constructor concrete;
  the producer policy of the value returned through the generic method still
  governs recovery.
- **Missing evidence.** An abstract boundary without a producer policy is
  reported with its source span, never resolved by arity or by a signature
  search.

## Code map

This document is the shared contract; it has no runtime code of its own. The
implementation target is split across the topic owners:

```text
cc/lower/conversion/   the planner and emission: scalar, reference, callable and
                       aggregate leaves, and the generated adapters
cc/layout/             constructor policies for the local constructors
                       (Function, Array, data, newtype) and signature interning
cc/verify/             conversion-plan endpoint, capture and call-signature checks
mir/layout/            the physical layout each RuntimeRep maps to
mir/lower/             lowering of the emitted adaptations
abi/ and wit/          canonical-ABI adaptation on concrete signatures only
```

The Core-to-CC **side table** that carries `Evidence` and `RepresentationPolicy`
is a boundary input, carried beside CC like `ExternalBindings`; it is not a
field of the CC model and never appears in an emitted operation. The frontend
owners of `Evidence` are
[classes and evidence](../../frontend/type-system/classes-and-evidence.md) and
[type inference](../../frontend/type-system/type-inference.md).

## Invariants and verification

- Every value has exactly one `RuntimeRep` at its definition and at each
  boundary; the planner is the only place a representation changes.
- An `Erased` value is recovered only to a policy recorded by its producer.
- Every adaptation is justified by `Evidence` and checked at both endpoints.
- A representation owner maps a constructor to exactly one policy; there is no
  fallback obtained by searching signatures or by arity.
- CC and MIR contain no target type above P9 and no runtime type tag anywhere.
- The verifiers of [CC IR](cc-ir.md) and [MIR](mir.md) check the emitted
  operations; the boundary side table is validated in both directions, as
  `ExternalBindings` is.

## Worked examples

### A bare variable at a concrete use

`identity :: forall a. a -> a` used at `Int`. Evidence is the instantiation
`a := Int`; the producer policy at `Int` is `Boxed(Int)`. The plan boxes the
argument, `Cast`s to `Erased`, calls the `Callable([Erased], Erased)` body, then
casts to the box and unboxes the result. This is
[polymorphism and erasure](polymorphism-and-erasure.md)'s identity fixture; the
same symbol serves every instantiation.

### An abstract constructor with a fixed payload

`again :: forall f. Bind f => f Unit -> f Unit` used at `Effect`. Evidence names
the selected `Bind Effect` dictionary and the instantiation `f := Effect`; the
`Effect` owner's policy is `Callable([state token], Unit)`. The generic body
still stores that callable value; the consumer recovers the policy and runs it.
No cast onto the consumer's signature is used, which is what the fixed-payload
case verifies.

### A representation-preserving coercion

`coerce (UserId (Raw 47)) :: Int` over two visible newtypes. The `Coercion`
proof establishes representational equality; the planner lowers the chain of
newtype field policies to `Identity`, so no wrapper object is allocated and no
cast is needed.

### A generic aggregate across a concrete boundary

`data Wrap a = Wrap (Array a)`, `wrap :: Array Int -> Wrap Int`, and
`unwrap :: forall a. Wrap a -> Array a`, with
`main = arrayIndex (unwrap (wrap [40, 42])) 1`. The concrete literal is
`Array Int`; the declared field template `Array a` has the canonical
`Array(Erased)` RuntimeRep. Construction plans an `ArrayMap` with an
`Integer -> Erased` element conversion, boxes each element into the canonical
array, and stores that reference in the variant's canonical field. `unwrap`
reads the canonical layout directly, and the outer boundary maps it back to
`Array Int`. No runtime tag and no nominal array cast is used. A bare-variable
field such as `data Hold a = Hold a` instead stores the concrete reference
directly in the erased slot and needs no array map.

## Boundaries and interfaces

- **From checking (P5/P6):** checked boundary `Evidence`, plus the representation
  policies of constructors and definitions, as a side table.
- **To CC (P8):** `RuntimeRep` requirements and explicit `Adaptation`
  operations; no source identity and no physical layout.
- **To MIR (P9):** the requirements and plans; P9 selects the physical layout
  and lowers the operations. MIR does not re-plan or invent a policy.
- **To the ABI boundary:** the canonical exchange shape is the consumer
  representation; its byte layout is owned by
  [canonical ABI and WIT](../wasm/canonical-abi-and-wit.md) and
  [linear memory](../wasm/linear-memory-and-canonical-abi-boundary.md). Erased
  values and generic aggregates never cross it.

## Open questions and future work

- **Registry home.** Whether the representation-owner registry lives beside the
  trusted-library identities or as its own module at the Core-to-CC boundary.
- **Monomorphization.** A specialization pass may remove boxes and casts at
  statically known instantiations while preserving the erased representation as
  the fallback; it must not become a second correctness mechanism.
- **Open rows.** Open-row records still need a canonical policy and conversion
  contract before they join the registry.

## Implementation notes

Checked instantiation evidence and the representation policies of constructors
reach P8 through the Core-to-CC side table (`psrs-backend/src/boundary.rs`),
beside CC the way `ExternalBindings` carries the WIT boundary. P8 reads the
checked relation from the immutable source program and the registered
`RepresentationPolicy` per constructor; it does not reconstruct the relation
from the Core type arena, key a semantic decision on a HIR identity, or derive a
policy by enumerating signatures. The registry registers `Function` (whose fixed
parameters are its checked instantiation arguments) and the trusted `Effect`
(whose fixed parameter is its runtime token); an unregistered constructor is
reported rather than resolved by arity or a signature search. Protocol
signatures are interned once per registered callable constructor as the concrete
calling convention with the payload erased. The checker's relation itself is a
single `TypeMatcher::relate` parameterized by a `Variance` (subsumption or
invariant); P8 consumes the `Instantiation` evidence it produces and never
re-derives it.

The effect token is the compiler-owned opaque state token
(`psrs_hir::TypeId::STATE_TOKEN`) that effect lowering threads through each
`Effect` closure, the `State# RealWorld` analogue. It has no source spelling and
one uninspectable value, so no later pass can treat it as an `Int`; its runtime
shape is a scalar because the synchronous, single-threaded effect carries no
payload ([effects](../../../implementation/backend/effects.md)).

## References

- Reynolds, *Types, Abstraction and Parametric Polymorphism* (1983).
- Crary, Weirich, and Morrisett, *Intensional Polymorphism in Type-Erasure
  Semantics* (2002).
- Wadler and Blott, *How to Make Ad-hoc Polymorphism Less Ad hoc* (1989);
  Peyton Jones, Jones, and Meijer, *Type Classes: Exploring the Design Space*
  (1997).
- Eisenberg and Peyton Jones, *Levity Polymorphism* (2017); GHC `RuntimeRep`,
  `Any`, and `unsafeCoerce#`.
- Launchbury and Peyton Jones, *State in Haskell* (1995); GHC `IO` as
  `State# RealWorld`.
- Swift function calling conventions (`@convention(thin)`/`thick`),
  reabstraction thunks, and witness tables.
- Java type erasure and bridge methods.
- [IR boundaries](../00-ir-boundaries.md), [functional core](../../frontend/semantics/functional-core.md),
  [classes and evidence](../../frontend/type-system/classes-and-evidence.md),
  [CC IR](cc-ir.md), [MIR](mir.md),
  [polymorphism and erasure](polymorphism-and-erasure.md).
- [DEC-17](../../decision/DEC-17-representation-and-evidence.md) records this
  model as a durable decision; [DEC-15](../../decision/DEC-15-unified-type-representation.md)
  is the type-spine counterpart.
