# Polymorphism and Erasure

**Feature:** F-02  
**Status:** Stable
**Prerequisites:** [functional core](../../frontend/semantics/functional-core.md), [CC IR](cc-ir.md),
[MIR](mir.md), and [data representation](data-representation.md); parametric
polymorphism (Reynolds) and type-erasure semantics; the Wasm GC type system
with typed function references. Read [IR boundaries](../00-ir-boundaries.md)
first.  
**Summary:** Rank-N polymorphism is erased at runtime: a type variable denotes
one uniform, non-null `eqref`, never a type tag; an aggregate type containing a
variable may instead have a canonical aggregate layout. Checked uses retain
compile-time instantiation, and class constraints become explicit dictionaries. Because Wasm
`call_ref` names one exact function type, a concrete closure cannot cross a
polymorphic boundary directly; the design inserts representation-directed
boxing, unboxing, casts, and generated function adapters. This document
specifies that erased representation, the concrete-versus-erased boundary, and
the adaptation operations. Generic arrays and closed records use the recursive
layout and conversion rules in
[generic aggregate erasure](generic-aggregate-erasure.md).

## Scope

This document owns the erased representation for polymorphic values, the
distinction between concrete and erased representation requirements, the
semantics of the adaptation operations, the generated function adapters used at
higher-order boundaries, and the closure capture rules that follow from
erasure. This includes applications of abstract constructors (`f a`) and
methods transported through ordinary class dictionaries. It does not own concrete scalar and GC layouts (see
[data representation](data-representation.md)), the MIR type model and verifier
(see [mir](mir.md)), type-class elaboration and dictionary construction (see
[type classes and dictionaries](type-classes-and-dictionaries.md)), scalar
operation semantics (see [scalars and primitives](scalars-and-primitives.md)),
or the byte-oriented ABI boundary (see
[canonical ABI](../wasm/canonical-abi-and-wit.md)).
The frontend owns the legality of nested quantifiers, subsumption, and skolem
scope in [type inference](../../frontend/type-system/type-inference.md).
Backend erasure consumes that checked contract; a representation adapter never
authorizes a monomorphic value at a universally quantified source boundary.

## Background

**Parametricity.** Reynolds (*Types, Abstraction and Parametric Polymorphism*,
1983) showed that a value of type `forall a. a -> a` is uniform in `a`: it may
use its argument only at the abstract type `a`, so it can inspect no part of the
argument's representation. No runtime type test is needed to implement it. This
is the semantic fact the erased representation relies on.

**Type-erasure semantics.** Crary, Weirich, and Morrisett (*Intensional
Polymorphism in Type-Erasure Semantics*, 2002) formalize type-erasure
semantics: type abstractions and applications have no runtime counterpart, and
a polymorphic value has a single runtime representation. Intensional
operations such as type case and type passing are a *feature added on top* of
the erased core; a language whose terms perform no runtime type analysis needs
none of them. PureScript with dictionary-passing type classes is exactly such a
language at the term level.

**Dictionary passing.** Wadler and Blott (1989) and Peyton Jones, Jones, and
Meijer (1997) show that type-class evidence can be elaborated to ordinary
explicit values: a dictionary is a record of method values, and a constrained
polymorphic function is a function taking a dictionary. There is no separate
runtime type representation; the dictionary is a product of closures and is
handled by the aggregate and closure rules. The official PureScript compiler
uses this model, and it is the semantic reference here.

**Wasm has no generic function type.** WebAssembly 3.0 gives typed function
references, and `call_ref` names one exact function type. Function types are
invariant in their parameters and results: a closure with code signature
`(i32) -> i32` is not a closure with code signature `(eqref) -> eqref`. Mapping
every type variable to `eqref` is therefore necessary but not sufficient.
Higher-order polymorphic boundaries need an explicit adapter value whose own
code signature is the erased one.

