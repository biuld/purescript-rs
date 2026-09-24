# Polymorphism and Erasure

**Feature:** F-02  
**Status:** Stable (design)  
**Prerequisites:** [functional core](../../frontend/semantics/functional-core.md), [CC IR](cc-ir.md),
[MIR](mir.md), and [data representation](data-representation.md); parametric
polymorphism (Reynolds) and type-erasure semantics; the Wasm GC type system
with typed function references. Read [IR boundaries](../00-ir-boundaries.md)
first.  
**Summary:** Rank-1 polymorphism is erased at runtime: a type variable denotes
one uniform, non-null `eqref`, never a type tag, and polymorphism is recovered
from explicit dictionaries and compile-time instantiation. Because Wasm
`call_ref` names one exact function type, a concrete closure cannot cross a
polymorphic boundary directly; the design inserts representation-directed
boxing, unboxing, casts, and generated function adapters. This document
specifies that erased representation, the concrete-versus-erased boundary, and
the adaptation operations.

## Scope

This document owns the erased representation for polymorphic values, the
distinction between concrete and erased representation requirements, the
semantics of the adaptation operations, the generated function adapters used at
higher-order boundaries, and the closure capture rules that follow from
erasure. It does not own concrete scalar and GC layouts (see
[data representation](data-representation.md)), the MIR type model and verifier
(see [mir](mir.md)), type-class elaboration and dictionary construction (see
[type classes and dictionaries](type-classes-and-dictionaries.md)), scalar
operation semantics (see [scalars and primitives](scalars-and-primitives.md)),
or the byte-oriented ABI boundary (see
[canonical ABI](../wasm/canonical-abi-and-wit.md)).

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

**Terminology.** An *erased value* is a value whose source type contains a type
variable and whose runtime representation is the uniform reference. *Boxing*
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
`is_erased_value_type` recognizes it. A representation requirement is *erased*
when its source type depends on a type variable: the checker's
`depends_on_type_variable` walks a `TypeId` and returns true for a
`Type::Variable`, or for any application or function type that reaches one. A
variable-free function type instead lowers to `Closure(SignatureId)`, whose
`Signature` records a concrete or erased `ValueShape` for each parameter and
the result.

### Concrete and erased signatures

A polymorphic function such as `forall a. a -> a` has the CC signature
`([Erased], Erased)`. P9 realizes that requirement as a MIR function type with
a non-null `(ref struct)` receiver followed by one `eqref` per parameter and an
`eqref` result:

```text
forall a. a -> a        =>  (ref struct, eqref) -> eqref
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

### Erased values and boxes

The erased representation is `eqref`, a non-null reference. Concrete values
enter it as follows:

| Concrete shape | Erased entry | Erased exit |
| --- | --- | --- |
| `Integer`, `Boolean`, `String` | one-field i32 `Box` struct | `ref.cast` to the box, then `struct.get` of field 0 |
| `Number` | one-field f64 `Box` struct | `ref.cast` to the box, then `struct.get` of field 0 |
| GC reference (`Repr`, `Aggregate`, `Closure`) | `ref.cast` to `eqref`, no allocation | `ref.cast` to the concrete reference |
| already erased | identity | identity |

`Boolean` and `String` are boxed in the integer box on the erased path, so all
32-bit patterns round-trip; the i31 shorthand is used only for closure *captures*, not
for the general erased protocol (see [data representation](data-representation.md)).
The empty-erasure case (an erased value used where an erased value is expected)
is an identity, so nested polymorphic boundaries add no work.

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
- `RepresentationTest`/`RepresentationCast` adapt only when the source is erased
  or the destination is erased.
- A concrete closure is never passed where a different concrete closure
  signature is expected without an adapter.
- Structurally equal `Signature`s intern to one `SignatureId`; one
  `SignatureId` maps to one MIR func type.
- An adapter captures its source function once and evaluates it before any
  invocation.

## Design

### Chosen representation: representation-directed erasure

Genuinely polymorphic values use the erased ABI; values with a known concrete
type stay specialized. The choice is made per requirement, not per module:
a call site with a concrete instantiation keeps concrete representation, while
a generic function's parameter and result use the erased requirement. There is
exactly one erased representation, and it carries no source type identity.

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

The reverse direction — a polymorphic function value returned from a generic
function and later invoked at a concrete type — is recorded at the concrete
consumer and adapted in the same way. Evaluation order is preserved: the
original function value is evaluated once and captured, and an argument is
unboxed only when the adapter is invoked.

### Closure captures under erasure

A closure is a GC struct `{ funref, capture-array }` whose captures live in one
uniform array of nullable `eqref` (`array (mut (ref null eq))`), so the closure
type does not depend on capture types. Captures enter the array as follows:

- an `Integer` or `String` capture is boxed in the one-field i32 box;
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

### Deciding erased versus concrete

```text
depends_on_type_variable(ty):
    never revisit a TypeId (cycle guard)
    Variable                         -> true
    Application(f, a)                -> depends(f) or depends(a)
    Function { parameter, result }   -> depends(parameter) or depends(result)
    otherwise                        -> false
