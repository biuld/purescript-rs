# Data Representation

**Feature:** F-02  
**Status:** Stable (design)  
**Prerequisites:** [CC IR](cc-ir.md), [MIR](mir.md), and
[polymorphism and erasure](polymorphism-and-erasure.md); the WebAssembly 3.0
type system (structs, arrays, subtyping, `ref.test`/`ref.cast`, `i31`, and
typed function references). Read [IR boundaries](../00-ir-boundaries.md)
first.  
**Summary:** Functional data lowers to a concrete Wasm GC representation chosen
entirely by the P9 planner: sums become a tag-carrying abstract supertype with
one final subtype per case (or an immediate `i32` tag when every case is
nullary), products and records become structs, arrays become mutable GC arrays,
and closures become `{ funref, capture-array }` structs whose captures are
stored in one uniform `eqref` array. This document fixes those layouts, the
operation lowering, and the execution-evidence expectations that promote a
backend capability.

## Scope

This document owns the concrete GC heap layouts that the P9 GC planner builds
from CC requirements, the Wasm operations that construct and observe them, and
the execution-evidence expectations recorded for each capability. It does not
own the planner contract (see [IR boundaries](../00-ir-boundaries.md)), the
target-neutral `Variant` model (see [CC IR](cc-ir.md) and
[CC IR](cc-ir.md)),
the erased protocol for polymorphic values (see
[polymorphism and erasure](polymorphism-and-erasure.md)), scalar semantics (see
[scalars and primitives](scalars-and-primitives.md)), or the byte-oriented
string and ABI boundary (see
[linear memory and the canonical ABI
boundary](../wasm/linear-memory-and-canonical-abi-boundary.md)).

## Background

**Wasm GC.** WebAssembly 3.0 adds nominal, definable struct and array types,
nullable and non-nullable references, declared subtyping, and the runtime tests
`ref.test` and `ref.cast`. A defined type may be final (no subtype) or
non-final (open to subtypes). Struct and array fields carry storage types and an
immutability flag. `i31` is an immediate 31-bit integer reference in the `any`
hierarchy. See the WebAssembly 3.0 specification (GC and typed function
references).

**Typed function references and `call_ref`.** A function value can be a typed
reference `(ref $functype)`, and `call_ref $functype` calls it with that exact
signature. Closures therefore need both a code reference and their environment;
the environment must be uniform enough that one closure struct type can hold any
capture list.

**Pure-update arrays.** The language treats array update as pure: updating an
array returns a new array and leaves the input and its aliases unchanged.
Mutable Wasm arrays alone cannot express this, so a pure update copies the
array before writing.

**Erasure.** Parameterized values erase their type arguments
([DEC-07](../../../decision/DEC-07-runtime-representation-for-parameterized-adts.md)),
so a field or capture whose representation depends on a type parameter is
stored as a uniform erased reference rather than as the parameter's concrete
type.

**Target-neutral variants.** [CC IR](cc-ir.md)
makes the sum encoding a P9 decision: CC states one `Variant` requirement per
source sum type with a stable tag and field shapes per case, and the GC planner
chooses the object layout.

## Model

### Value types

| CC `ValueShape` | MIR `ValueType` |
| --- | --- |
| `Integer` | `I32` |
| `Boolean` | `Boolean` (encoded as `i32`) |
| `Number` | `F64` |
| `String` | `I32` (ABI address; distinct from numeric `Integer` in CC) |
| `Reference(Repr(id))` | `(ref $repr)` or `(ref null $repr)` |
| `Reference(Aggregate)` | `(ref struct)` or `(ref null struct)` |
| `Reference(Closure(signature))` | `(ref $closure)` or `(ref null $closure)` |
| `Reference(Erased)` | `(ref eq)` or `(ref null eq)` |

Nullability is copied from the CC `Reference`. A `String` is not a GC object: it
stays an `i32` pointer to a length-prefixed UTF-8 buffer so the canonical ABI
can consume it directly ([linear memory](../wasm/linear-memory-and-canonical-abi-boundary.md)).

### Concrete layouts

All GC objects are immutable unless stated otherwise.

| CC requirement | MIR type | Fields |
| --- | --- | --- |
| `Box { Integer }` | final `struct` | one `i32` |
| `Box { Number }` | final `struct` | one `f64` |
| `Product { fields }` | final `struct` | each field's storage, in order |
| `Variant { cases }` | abstract non-final `struct { tag: i32 }` plus one final case `struct { tag: i32, fields... }` | see below |
| `Array { element }` | `array (mut storage)` | one mutable element type |
| closure | final `struct` | `(ref func)` code reference, then `(ref $capture_array)` |
| capture array | `array (mut (ref null eq))` | one mutable nullable `eqref` per capture |