**Terminology.** An *erased value* is the value of an abstract type variable,
whose runtime representation is the uniform reference. A type such as
`Array a` contains a variable but is itself a generic aggregate; see
[generic aggregate erasure](generic-aggregate-erasure.md). *Boxing*
allocates a wrapper for a scalar so it can inhabit the erased representation;
*unboxing* projects it back. A *concrete* value is one whose source type is
variable-free and therefore has a specialized representation. An *adapter* is a
generated closure that converts between a concrete and an erased function
signature.

## Model

### Representation requirements

CC carries target-neutral requirements, not Wasm types. The relevant model is:

```text
ValueShape = Integer | Boolean | Number | String | Reference(Reference)
Reference  = { nullable: bool, heap: RefShape }
RefShape   = Repr(ReprId) | Aggregate | Erased | Closure(SignatureId)
Signature  = { parameters: [ValueShape], result: ValueShape }
```

The erased requirement is exactly
`Reference { nullable: false, heap: Erased }`. The predicate
`is_erased_value_type` recognizes it. A bare type variable has this shape;
containing a type variable does not by itself make an entire aggregate value an
`Erased` reference. P8 recursively normalizes arrays, closed records, ADT
fields, and function signatures. In particular, `Array a` uses the canonical
generic array shape and a dependent closed record uses a canonical product;
their element or field values may use `Erased`. See
[generic aggregate erasure](generic-aggregate-erasure.md) for the normalization
and layout conversions. A function value uses `Closure(SignatureId)`, whose
`Signature` records the normalized `ValueShape` for each parameter and result.

### Concrete and erased signatures

A polymorphic function such as `forall a. a -> a` has the CC signature
`([Erased], Erased)`. P9 realizes that requirement as a MIR function type with
a non-null `(ref struct)` receiver followed by one `eqref` per parameter and an
`eqref` result:

```text
forall a. a -> a        =>  (ref struct, eqref) -> eqref
forall a. Array a -> Array a => (ref struct, (ref $array_erased)) -> (ref $array_erased)
Int -> Int              =>  (ref struct, i32) -> i32
Number -> Number        =>  (ref struct, f64) -> f64
(Int -> Int) -> Int     =>  (ref struct, (ref $closure)) -> i32
```

The receiver is a non-null `(ref struct)`; closure values are cast to the
concrete closure struct before their code reference is projected, so the
signature does not name the closure struct type (see
[data representation](data-representation.md)). Distinct Core function types
that erase to the same `Signature` share one `SignatureId`: P8 interns
structurally equal `Signature` values, and P9 maps each `SignatureId` to one MIR
func type. Two nominally different source types with the same erased shape must
therefore not produce incompatible `ref.func`/`call_ref` pairs.

### Adaptation operations

Two CC operations cross the concrete/erased boundary:

- `RepresentationCast { destination, value, reference }` adapts `value` to the
  requirement `reference`.
- `RepresentationTest { destination, value, reference }` tests whether `value`
  satisfies `reference`.

`RepresentationTest`/`RepresentationCast` are reserved for erased
representation adaptation and are **not** used for constructor dispatch; sum
dispatch uses the tag-carrying variant representation of
[CC IR](cc-ir.md). The
CC verifier accepts an adaptation only when the source value is erased or the
destination requirement is erased. P9 lowers `RepresentationCast` to `RefCast`
and `RepresentationTest` to `RefTest`, each carrying the concrete target
`RefType`.

### Quantified function values

`ForAll(variables, body)` binds variables at its position in the checked type.
A polymorphic function parameter, record or constructor field, returned value,
or closure capture retains one uniform definition representation. Each use
records its instantiated type and adapts arguments and results between that
use and the definition representation. Instantiation adds no runtime type
argument or type tag. A constrained polymorphic value additionally receives
the dictionary parameters produced by frontend evidence elaboration.

For example, a parameter `f :: forall a. a -> a` has an erased entry signature.
The uses `f 42` and `f true` call that same value with the appropriate scalar
boxes and recover results using their checked instantiations. Storing or
capturing `f` retains the generic closure signature, rather than selecting one
of those concrete uses as its storage type.