```

A function type is *generic* when it is a `Function` and depends on a type
variable. Such a value's requirement is erased; all other value shapes are
concrete.

### Boxing and unboxing

```text
box(value):
    match shape(value):
        Integer | Boolean -> new Box{Integer}(value)   // one-field i32 struct
        Number           -> new Box{Number}(value)      // one-field f64 struct
        Erased           -> value
        other Reference  -> RepresentationCast(value, Erased)   // ref.cast

unbox(value, expected):
    match expected:
        Erased           -> value
        Integer|Boolean  -> StructGet(RepresentationCast(value, Repr(box)), 0)
        Number           -> StructGet(RepresentationCast(value, Repr(box)), 0)
        other Reference  -> RepresentationCast(value, expected)
```

`box` is used when supplying an erased parameter or result; `unbox` is used
when recovering a concrete value from an erased one.

### Adapter generation

```text
adapt(value, concrete_type, generic_type):
    source = function_signature(concrete_type)
    target = function_signature(generic_type)
    require source and target have equal arity

    adapter:
        captured  = ClosureGetCapture(closure = adapter_closure, index = 0)
        concrete  = RepresentationCast(captured, Closure(signature(concrete_type)))
        args' = for each (arg, source_param, target_param):
            source erased and target concrete -> box(arg)
            source concrete and target erased -> unbox(arg, source_param)
            otherwise                         -> arg
        result = IndirectCall(concrete, signature(concrete_type), args')
        return  target erased and source concrete -> box(result)
                target concrete and source erased -> unbox(result, target)
                otherwise                         -> result

    emit FunctionRef(adapter, signature(generic_type), captures = [value])
```

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
mir/layout/          concrete Box{Integer}/Box{Number} structs, the closure
                     struct type, and the uniform nullable-eqref capture array
                     that erased captures inhabit
mir/lower/erased.rs  boxing and unboxing helpers and adapter generation
mir/verify/          RefTest/RefCast agreement, closure capture checks, and
                     call-signature agreement at erased boundaries
```

The erased requirement itself is produced by the CC representation model as
`Reference { nullable: false, heap: Erased }`; MIR represents it as the
non-null `eqref` reference (`RefType { nullable: false, heap: Eq }`). No module
may attach a runtime type tag to an erased value.

**Required types and helpers.** `mir/lower/` MUST lower `RepresentationCast` to
a `RefCast` and `RepresentationTest` to a `RefTest` carrying the concrete target
`RefType`, and MUST provide the erased-boundary helpers:

```rust
fn box_erased_value(&mut self, value: ValueId, span: TextRange) -> Result<ValueId, Vec<BackendError>>;
fn unbox_erased_value(&mut self, value: ValueId, expected: ValueShape, span: TextRange) -> Result<ValueId, Vec<BackendError>>;
fn adapt_erased_function_value(&mut self, value: ValueId, source: SignatureId, target: SignatureId, span: TextRange) -> Result<ValueId, Vec<BackendError>>;
```

`box_erased_value` MUST emit the erased entry for a concrete shape, allocate
the matching `mir/layout/` box when the shape is a scalar, and add no
allocation for an existing reference or an already-erased value.
`unbox_erased_value` MUST reverse exactly the box selected for `expected`, and
MUST be an identity when `expected` is the erased shape.
`adapt_erased_function_value` MUST generate the adapter closure of
[Adapter generation](#adapter-generation) with the erased target signature,
capture the original value once, and preserve evaluation order.

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

- accepts `RepresentationTest`/`RepresentationCast` only when the source value
  is erased or the destination requirement is erased
  (`cc/verify/adaptation.rs`);
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
- **Generic aggregates and open rows.** Generic records, generic arrays, and
  open rows need erased fields and recovery at every access; DEC-07 fixes the
  policy, and layout coverage continues in
  [data representation](data-representation.md).
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
reference through identity; higher-order adapter execution fixtures and
type-class dictionaries are not yet wired to the frontend. Nothing in this
document depends on those temporary gaps.

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