A `Variant` is realized per
[CC IR](cc-ir.md): the
supertype is non-final and carries only the tag at field 0; each case is a
final subtype that repeats the tag at field 0 and appends the case fields at
fields `1..`. Constructor identity is the tag; source type arguments are never
runtime tags. A sum type whose constructors are all nullary uses an immediate
`i32` tag and creates no `Variant` requirement and no object.

Captures are stored in one uniform `(ref null eq)` array so a closure type does
not depend on its capture types:

- an `Integer` or `String` capture is boxed in the one-field i32 box;
- a `Boolean` capture is boxed with `i31.new`;
- an `f64` capture is boxed in the one-field number box;
- a reference capture is stored as-is; and
- an erased capture is already `eqref`.

### Type-table invariants

- Every defined type reference resolves within the completed recursion group;
  mutual references may point forward. A variant supertype precedes every case
  subtype because Wasm subtyping requires that order.
- All planned definitions live in one `RecGroup`, so recursive and mutually
  recursive types are admissible.
- `Box` types exist only when a value is actually boxed; an unreachable box
  does not become a type.
- Every field is immutable except an array's element field.
- The closure struct's two fields are non-null; the capture array's elements
  are nullable because the fresh array is initialized by `array.copy`.

## Design

### Chosen method

P9 is the only stage that chooses representation
([IR boundaries](../00-ir-boundaries.md)). The `GcPlanner` receives the
CC `RepresentationTable` plus the reachable handles and produces a
`PlannedLayout` containing one `RecGroup` and the index maps the lowering uses.
The lowering then emits a fixed set of GC operations; the Wasm emitter maps
each MIR instruction to its GC opcode mechanically.

### Planner mapping

`PlannedLayout::plan_selected` assigns concrete type IDs as follows (order is
significant):

```text
sort reachable ReprIds and SignatureIds ascending
for each reachable ReprId, in order:
    reserve a final struct type at that index; remember Product field shapes
for each reachable Variant ReprId:
    its own reserved type becomes the abstract non-final supertype { i32 tag }
    append one final subtype per case after all reserved types
if any signature is reachable:
    append the mutable nullable eqref capture-array type
    append the closure struct type { (ref func), (ref capture-array) }
for each reachable SignatureId:
    append a final func type (ref struct, erased/concrete params...) -> result
fill in each reserved/composite definition
```

Two details are load-bearing:

- **Box discovery.** The planner records the type IDs of reachable
  `Box { Integer }` and `Box { Number }` representations (`boxed_integer_index`,
  `boxed_number_index`) so closure creation and capture projection can name
  them. A box that no reachable conversion needs is never planned.
- **Closure receiver.** A signature's MIR func type starts with a non-null
  `(ref struct)` receiver, not the concrete closure struct. The closure is cast
  to the closure struct when its code reference is projected, so the signature
  does not depend on the closure type.

A profile without `gc`, `reference_types`, or (for closures) `function_references`
is rejected with `UnsupportedGcTarget`/`UnsupportedClosureTarget` and a
source-associated diagnostic, never by a silent fallback.

### Operation lowering

| CC operation | MIR instruction | Wasm |
| --- | --- | --- |
| `ProductNew` / `ProductGet` | `StructNew` / `StructGet` | `struct.new` / `struct.get` |
| `VariantNew` | `Constant tag`, `StructNew` of the case type | `i32.const`, `struct.new` |
| `VariantTag` | `RefCast` to supertype, `StructGet` field 0 | `ref.cast`, `struct.get` |
| `VariantGet` | `RefCast` to case type, `StructGet` field `field + 1` | `ref.cast`, `struct.get` |
| `ArrayNew` | `ArrayNew` | `array.new_fixed` |
| `ArrayGet` | `ArrayGet` (+ `RefCast` for a non-null reference element) | `array.get` (+ `ref.cast`) |
| `ArraySet` | `ArraySet` | `array.set` |
| `ArrayLen` | `ArrayLen` | `array.len` |
| `ArrayClone` | `ArrayLen`, `ArrayNewDefault`, `ArrayCopy` | `array.len`, `array.new_default`, `array.copy` |
| `FunctionRef` | `ClosureNew` | `ref.func`, boxing, `array.new_fixed`, `struct.new` |
| `ClosureGetCapture` | `ClosureGetCapture` | `struct.get`, `array.get`, unbox/`ref.cast` |
| `IndirectCall` | `ClosureCall` | `struct.get`, `ref.cast`, `call_ref` |
| erased adaptation | `RefTest` / `RefCast` | `ref.test` / `ref.cast` |