Derive callable arity inside one quantifier boundary at a time. For
`Int -> (forall a. a -> a)`, the outer function takes one `Int` and returns
a polymorphic closure. Flattening through the result's `ForAll` would change
that contract into a two-argument function and is forbidden. Opening the
outermost `ForAll` of a function value exposes that value's own body signature;
it does not remove inner quantifier boundaries. This rule also applies to
partial applications and generated adapters. A representation closure is the
same kind of boundary: `Effect (a -> b)` lowers to a closure that takes the
runtime token and returns a function, and flattening that function into the
effect closure is forbidden ([effects](effects.md)).

### Checked boundaries and stored representation contracts

Keep three facts separate until a representation conversion has been planned:

1. The source definition scheme and the checked type at this particular use.
2. The representation actually produced or stored by the definition, including
   a closure's complete parameter/result signature.
3. The representation required by the consumer.

Source checking owns scheme instantiation, subsumption, binder scope and field
compatibility. P8 consumes that result together with representation mappings;
it does not extend source compatibility to accommodate rewritten types. A
checked relation is attached to a particular boundary and immutable source
artifact. Type IDs alone, declaration arity, or a set of successful matcher
node pairs are insufficient: evidence must preserve relation direction,
quantifier scope, substitutions and the argument/result or field position to
which it applies. Contravariant parameters reverse the checking direction;
they do not make the evidence interchangeable with arbitrary endpoint pairs.

The planning inputs have the following conceptual shape; these are compiler
contracts, not runtime fields:

```text
Boundary = CallFrame | Return | Field | Element | Capture
CheckedBoundary = { source_artifact, boundary_position, scoped_type_relation }
RepresentationView = { source_use_or_generated_plan, value_shape, stored_protocol }
ConversionPlan = { checked_boundary, producer_view, consumer_view, operation_tree }
```

`source_use_or_generated_plan` identifies either a checked source type in its
binder environment or the plan that authorized a synthetic endpoint. An erased
value shape does not remove `stored_protocol` from planning. CC receives the
resulting explicit operations and signatures; it does not receive a runtime
constructor identity or type witness.

A bare variable `a` can transport an existing reference object unchanged. Its
recovery contract is the representation established when the value entered
that slot. An application `f a` also hides its constructor. A generic method
may produce a new value of that application, so recovering it requires the
constructor transport contract shared by the method implementation and its
generic consumers, rather than a guess from the consumer's concrete type.
The same requirement applies to `f Unit`; a fixed argument does not establish
the stored calling convention of a value returned through a generic method.

At a checked instantiation of `f`, the representation owner supplies that
constructor's transport protocol. For a callable constructor it specifies the
fixed parameters and the result protocol; for arrays it uses the canonical
element layout; ADTs retain their declared field storage contracts. Partial
constructor applications retain their fixed arguments. This is representation
lowering, not runtime instance selection. It neither requires a runtime type
tag nor authorizes erasing every constructor argument unconditionally.

Dictionary selection determines the implementation to call. A shared generic
body and that implementation still have definition ABIs. Direct calling or
specializing a known dictionary may remove a boundary, but the unspecialized
path must satisfy the same transport contract. Dictionary fields, callbacks,
returns, captures and ordinary functions use the common conversion planner.

Source types and checked boundary evidence remain available until P8 emits
explicit conversion operations with exact physical endpoints. Representation
lowering must not overwrite the authoritative source type arena and then
reconstruct source relations from closure signatures. A synthesized adapter
endpoint can be representation-only: its validity follows from its conversion
plan and signature, without inventing a source constructor for it.

### Erased values and boxes

The erased representation is `eqref`, a non-null reference. Concrete values
enter it as follows:

| Concrete shape | Erased entry | Erased exit |
| --- | --- | --- |
| `Integer`, `Boolean` | one-field i32 `Box` struct | `ref.cast` to the box, then `struct.get` of field 0 |
| `Number` | one-field f64 `Box` struct | `ref.cast` to the box, then `struct.get` of field 0 |
| `String` | `ref.cast` to `eqref`, no allocation (a GC string is an `eq` value) | `ref.cast` to the string representation |
| GC reference (`Repr`, `Aggregate`, `Closure`) | `ref.cast` to `eqref`, no allocation | `ref.cast` to the concrete reference |
| already erased | identity | identity |

The GC-reference row applies when the value crosses a bare type-variable
boundary or keeps the same runtime object shape. A value whose type is
`Array a`, or a dependent closed record, may need an element-wise or
field-wise layout conversion before it can use its canonical generic shape;
an `eqref` cast alone is not that conversion.

`Boolean` is boxed in the integer box on the erased path, and a `String` is a GC
`eq` value that erases and recovers by cast, so all patterns round-trip without
turning a string into an integer. The i31 shorthand is used only for closure
*captures*, not
for the general erased protocol (see [data representation](data-representation.md)).
The empty-erasure case is an identity when both endpoints share the same stored
representation contract. Equal `Erased` shapes alone do not prove that two
hidden closure or aggregate protocols agree.

### Dictionaries

Type-class evidence is an ordinary explicit value at the Typed Core boundary
following the official dictionary-passing model. Its runtime representation is
chosen by the normal product and closure rules; it is not a special Wasm
type-system feature and does not use runtime type tags. The design of
elaboration and dictionary construction lives in
[type classes and dictionaries](type-classes-and-dictionaries.md).

### Invariants

- Every erased value has the exact shape
  `Reference { nullable: false, heap: Erased }`.
- An aggregate type that contains a type variable is normalized by its
  aggregate constructor; it is not automatically the `Erased` shape.
- `RepresentationTest`/`RepresentationCast` adapt only when the source is erased
  or the destination is erased.
- Representation casts never convert between distinct nominal array or record
  layouts; those conversions use the aggregate rules.
- A concrete closure is never passed where a different concrete closure
  signature is expected without an adapter.
- Structurally equal `Signature`s intern to one `SignatureId`; one
  `SignatureId` maps to one MIR func type.
- An adapter captures its source function once and evaluates it before any
  invocation.

## Design

### Chosen representation: representation-directed erasure

Values of a bare abstract type use the erased ABI; values with a known concrete
type stay specialized. Aggregate types are normalized recursively, so a
generic function's `a` parameter uses `Erased`, while its `Array a` parameter
uses the canonical generic array. A call site with a concrete instantiation
converts between that canonical shape and its specialized shape as specified
in [generic aggregate erasure](generic-aggregate-erasure.md). There is exactly
one erased representation for abstract values, and it carries no source type
identity.

This is viable because:

- the erased core performs no runtime type analysis, so no tag is required;
- concrete scalars are boxed only at the erased boundary; and
- a generic function body is compiled once against the erased signature,
  independent of its instantiations.

### Function adapters

When a concrete function value is passed to a parameter whose source type is a
polymorphic function type, P8 generates an adapter closure. The adapter has the
erased (target) signature, captures the original concrete closure as capture 0,
and on each call:

1. casts the captured erased reference back to the concrete closure signature;
2. unboxes each erased argument that the concrete signature expects concretely;
3. calls the concrete closure indirectly;
4. boxes the concrete result if the erased signature requires it; and
5. returns.

A function value retains its normalized `Closure(SignatureId)` even when its
source type contains variables. Erasure of its abstract arguments/results does
not require erasing the closure itself. Uniform erased storage may hide the
closure shape; recovery restores the signature established by the producer,
then adaptation changes the calling convention if necessary. The recursive
conversion plan includes function adapters inside records and arrays, so
correctness does not depend on specialization or the kind of enclosing value.

The reverse direction — a polymorphic function value returned from a generic
function and later invoked at a concrete type — is recorded at the concrete
consumer and adapted in the same way. Evaluation order is preserved: the
original function value is evaluated once and captured, and an argument is
unboxed only when the adapter is invoked.

### Closure captures under erasure

A closure is a GC struct `{ funref, capture-array }` whose captures live in one
uniform array of nullable `eqref` (`array (mut (ref null eq))`), so the closure
type does not depend on capture types. Captures enter the array as follows:

- an `Integer` capture is boxed in the one-field i32 box, and a `String`
  capture is stored as its GC reference;