Three sequences are worth spelling out:

- **Pure array update.** `ArrayClone` reads the length, allocates a fresh array
  with `array.new_default`, then `array.copy`s the source range into the fresh
  array; the update then writes the copy. The reference-element slots are
  nullable so `array.new_default` can fill them before the copy.
- **Reading a non-null reference element.** `array.get` on a nullable element
  type yields a nullable reference. When the destination is non-null, P9 emits
  a temporary nullable `ArrayGet` followed by a `RefCast` to the non-null
  element type.
- **Closure call.** The closure is loaded, the arguments are loaded, and the
  closure is loaded again, cast to the closure struct, and its field 0 code
  reference is extracted and cast to the signature func type before `call_ref`.

### Rejected alternatives

- **One GC struct per constructor with a leading tag.** Rejected by
  [CC IR](cc-ir.md):
  it forces constructor dispatch to be a GC type test, conflating it with the
  erased protocol and making the target-neutral `Variant` model impossible.
- **`i31` for every nullary constructor, including mixed sums.** Rejected as
  the current design: an immediate `i31` cannot carry a field, so a mixed sum
  would need two representations and a case analysis. Nullary-only sums do use
  an immediate `i32` tag. A future planner may choose `i31` for nullary cases
  without changing CC.
- **Immutable arrays with an update-by-copy outside the array operations.**
  Rejected: pure update is an array-semantic operation, and keeping it in the
  array lowering makes the aliasing guarantee explicit and verifiable.
- **Typed per-capture closure fields.** Rejected: the closure type would vary
  with the capture list, so a single closure signature could not be shared and
  higher-order values would need monomorphization. The uniform `eqref` capture
  array keeps one closure struct type.
- **Storing `Int` and `Boolean` captures as raw `i32` in the `eqref` array.**
  Rejected: `eqref` cannot hold `i32`, and `i31` cannot represent every signed
  32-bit value. `Int` uses a full-width box; `Boolean` fits in `i31`.

## Algorithms

### Planning

The reachability worklist starts from every value and result shape, every
operation, and the referenced external signatures, then follows referenced
representations, variant fields, array elements, and closures. The planner then
applies `plan_selected` as described above. The result is a `PlannedLayout`
whose `types` become the MIR `RecGroup` and whose index maps drive the lowerer.

### Closure creation and projection

```text
ClosureNew(function, captures):
    ref.func function
    for each capture c:
        load c
        Integer   -> struct.new boxed_integer_type
        Boolean   -> i31.new
        F64       -> struct.new boxed_f64_type
        Ref       -> (unchanged)
    array.new_fixed capture_array_type, count
    struct.new closure_type

ClosureGetCapture(closure, index):
    load closure
    ref.cast closure_type
    struct.get closure_type field 1        # capture array
    i32.const index
    array.get capture_array_type
    match result ValueType:
        I32     -> ref.cast boxed_integer_type; struct.get field 0
        Boolean -> ref.cast (ref i31); i31.get_s
        F64     -> ref.cast boxed_f64_type; struct.get field 0
        Ref(r)  -> ref.cast r
```

Creation and projection are exact inverses; the capture index is a compile-time
constant.

### Variant construction, tag test, and projection

```text
VariantNew(case, fields):
    tag = i32.const case
    struct.new case_type (tag, fields...)

VariantTag(value):
    cast = ref.cast supertype(value)
    struct.get supertype field 0

VariantGet(case, field, value):
    cast = ref.cast case_type(value)
    struct.get case_type (field + 1)
```

Pattern matching emits a `VariantTag` followed by a `Switch` (or a comparison)
over the tag and a `VariantGet` inside the selected case.

## Code map

This stage is split between the P9 planner, which chooses every concrete layout,
the MIR lowerer, which emits construction and observation instructions, and the
Wasm encoder, which maps those instructions to GC opcodes. Each component must
consume the representation produced by the layer below it and must not reach
around that boundary.

```text
mir/
  layout/              # P9 GC planner: CC representation table -> concrete types
    mod.rs             # RepresentationPlanner, PlannedLayout, index accessors
    reachable.rs       # reachability worklist over reprs, signatures, fields
    plan.rs            # plan_selected assignment of defined-type IDs
    construct.rs       # box/product/variant/array/closure/capture construction
  lower/
    assignments.rs     # StructNew/StructGet, Array*, Closure* lowering
    variant.rs         # VariantNew/VariantTag/VariantGet, RefTest/RefCast
  verify/instruction/  # per-instruction checks against PlannedLayout
wasm/lower/structure/
  ops.rs               # struct.get/struct.new and ref.test/ref.cast emission
  arrays.rs            # array.new_fixed/new_default/get/set/len/copy emission
  closure.rs           # closure struct and capture-array emission
```