- a `Boolean` capture is boxed with `i31.new`;
- an `f64` capture is boxed in the one-field number box;
- a reference capture is stored as-is; and
- an erased capture is already `eqref`.

Projection reads the array slot and reverses the corresponding step. This is
consistent with the erased protocol: an erased capture is not boxed twice.

### Rejected alternatives

- **Runtime type tags for every erased value (intensional polymorphism).**
  Rejected: it adds a tag store, tag loads, and tag comparisons to every
  polymorphic boundary, while the type-safe Core already determines the exact
  unboxing operation at each consumer. It also makes representation equality
  depend on a dynamic tag scheme rather than on compile-time requirements.
- **Type passing to recover polymorphism at runtime.** Rejected for the same
  reason, and because Wasm has no generic function type to receive a type
  argument. Dictionaries already carry the operations a type class needs.
- **Blindly mapping a type variable to `eqref` with no adapters.** Rejected: it
  fails for higher-order calls because Wasm function references are invariant
  and `call_ref` names one exact signature.
- **Whole-program monomorphization as the only representation.** Rejected as
  the correctness mechanism: it needs a global view of all instantiations and a
  fallback for instantiations not visible at a boundary, and it makes closures
  and separate modules depend on specialization. It may later be added as an
  optimization that preserves the erased representation as the semantic
  fallback.

## Algorithms

### Normalizing erased components

```text
normalize(ty, substitution):
    never revisit a (TypeId, substitution) pair
    Variable                         -> Erased
    ForAll(variables, body)           -> body shape with bound variables erased;
                                        preserve nested callable boundaries
    Application(variable head, args) -> Erased (unknown storage constructor)
    Array(element)                   -> concrete or canonical array shape;
                                        record element conversion
    closed Record(fields)            -> product of recursively normalized fields
    Parameterized ADT                -> nominal variant; declared template fields
    Function(parameters, result)     -> Closure(Signature(normalize each part))
    concrete scalar or other value   -> its concrete shape
```

The array and record cases are defined by
[generic aggregate erasure](generic-aggregate-erasure.md), including their
conversion plans. An application headed by an abstract constructor, such as
`f a`, has no known aggregate layout and uses the erased value protocol; known
constructors such as `Array a` still retain their canonical layouts. A type
variable nested in an ADT field continues to follow
[DEC-07](../../../decision/DEC-07-runtime-representation-for-parameterized-adts.md).

### Planning a checked representation boundary

All typed boundaries use one planning operation, including direct and indirect
calls, partial applications, returned functions, dictionary fields, aggregate
elements and lifted captures:

```text
plan(checked_boundary, producer_contract, consumer_contract):
    validate boundary ownership, scope and endpoint positions
    obtain source/use relation from the source checking owner
    obtain physical views and transport protocols from representation lowering
    if both contracts agree:
        Identity
    else:
        recursively plan scalar, reference, callable and aggregate conversions
    reject any recovery whose stored representation cannot be established

emit(plan, value):
    emit the plan's exact source/target shapes and signatures
    verify generated adapter bodies, captures and calls
```

Planning retains semantic evidence; emission consumes the completed plan.
Emission must not rerun source matching for representation-only adapter types,
search a module for a plausible signature, or recover directly to the desired
consumer signature. A function conversion first establishes the stored
producer signature, then generates an adapter if the consumer signature differs.
Producer erasure and consumer recovery use the same protocol. For example, an
abstract callable-constructor protocol may transport a closure with a fixed
parameter and erased result; entering it adapts the result before erasure,
and leaving it recovers that closure before adapting to the concrete result.

Representation owners contribute mappings and protocols to this common
operation. An Effect owner supplies the trusted application-to-token-closure
mapping; it does not collect separate call, dictionary or field evidence.
Planning context is explicit and belongs to a boundary. Ambient Effect-specific
matcher state is not a substitute for that context.

### Boxing and unboxing