Required types and entry points:

- The representation value/type model (`ValueType`, `CompositeType`,
  `RecGroup`, `DefinedTypeId`) must live in `types.rs`; `PlannedLayout` must
  populate exactly one `RecGroup` of those definitions.
- `RepresentationPlanner` must be the only component that assigns concrete type
  IDs, with the entry point

  ```rust
  impl RepresentationPlanner {
      fn plan_module(
          &self,
          table: &RepresentationTable,
          module: &MirModule,
      ) -> Result<PlannedLayout, LayoutError>;
  }
  ```

  It must reject an unsupported profile with `LayoutError::UnsupportedGcTarget`
  or `LayoutError::UnsupportedClosureTarget` and must never emit a fallback for
  a missing `gc`, `reference_types`, or `function_references` capability.
- `PlannedLayout` must own one `RecGroup` whose definitions precede their uses,
  the `boxed_integer_index` and `boxed_number_index` discovery slots, the
  closure and capture-array indices, and the accessors `repr_index`,
  `variant_index`, `product_field`, `signature_index`, `closure_layout`, and
  `value_type` that the lowerer and verifier require.
- `mir/layout/` must construct each planned kind: the box structs, product
  structs, the non-final variant supertype with one final case subtype per
  constructor (and no object for an all-nullary sum), the mutable array type,
  the closure struct, and the uniform mutable nullable `eqref` capture array.
- `mir/lower/assignments.rs` must lower product, array, and closure operations
  to `StructNew`/`StructGet`, `ArrayNew`/`ArrayGet`/`ArraySet`/`ArrayLen`/
  `ArrayClone`, and `ClosureNew`/`ClosureCall`/`ClosureGetCapture`;
  `mir/lower/variant.rs` must lower `VariantNew`/`VariantTag`/`VariantGet` and
  the `RefTest`/`RefCast` erased-adaptation operations.
- `wasm/lower/structure/` must emit those MIR instructions as their GC opcodes
  (`struct.new`/`struct.get`, `array.*`, the closure sequence,
  `ref.test`/`ref.cast`) and must not choose or renumber any layout.
- `mir/verify/instruction/` must verify every GC instruction against
  `PlannedLayout`: struct and array well-formedness and subtyping, field and
  element types, capture count and boxing, cast operands and targets, and
  legality under the selected capability profile.

The planner consumes only the CC `RepresentationTable` and produces only MIR
types; the Wasm encoder assigns final binary type indices mechanically and must
never observe CC or planner state. The planner contract is owned by
[IR boundaries](../00-ir-boundaries.md), the capability gate by
[capability profile](../wasm/capability-profile.md), and the erased protocol by
[polymorphism and erasure](polymorphism-and-erasure.md).

## Invariants and verification

The MIR verifier checks every GC operation against the concrete type table:

- struct and array well-formedness and subtyping (a supertype is declared and
  non-final; case subtypes add fields; array elements are subtypes under an
  invariant mutable element type);
- `StructNew`/`StructGet` argument, field, and result types against the struct
  definition;
- `ArrayNew`/`ArrayGet`/`ArraySet`/`ArrayLen`/`ArrayClone` element and index
  types, and the nullable temporary used when recovering a non-null reference
  element;
- `ClosureNew`/`ClosureCall`/`ClosureGetCapture` capture count and boxing
  against the closure and capture-array types;
- `RefTest`/`RefCast` operand and target types; and
- legality of every type and instruction under the selected capability
  profile.

Failure is a compiler bug or an unsupported program, reported with the
operation's source span; it is never emitted as invalid output.

### Execution evidence

A capability row is promoted to `Implemented` only with the four pieces
[capability profile](../wasm/capability-profile.md) requires: a capability flag,
lowering and validation coverage, a binary or WAT regression test, and a
Wasmtime execution test where the behavior is observable. A local regression
alone is not sufficient for a row the official suite covers. Each capability
must run fixtures of the following class:

| Capability | Fixture class |
| --- | --- |
| Scalars and direct calls | the full scalar unary/binary vocabulary plus direct calls |
| `if` and `case` | value-producing branches and constructor matches |
| Nullary data types | tag construction and tag comparison |
| Field data types | construction, tag test, and field projection ([CC IR](cc-ir.md)) |
| Newtypes | erased single-field construction and match |
| Records | literal, field read, and update |
| Arrays | literal, length, index, and update |
| Closures | captured scalar/reference closures and higher-order calls |
| Parameterized ADTs | erased field construction and recovery |
| Strings and `log` | data segment, length prefix, stdout write |
| WASI clock and random | monotonic clock and random bytes |
| WIT enums and flags | validated enum tags and Boolean-record flags packing |
| Variants (unified) | mixed nullary/field sum construction and match |

GC is the only language-heap profile
([DEC-09](../../../decision/DEC-09-gc-only-language-heap.md)); linear memory is
exercised only at the canonical ABI boundary. Wasmtime tests skip when the
runtime is unavailable, so they never block `cargo test --workspace`.

## Worked example

Take a data type with a field constructor, a nullary case, and a record:

```purescript
data Shape = Rect Int Number | Dot
type Point = { x :: Int, y :: Number }

main = case Rect 7 2.5 of
  Rect w h -> w
  Dot      -> 0
```

P8 records one `Variant` requirement for `Shape` (case `Rect` tag 0 with fields
`[Integer, Number]`, case `Dot` tag 1 with no fields) and one `Product`
requirement for the record `Point` `[Integer, Number]`. P9 plans:

```text
$variant    = non-final struct { i32 }                    # supertype
$rect       = final struct { i32, i32, f64 }              # tag, w, h
$dot        = final struct { i32 }                        # tag
$point      = final struct { i32, f64 }                   # x, y
```

The constructor lowers to:

```text
t = Constant 0
v = StructNew $rect(t, 7, 2.5)
```

and the match lowers to a `VariantTag` plus a tag test, with `VariantGet` inside
the `Rect` arm:

```text
cast = RefCast v : (ref $variant)
tag  = StructGet $variant(field 0, cast)         # 0
...  branch on tag ...
case = RefCast v : (ref $rect)
w    = StructGet $rect(field 1, case)            # 7
```

A record literal `{ x: 3, y: 4.5 }` lowers to `StructNew $point(3, 4.5)`, and
its field read to `StructGet $point(field 0, ...)`. All four definitions are in
one `RecGroup`, and the `Rect` subtype follows its `$variant` supertype.

## Boundaries and interfaces

- **Input:** reachable CC representation handles and the concrete type table
  produced by P9.
- **Output:** a MIR `RecGroup` plus concrete reference types; the Wasm encoder
  only assigns final indices mechanically
  ([IR boundaries](../00-ir-boundaries.md)).
- **To [polymorphism and erasure](polymorphism-and-erasure.md):** the box,
  closure, capture-array, and reference layouts used by the erased protocol.
- **To [capability profile](../wasm/capability-profile.md):** the operation set
  requires `gc`, `reference_types`, and `function_references`; `multi_value` is
  needed only for multi-result signatures, which the current model does not
  produce.
- **To [linear memory](../wasm/linear-memory-and-canonical-abi-boundary.md):**
  strings and byte lists stay at the ABI boundary; language aggregates never use
  linear memory.

## Open questions and future work

- **`i31` for nullary cases of mixed sums.** Reserved as a future optimization;
  it must preserve the tag-carrying supertype contract.
- **GC strings.** Replacing the linear-memory string pointer with a GC string
  type would remove the last language value at the ABI boundary; the canonical
  ABI exchange format is bytes, so this is a separate design.
- **Unboxed parameterized fields.** DEC-07 allows fields proven independent of
  the parameters to stay unboxed; the layout verifier must decide this, and the
  planner currently does not perform that optimization.
- **Product and record interning.** Whether equivalent product requirements
  share one `ReprId` at P8 determines the planned type count; if they do not,
  interning equal product shapes is a future reduction.
- **Array growth.** The design fixes pure update and clone; any growable array
  operation is a vocabulary addition, not a representation change.

## Implementation notes

The layouts and operation lowerings in this document are implemented for the GC
planner, including the unified variant representation, records, arrays,
closures, and boxes. Parameterized-ADT erased field construction and recovery
have a lowering and fixture but are not yet reachable end to end through the
frontend for the full aggregate case. These are capability-coverage gaps
tracked by the matrix above and the feature matrix, not deviations from the
design.

## References

- WebAssembly 3.0: garbage collection, typed function references, `i31`, and
  `call_ref`.
- [DEC-07](../../../decision/DEC-07-runtime-representation-for-parameterized-adts.md),
  [CC IR](cc-ir.md),
  [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md).
- [capability profile](../wasm/capability-profile.md).