```text
adapt(value, checked_boundary, stored_contract, consumer_contract):
    validate the checked plan and its endpoint contracts
    if consumer is a bare type-variable slot:
        scalar -> allocate its existing erased box
        reference -> RepresentationCast(value, Erased)
    else if stored and consumer contracts require aggregate conversion:
        AggregateConvert(value, checked recursive plan)
    else if value is erased:
        recover the box, layout or callable signature established by storage
        apply the remaining plan to reach the consumer contract
    else if callable signatures differ:
        generate the checked function adapter
    else if physical shapes and storage protocols agree:
        Identity
    otherwise:
        report a source-spanned unsupported conversion
```

Supplying a value to a bare erased parameter may erase an aggregate reference
without copying it, as in `Hold a`. Supplying it to a generic aggregate such
as `Array a` may need `AggregateConvert` first. Recovery from an erased ADT
field likewise depends on the declared field template: `a` can recover a
concrete reference directly, while `Array a` first recovers its canonical
array and then maps to a concrete array when required. Aggregate conversions
are never implemented as `RepresentationCast`s between distinct nominal
layouts.

Newtypes retain the storage protocol of their declared field template. Before
planning a conversion, transparently unfold newtype endpoints to that template
while preserving their physical value shapes. For `newtype Wrap a = Wrap a`,
`Wrap Int` therefore still stores an erased value; converting it to `Int`
unboxes that value. A function or array field uses the existing function adapter
or element mapping instead. Newtype pattern projection applies the same
template-to-instantiated-field conversion before binding or inspecting the
payload. Unwrapping a newtype never allocates a separate wrapper object.

### Adapter generation

```text
adapt(value, checked_callable_plan):
    source = checked_callable_plan.producer_signature
    target = checked_callable_plan.consumer_signature
    require source and target have equal arity

    adapter:
        captured  = ClosureGetCapture(closure = adapter_closure, index = 0)
        producer  = RepresentationCast(captured, Closure(source))
        args' = emit each checked target-parameter -> source-parameter plan
        result = IndirectCall(producer, source, args')
        return emit the checked source-result -> target-result plan

    emit FunctionRef(adapter, target, captures = [value])
```

This is the equal-arity branch. Curried and eta-expanded adapters segment the
call at quantifier and representation-closure boundaries and use the same
checked plans for each segment; they do not flatten a returned closure into
the producer's own parameters.

The original `value` is named once and captured; the adapter body is verified
against the CC signatures and representations exactly like a source function
before it is added to the module.

### Signature interning

P8 assigns each Core function type a provisional `SignatureId`, computes its
`Signature` bottom-up, and reuses an existing ID when an equal `Signature` was
already seen. P9 then allocates one MIR func type per reachable `SignatureId`.
The result is that equal erased shapes share one call signature, so a
`ref.func` produced for one source function type is callable through the shared
`call_ref`.

## Code map

Erasure spans the concrete layout of boxes and closure captures, the lowering
that emits boxing, unboxing, casts, and adapters, and the verification that
binds them. The modules that own erasure MUST be:

```text
cc/layout/           uniform definition shapes, quantifier-aware callable arity,
                     signature interning and template closure storage
cc/lower/call/       independent use-site calls and application-spine segmentation
cc/lower/erased/     generated function adapters and erased local-value recovery
cc/lower/conversion/ recursive scalar, reference and aggregate conversion plans
mir/layout/          concrete Box{Integer}/Box{Number} structs, the closure
                     struct type, and the uniform nullable-eqref capture array
                     that erased captures inhabit
mir/lower/aggregate/ boxing, projection and array/product reconstruction
cc/verify/           conversion endpoint, capture and call-signature checks
mir/verify/          RefTest/RefCast agreement, closure capture checks, and
                     call-signature agreement at erased boundaries
```

The erased requirement itself is produced by the CC representation model as
`Reference { nullable: false, heap: Erased }`; MIR represents it as the
non-null `eqref` reference (`RefType { nullable: false, heap: Eq }`). No module
may attach a runtime type tag to an erased value.

The source Core checking owner supplies scoped instantiation and compatibility
evidence. P8 representation lowering supplies source-to-physical mappings and
storage protocols. `cc/lower/conversion/` combines those inputs into the common
plan; `cc/lower/erased/` emits its callable leaves. Extracting evidence must
preserve the source checker's acceptance contract. Missing evidence is a
reported limitation, not a reason to silently strengthen or weaken subsumption.

**Required types and helpers.** CC owns representation-directed conversion and
adapter generation. Its callable-shape and adapter entry points are:

```rust
fn function_arrow_parameters(module: &CoreModule, ty: TypeId) -> (Vec<TypeId>, TypeId);
fn adapt_erased_function_value(
    &mut self, value: ValueId, source: TypeId, target: TypeId,
    span: TextRange, assignments: &mut Vec<Assignment>,
) -> Result<ValueId, Vec<BackendError>>;
```

`function_arrow_parameters` opens leading quantifiers of the current value,
stops at a quantified result, and retains that result's separate closure type.
`adapt_erased_function_value` MUST generate the adapter closure of
[Adapter generation](#adapter-generation) with the required target signature,
capture the original value once, and preserve evaluation order.

MIR lowering consumes the checked CC conversion plans. Scalar erasure
allocates the matching `mir/layout/` box; scalar recovery projects exactly that
box. Reference and already-erased conversions add no scalar allocation.
`RepresentationCast` lowers to `RefCast` and `RepresentationTest` to `RefTest`
with the concrete target `RefType`. MIR does not infer source instantiations
or generate a second set of adapters.

`mir/layout/` MUST expose the concrete box, closure, and capture-array types so
boxing and projection agree with the layout table
([data representation](data-representation.md)). `mir/verify/` MUST check that
`RefTest`/`RefCast` operands and targets agree, that `ClosureNew` and
`ClosureGetCapture` boxing matches the concrete box and capture-array types, and
that no rule treats two different function signatures as compatible merely
because both are references.

Signature interning MUST happen above this boundary: structurally equal
signatures map to one `SignatureId` and one MIR func type, so one
`ref.func`/`call_ref` pair serves every instantiation.

## Invariants and verification

The CC verifier:

- validates stored representation contracts and checked conversion-plan
  endpoints before semantic evidence is discharged; equal erased shapes do
  not authorize recovery to an arbitrary signature;
- accepts `RepresentationTest`/`RepresentationCast` only when the source value
  is erased or the destination requirement is erased
  (`cc/verify/adaptation.rs`);
- verifies `AggregateConvert` endpoints and nested plans as specified by
  [generic aggregate erasure](generic-aggregate-erasure.md); and
- checks that direct-call arguments and results exactly match the callee
  `Signature`, closure-call arguments match the closure `SignatureId`, and
  capture count, order, and representations match the lifted function; and
- rejects target types, physical layouts, and Wasm indices in CC.

The MIR verifier then checks the concrete side:

- `RefTest`/`RefCast` operand and target `RefType` agreement;
- `ClosureNew`/`ClosureGetCapture` capture boxing against the concrete box and
  capture-array types;
- exact direct, closure, and reference-call signatures and `ref.func`
  agreement; and
- that no verifier rule treats two different function signatures as compatible
  merely because both are references.

Failure is a compiler bug or an unsupported program, reported with the
operation's source span.

If P8 uses an internal Core-shaped materialization, it is a distinct lowered
representation with a complete structural verifier. Checking source Core and
selected rewritten types alone does not verify its expressions, generated
declarations, suspension wrappers or entry adapter. Removing source verification
from a physical view requires replacing it with these target contracts; it
does not remove the verification obligation.

## Worked example

### Polymorphic identity

The fixture `mir/gc_tests/erased.rs` uses `identity :: forall a. a -> a` with
signature `([Erased], Erased)`. Calling it at `Int` produces the CC sequence:

```text
v0 = 23                              // Integer
v1 = ProductNew Box{Integer}(v0)     // box
v2 = RepresentationCast(v1, Erased)  // ref.cast to eqref
v3 = DirectCall identity(v2)         // (ref struct, eqref) -> eqref
v4 = RepresentationCast(v3, Repr(integer_box))   // recover the box
v5 = ProductGet integer_box(field 0, v4)         // unbox -> 23
```

The `Number` case is identical with the number box and a `NumberToInt`
conversion, and the reference case is an identity cast to `eqref` and back. The
fixture sums `23 + 42 + 5` and the Wasmtime exit code is `70`; the same
`identity` symbol serves all three instantiations.

### Higher-order adapter

A generic `apply :: forall a. (a -> a) -> a -> a` used at `Int` receives a
concrete `Int -> Int` lambda. The declared parameter type `a -> a` itself
depends on a type variable, so it erases to `([Erased], Erased)`. P8 emits a
semantic adapter closure with that erased MIR signature
`(ref struct, eqref) -> eqref` and passes it to `apply` as an erased value:

```text
adapter(closure, x):
    f  = ClosureGetCapture(closure, 0)          // erased capture, index 0
    g  = RepresentationCast(f, Closure(Int -> Int))
    a  = RepresentationCast(x, Repr(integer_box))   // argument first
    n  = ProductGet(integer_box, field 0, a)        // unbox to Int
    r  = IndirectCall(g, signature(Int -> Int), n)
    rb = ProductNew Box{Integer}(r)                 // box the result
    return RepresentationCast(rb, Erased)
```

The adapter is created by `FunctionRef(adapter, erased_signature,
captures = [lambda])`; the original lambda is evaluated once, before the
adapter is invoked, and each adapter call unboxes its argument exactly once.

## Boundaries and interfaces

- **Input:** CC values, `SignatureId`s, and adaptation operations produced by
  P8's closure conversion.
- **Output:** concrete MIR value types and `RefCast`/`RefTest`/closure
  operations; MIR contains no type variables or erased requirements.
- **To [data representation](data-representation.md):** the concrete box, boxed
  capture, closure, and capture-array layouts.
- **To [type classes and dictionaries](type-classes-and-dictionaries.md):** the
  requirement that dictionary evidence is ordinary product/closure data.
- **To [canonical ABI](../wasm/canonical-abi-and-wit.md):** erased values never
  reach the ABI boundary; ABI adaptation happens on concrete canonical
  signatures only.

## Open questions and future work

- **Dictionaries end to end.** Type-class elaboration must produce the
  dictionary values this design assumes and feed them through the normal
  aggregate path.
- **Open-row aggregates.** Canonical generic arrays and closed records use the
  conversion contract in [generic aggregate erasure](generic-aggregate-erasure.md).
  Open-row records still need a separate representation and conversion contract.
- **Higher-order acceptance breadth.** Direct generic calls, concrete arguments
  to generic parameters, and returned polymorphic functions are the remaining
  adapter cases to exercise end to end.
- **Monomorphization as an optimization.** A future specialization pass may
  remove boxes at statically known instantiations while preserving the erased
  representation as the fallback.
- **`i31` for boxed `Integer`.** `i31` covers fewer values than the full signed
  32-bit range, so the erased protocol cannot use it without a fallback; the
  current design keeps the full-width integer box.

## Implementation notes

`RepresentationTest` is present in the CC model, the verifier, and the MIR
lowering, but no current lowering pass generates it; only `RepresentationCast`
is produced. The erased execution fixture covers scalars and a concrete
reference through identity. Source execution covers higher-order adapters and
type-class dictionaries, including imported generic instances. Generic arrays
and closed generic records reconstruct across nominal layouts through explicit
conversion plans, including recursive function-adapter leaves. The
[acceptance record](../../../implementation/backend/generic-aggregate-erasure.md)
distinguishes source programs from verified Typed Core backend fixtures and
records source coverage and remaining obligations.

## References

- Reynolds, *Types, Abstraction and Parametric Polymorphism* (1983).
- Crary, Weirich, and Morrisett, *Intensional Polymorphism in Type-Erasure
  Semantics* (2002).
- Wadler and Blott, *How to Make ad-hoc Polymorphism Less ad hoc* (1989).
- Peyton Jones, Jones, and Meijer, *Type Classes: an exploration of the design
  space* (1997).
- WebAssembly 3.0: garbage collection and typed function references.
- [DEC-07](../../../decision/DEC-07-runtime-representation-for-parameterized-adts.md),
  [CC IR](cc-ir.md),
  [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md).
