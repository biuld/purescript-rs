# Generic Aggregate Erasure

**Feature:** F-02
**Status:** Stable (design)
**Prerequisites:** [CC IR](cc-ir.md), [MIR](mir.md), [data representation](data-representation.md), and [polymorphism and erasure](polymorphism-and-erasure.md); WebAssembly GC's nominal struct and array types. Read [IR boundaries](../00-ir-boundaries.md) first.
**Summary:** This document defines how generic arrays and closed records cross between concrete, specialized layouts and canonical layouts that are independent of type arguments. It keeps DEC-07's runtime erasure for parameterized ADTs and adds explicit, typed conversions wherever an aggregate's nominal GC layout changes. Generic arrays and closed records use canonical erased layouts inside polymorphic code; concrete instantiations keep their specialized layouts.

## Scope

This document owns representation normalization and conversion for generic
arrays and closed records, including their use as fields of parameterized ADTs.
It defines the CC conversion contract and its lowering and verifier obligations
through MIR. It does not change the erased-value policy for ADT fields in
[DEC-07](../../../decision/DEC-07-runtime-representation-for-parameterized-adts.md),
define open-row records, change source type inference, or define independent
Wasm artifact linking. Scalar boxing and polymorphic function adapters remain
specified by [polymorphism and erasure](polymorphism-and-erasure.md); concrete
GC objects and pure array update remain specified by
[data representation](data-representation.md).

Implementation acceptance is tracked in the
[topic execution checklist](../../../implementation/backend/generic-aggregate-erasure.md).
Existing code and regression tests are evidence candidates; this stable design
status does not claim that every requirement has been implemented or verified.

## Background

Wasm GC struct and array types are nominal. Mutable array element types are
invariant: `(array (mut i32))` and `(array (mut eqref))` are distinct defined
types, and `ref.cast` cannot transform one array's elements into another
layout. A reference cast checks object identity and declared subtyping; it does
not walk an array or rebuild a struct.

DEC-07 selects one erased runtime representation for parameterized ADTs and
does not specialize a polymorphic function for every call. Its erased `eqref`
slot can hold a reference, but that fact does not make a concrete aggregate
object have the layout expected by a generic body. For example, an
`Array Int` reference may be upcast to `eqref`, but it is not an instance of the
canonical array type needed to implement `Array a` for arbitrary `a`.

The design therefore distinguishes **erasing a reference** from **converting an
aggregate layout**. Erasing a reference is a reference upcast and preserves the
object. Converting a layout allocates a new aggregate and converts its elements
or fields according to the typed source and destination. Scalar values use the
existing typed boxes when they cross an `Erased` boundary.

## Model

### Runtime shape normalization

P8 derives a runtime shape from a typed Core type and its type-variable scope.
It distinguishes a declaration template from an instantiated actual type; this
keeps a generic layout canonical at a concrete call site. Normalization is
structural and cycle-safe, and produces CC requirements, never Wasm types:

- `Template(T, Q)` normalizes a type in a polymorphic declaration while
  treating that declaration's bound variables `Q` as abstract. It does not
  apply a call-site substitution to those variables.
- `Actual(T, S)` first applies boundary substitution `S` to the source
  expression's type. A closed result receives its specialized shape; variables
  that `S` does not resolve remain abstract and use template normalization.

Formal parameters, function results, and declared ADT field templates are
normalized with `Template`. Values at calls and constructors are normalized
with `Actual`. P8 constructs a conversion plan between those two shapes.

In the CC plan, `TypeRole` records only which normalization rule P8 used:
`Template` or `Actual`. It carries no Core `TypeId` into MIR. `TemplateContext`
identifies the bound variables that remain abstract; `Substitution` resolves
the variables known at an actual call or construction boundary.

| Typed Core type | Normalized runtime value shape |
| --- | --- |
| A bare type variable `a` | `Erased` |
| A concrete scalar such as `Int` | Its scalar CC shape |
| A concrete `Array Int` | `Reference(Repr(Array(Integer)))` |
| A type-dependent `Array a` | `Reference(Repr(Array(Erased)))`, the canonical generic array |
| A type-dependent `Array (Array a)` | An array of references to canonical `Array(Erased)` values |
| A closed record `{ x :: Int }` | A product with the concrete `Integer` field |
| A dependent closed record `{ x :: a }` | A canonical product with an `Erased` field |
| A dependent closed record `{ xs :: Array a }` | A canonical product with an `Array(Erased)` reference field |
| A parameterized ADT | Its nominal variant representation, independent of type arguments |

For a type-dependent `Array T`, P8 recursively normalizes `T` and interns the
array by that normalized element shape. Thus the canonical `Array a` layout is
`Array(Erased)`, a GC array whose physical element storage is nullable `eqref`.
Its elements are logically non-null language values; nullable storage exists
for default allocation and private construction only. `Array (Array a)` stores
references to the canonical inner-array layout in its slots. This recursive
rule preserves useful reference shapes while ensuring that `Array a` has one
layout independent of the instantiation of `a`.

A closed generic record uses a canonical product shape. Each field is
normalized recursively, and the physical field order is the canonical field
order. Thus `{ value :: a, items :: Array a }` has the field shapes
`[Erased, Reference(Repr(Array(Erased)))]`. A bare variable field is erased;
aggregate fields retain their canonical aggregate reference shape. Open rows
have no canonical product in this design and remain unsupported.

An ADT's variant layout continues to be keyed by its resolved declaration. A
constructor field whose declared type depends on an ADT parameter is stored as
`Erased`, as DEC-07 requires. Before storage or after projection, however, the
field value is adapted through the normalized shape of the declared field
template. Consequently a field declared `Array a` is canonicalized to
`Array(Erased)` before it is placed in the erased slot. A field declared just
`a` remains an ordinary erased value and does not force an aggregate copy.

### Layout identity and conversion plans

`CoreTypeId` and a concrete type substitution are inputs to normalization, not
runtime layout identities. CC interns requirements by canonical shape:

- generic arrays share a representation keyed by normalized element shape;
  every `Array a` specifically uses `Array(Erased)`;
- concrete arrays are keyed by their element storage shape;
- closed record keys contain canonical `(label, normalized field shape)` pairs;
  ordinary positional products have their own key. `Representation::Product`
  still contains only physical field shapes, while P8 keeps each record's
  label-to-index mapping for lowering; and
- ADT variants are keyed by resolved declaration identity, with one set of
  dependent erased fields for all type arguments.

P8 reserves a representation handle before recursively normalizing its fields,
so recursive references are cycle-safe. Equal canonical keys share a CC
`ReprId`; distinct concrete instantiations may have different IDs. P9 assigns
one `DefinedTypeId` to each reachable `ReprId` according to the existing
planner contract. P9 does not merge IDs by physical shape: ADT identity remains
declaration-based, and record/positional-product identity follows its P8 key.
The current driver links source modules into one Core program before P8, so
they share normalized keys and the resulting layout table. Linking
independently compiled Wasm artifacts while sharing GC nominal types is
outside this design.

P8 records a target-neutral conversion plan when typed values with different
normalized shapes cross a semantic boundary:

```text
ValueConversion = Identity
                | BoxScalar(BoxKind)
                | UnboxScalar(BoxKind)
                | EraseReference
                | RecoverReference { destination: ValueShape,
                                     evidence: RecoveryEvidence }
                | Sequence([ValueConversion])
                | ArrayMap { source: ReprId, target: ReprId,
                            element: ValueConversion }
                | ProductMap { source: ReprId, target: ReprId,
                              fields: [ValueConversion] }
                | FunctionAdapter { source: SignatureId, target: SignatureId }

RecoveryEvidence = TypeInstantiation
                 | ErasedVariantField { variant: ReprId, tag: u32,
                                       field: u32, template: ValueShape }

AggregateConvert = { value: ValueId, source: ValueShape,
                     destination: ValueShape, plan: ValueConversion }
```

The exact Rust names may differ, but CC must state the source and destination
shapes and the recursive work. The plan contains no Wasm type index, physical
field offset, or unverified type-variable cast. `ArrayMap` and `ProductMap`
mean element-wise or field-wise reconstruction; they are not reference casts.

## Design

### Canonical layouts and concrete layouts

Concrete, variable-free arrays and records keep their specialized layouts. For
example, `Array Int` remains `(array (mut i32))`, and `{ x :: Int }` remains a
struct with an `i32` field. This preserves the current concrete data path.

Within a polymorphic function, `Array a` uses the canonical
`(array (mut (ref null eq)))` layout. Its slots contain erased elements: `Int`,
`Boolean`, and `String` values are boxed in the integer box, `Number` values
use the number box, and references are upcast to `eqref`. Other
generic arrays use an array of their recursively normalized element shape; for
example, `Array (Array a)` stores references to the canonical `Array a`
layout. A dependent closed record similarly uses one canonical product whose
fields are the normalized shapes of its declared fields. Such a product may
contain scalar fields, erased fields, canonical generic arrays, or nested
canonical records.

When values cross between these layouts, P8 emits a conversion plan and P9
lowers it to explicit work. If the source and destination shapes agree, the
conversion is identity. If the shapes differ only because a concrete scalar
crosses an erased slot, the conversion boxes or unboxes. If a nominal aggregate
layout differs, the conversion allocates and reconstructs it. A nominal
`ref.cast` is used only to recover a reference whose canonical or concrete
layout is guaranteed by the typed construction path; it never substitutes for
reconstruction.

### Where conversions occur

Conversions are inserted at every typed boundary where the caller's actual
representation and the callee or storage representation differ:

- **Parameterized ADT construction and projection.** Convert the field value
  to the normalized shape of its declared field template, then store an
  erased field as required by DEC-07. Projection first recovers that normalized
  template shape; an instantiation-specific caller then converts to its
  concrete shape if needed.
- **Record construction, access, and update.** Construct the product required
  by the record type at that boundary. Access reads the current record layout
  and converts its field to the typed result shape. Update converts assigned
  fields to the layout of the new record and returns a fresh product.
- **Array construction, read, and update.** A literal converts each element to
  the chosen array's logical element representation before storage. Reads
  convert from the physical slot shape to the statically known result shape.
  A pure update clones the source layout and converts the replacement element
  before writing; it never mutates the source.
- **Direct calls and returns.** Arguments convert from the caller's actual
  shapes to the callee signature's normalized shapes. Results convert from the
  callee result shape to the caller's instantiated result shape.
- **Higher-order calls.** The existing erased function adapter includes these
  argument and result conversions in its body, alongside scalar box/unbox and
  closure-signature adaptation.
- **Captures.** A captured generic aggregate is stored using its canonical
  aggregate layout and then placed in the uniform capture array as an
  `eqref`. A concrete aggregate captured by concrete code keeps its specialized
  layout. Any conversion required by the lifted function's signature is
  performed before capture or when the capture is read.
- **Linked source modules.** Since modules are combined into one Core program
  before P8, the call boundary uses the same canonical `ReprId` and requires
  only the type-directed conversion above, not a separate module ABI adapter.

### Reconstruction semantics

`ArrayMap` allocates a fresh destination array of the target layout, iterates
from zero to the source length, reads each source element, applies the nested
element conversion, and writes the converted element into the private target.
For reference element storage, physical slots are nullable references of the
target heap type; for `Array(Erased)` they are nullable `eqref`. All logical
values are converted to non-null references before the destination is exposed.
No source array is changed. Nested array and record values recursively use
their own conversion plans. A conversion whose source and target layouts are
identical does not allocate.

`ProductMap` reads fields in canonical logical order, converts each field, and
allocates one fresh destination product only after the field values are ready.
Record updates follow the existing pure semantics: they construct a fresh
product with unchanged fields copied and changed fields converted. They do not
mutate an aliased record. Because arrays and records have no observable pointer
identity or mutation, a conversion may duplicate physical sharing while
preserving values. If the language later exposes pointer identity, mutable
aggregates, or cyclic aggregate graphs, the conversion design must add an
identity-preserving memo before those features are enabled.

### Rejected alternatives

- **Blind `ref.cast` from concrete to generic aggregate.** Rejected because
  nominal GC types and mutable array element types differ; the cast does not
  transform slots and may trap.
- **Erase every aggregate pointer and cast it back at each use.** Rejected for
  the same nominal-layout reason. A cast from `eqref` to `Array(Erased)` is
  valid only after the compiler has established that canonical layout.
- **Give every erased value a runtime type tag or dictionary.** Rejected by
  DEC-07's uniform erased protocol; aggregate type reflection is not needed for
  parametric source operations.
- **Use erased layouts for all concrete arrays and records.** Rejected because
  it would discard specialized concrete layouts and force boxing and mapping
  on programs that do not cross a polymorphic boundary.
- **Monomorphize every generic function.** Rejected by DEC-07; it changes the
  selected runtime model and does not define behavior for higher-order values
  or separate linking.

## Algorithms

### Normalization and plan construction

P8 walks each typed value shape with a cycle guard and the active substitution.
It preserves concrete shapes when the type is closed. When a variable occurs
inside a generic aggregate, it creates or reuses the canonical aggregate
representation and records the logical element or field conversions at the
operation that crosses into or out of it. Open record rows and unknown foreign
aggregate layouts produce a named, source-spanned backend diagnostic.

At a typed boundary, plan construction follows this procedure:

```text
convert(source_type, source_role, destination_type, destination_role, substitution):
    source_shape = normalize(source_type, source_role, substitution)
    destination_shape = normalize(destination_type, destination_role, substitution)
    require Core proves the typed boundary is valid under substitution
    if source_shape == destination_shape:
        return Identity
    if destination_role is Template and destination_type is a bare variable:
        if source_shape is scalar: return BoxScalar(box_kind(source_shape))
        if source_shape is reference: return EraseReference
    if source_role is Template of bare variable and destination_role is Actual:
        if destination_shape is scalar: return UnboxScalar(box_kind(destination_shape))
        if destination_shape is reference:
            return RecoverReference(destination_shape, TypeInstantiation)
    if both types are arrays and their element types correspond under substitution:
        return ArrayMap(source_repr, destination_repr,
                        convert(source_element_type, source_role,
                                destination_element_type, destination_role,
                                substitution))
    if both types are closed records with the same label set and corresponding
       field types under substitution:
        return ProductMap(source_repr, destination_repr,
                          convert each matching field in canonical order with
                          the same source and destination roles)
    otherwise:
        report UnsupportedAggregateConversion at the boundary's source span
```

For an ADT field projection from erased storage, P8 emits a separate recovery
plan: recover a scalar box or cast to `Template(field_type)`, attaching
`ErasedVariantField { variant, tag, field, template }` evidence. CC verifies
that the referenced variant slot is physically `Erased` and that the recovery
target agrees with the evidence. The constructor invariant proves the
canonical template value was placed there. If the field template itself is
`a`, this recovery is identity and leaves the value erased.

For a dependent ADT field, normalization uses its declared field template, not
just the substituted field type. Construction first converts the actual field
value to `Template(field_type)`, then adapts that canonical value to the
variant's `Erased` storage shape. If the template is `Array a`, an actual
`Array Int` is first mapped to canonical `Array(Erased)`, then that reference
is erased. If the template is only `a`, an actual `Array Int` is erased
directly. Projection reverses those steps: recover the stored value as the
normalized field template, then convert from that template to the pattern's
instantiated actual shape. Thus a concrete match on `Wrap Int` recovers
`Array(Erased)` before mapping to `Array Int`; a polymorphic `unwrap` keeps the
canonical layout.

The order is explicit in the lowering contract:

```text
construct_dependent_field(actual_value, field_template, substitution):
    template_shape = Template(field_template)       # never substitute first
    actual_shape = Actual(actual_value.ty, substitution)
    logical = plan_conversion(actual_value.ty, Actual, field_template, Template,
                              substitution, actual_shape, template_shape)
    storage = Erased
    return Sequence([logical, adapt_to_storage(template_shape, storage)])

project_dependent_field(stored_value, field_template, substitution):
    template_shape = Template(field_template)       # same key as construction
    recover = if template_shape == Erased: Identity
              else RecoverReference(template_shape,
                  ErasedVariantField(variant, tag, field, template_shape))
    actual_type = substitute(field_template, substitution)
    actual_shape = normalize_actual(actual_type, substitution)
    specialize = plan_conversion(field_template, Template,
                                actual_type, Actual,
                                substitution, template_shape, actual_shape)
    return Sequence([recover, specialize])
```

`adapt_to_storage` boxes scalar template values or erases a reference. It is
identity when the canonical template shape is already `Erased`. The
`ErasedVariantField` token is produced only by P8 for the corresponding
dependent `VariantGet`; the CC verifier checks that its named case field is
stored as `Erased` and that its target agrees with the token. The type
substitution affects only the final `specialize` step, never the canonical
shape used by construction and generic projection.

### Lowering conversion plans

P9 lowers `Identity`, scalar boxing, scalar unboxing, reference erasure, and
reference recovery with the existing shape-checked operations. It lowers
`ProductMap` by `StructGet` on each source field, recursively converting the
fields, then `StructNew` on the target product. It lowers `ArrayMap` to MIR
control flow: read the source length, allocate a private destination with
defaultable storage, loop over elements, convert each element, and store it.
MIR includes an explicit `ArrayNewDefault` operation for this construction;
the verifier proves that the target element storage is defaultable and that
every slot is initialized before the array escapes. The Wasm encoder maps this
operation to `array.new_default` and emits the already-verified loop.

Conversion helpers are interned by their complete source shape, target shape,
and nested conversion plan. This shares identical work without conflating
different type substitutions. The helper receives and returns the exact MIR
types in its key. A plan with an unsupported source or target fails in P8 with
a named diagnostic at the source operation; malformed internal plans are
rejected by the CC or MIR verifier as compiler errors.

## Code map

The design keeps normalization in P8, abstract conversion intent in CC, and
physical reconstruction in P9/MIR. Wasm encoding remains a mechanical mapping.

```text
cc/
  layout/normalize.rs       # typed Core shape normalization and canonical keys
  convert.rs                # target-neutral conversion plan construction
  representation.rs         # interned ReprId and ValueShape requirements
mir/
  layout/                   # canonical and concrete GC layout planning
  lower/aggregate.rs        # ProductMap and ArrayMap helper/CFG lowering
  lower/erased.rs           # scalar boxes and checked erased-reference recovery
  verify/conversion.rs      # conversion-plan endpoints and initialization proof
wasm/lower/structure/
  arrays.rs                 # verified array.new_default and array operations
```

The entry points must preserve the IR boundary:

```rust
fn normalize_template(core: &TypedCore, ty: TypeId, context: &TemplateContext)
    -> Result<ValueShape, BackendError>;
fn normalize_actual(core: &TypedCore, ty: TypeId, substitution: &Substitution,
                    residual_template: &TemplateContext)
    -> Result<ValueShape, BackendError>;
enum TypeRole { Template(TemplateContext), Actual(Substitution) }
fn plan_conversion(core: &TypedCore, source_type: TypeId, source_role: TypeRole,
                   destination_type: TypeId, destination_role: TypeRole,
                   substitution: &Substitution,
                   source_shape: ValueShape, destination_shape: ValueShape,
                   span: TextRange) -> Result<ValueConversion, BackendError>;
fn lower_conversion(plan: &ValueConversion, value: ValueId, span: TextRange)
    -> Result<ValueId, BackendError>;
```

P8 owns type substitutions, source spans, and the proof that source and
destination types are compatible. CC carries only interned shapes and
target-neutral conversion plans. P9 resolves those shapes to `DefinedTypeId`s
and emits MIR instructions and CFG. MIR carries no Core `TypeId` or type
variable. The Wasm encoder receives verified MIR and chooses no layout.
`TemplateContext`, `Substitution`, and `TypeRole` are P8-only inputs; emitted CC
plans carry only `ValueShape`s, representation handles, and recovery evidence.

## Invariants and verification

- Every canonical key is deterministic and cycle-safe; `CoreTypeId` and
  concrete type substitutions do not become runtime tags.
- Every value crossing a boundary has a conversion plan whose source and
  destination match the typed Core types and CC shapes recorded at that
  operation.
- A reference cast from `Erased` to an aggregate layout is allowed only when
  the producing operation establishes that exact canonical or concrete
  layout. It never converts between two different nominal aggregate layouts.
- Every `RecoverReference` includes P8 evidence. `TypeInstantiation` is emitted
  only for a value whose typed source is an abstract variable instantiated at
  that boundary. `ErasedVariantField` identifies a variant case field whose
  stored shape is `Erased` and whose declared template normalizes to the target
  shape.
- `ArrayMap` source and target handles resolve to array representations; its
  nested plan matches their logical element conversion. The source is not
  mutated, every target slot is initialized before exposure, and the result has
  the target array shape.
- `ProductMap` source and target handles resolve to closed products with the
  same field-label set. Corresponding field types agree under the boundary
  substitution; field plans match labels and normalized shapes and the result
  has the target product shape.
- A generic ADT dependent field is physically `Erased`. Construction converts
  through the normalized declared field template before erasing. Projection
  recovers that normalized template shape, not an arbitrary instantiation.
- Pure array and record updates produce fresh values. Conversion cannot mutate
  its input or change any language-observable alias.
- CC verification rejects plans with unresolved handles, incompatible source
  and destination shapes, wrong field arity, or an invalid nested conversion.
  P9/MIR verification rejects mismatched nominal types, uninitialized
  reference slots, invalid casts, and incorrect array element/index types.
- A source-level conversion that cannot be represented is a named,
  source-spanned P8 diagnostic. A malformed internal plan is a compiler error;
  it must never be emitted as a runtime cast that can trap on a well-typed
  program.

## Worked example

Consider a parameterized constructor whose field is an array:

```purescript
data Wrap a = Wrap (Array a)

wrap :: Array Int -> Wrap Int
wrap xs = Wrap xs

unwrap :: forall a. Wrap a -> Array a
unwrap (Wrap xs) = xs

main :: Int
main = arrayIndex (unwrap (wrap [40, 42])) 1
```

The concrete literal and `wrap` argument use `Array Int`, whose runtime layout
is `(array (mut i32))`. The declared field template is `Array a`, so its
canonical shape is `Array(Erased)`. At construction P8 records `ArrayMap` with
an `Integer -> Erased` element conversion; P9 allocates the canonical array,
boxes each integer, then upcasts the canonical array reference into the ADT's
erased field:

```text
ArrayInt([40, 42])
  -> ArrayMap(BoxInteger)
  -> ArrayErased([box(40), box(42)])
  -> Erased variant field
```

`unwrap` is compiled once. Its pattern projection recovers the canonical
`Array(Erased)` layout established by the constructor path, and its generic
result uses that same layout. At the concrete call site, the result boundary
maps the canonical array back to `Array Int`, unboxing each element before
`arrayIndex` returns an `Int`. No runtime type tag is needed, and no nominal
array cast is used as a conversion.

A closed generic record follows the same rule:

```purescript
copy :: forall a. { values :: Array a } -> Array a
copy record = record.values

main = arrayIndex (copy { values: [40, 42] }) 1
```

The concrete record contains an `Array Int` reference. The generic record
layout contains a canonical `Array(Erased)` reference. The call boundary
converts the nested array and builds the canonical product. `copy` reads that
field using its canonical product layout and returns the canonical array. The
concrete caller converts the result back to `Array Int`. A generic update
builds a fresh canonical product and adapts the new `values` field by the same
rule; it does not mutate the input record.

By contrast, for `data Hold a = Hold a`, `Hold (Array Int)` stores the concrete
array reference directly in the erased `a` slot. A polymorphic function may
return it as `a` without inspecting its shape; a concrete caller at
`Hold (Array Int)` recovers the same `Array Int` representation. This case
does not need an array map because the declared template is the bare variable
`a`, not `Array a`.

## Boundaries and interfaces

- **Typed Core to P8:** Core supplies the actual type substitution and source
  span. P8 determines whether a boundary is identity, box/unbox, reference
  erasure/recovery, or aggregate reconstruction.
- **P8 to CC:** CC receives target-neutral shapes and conversion plans. It does
  not receive Core type variables, Wasm heap types, or physical field indices.
- **CC to P9/MIR:** P9 resolves canonical `ReprId`s to nominal layouts and
  lowers `ArrayMap`/`ProductMap` to verified instructions and CFG. `ArrayNewDefault`
  is legal only for defaultable storage and only while the result is private
  until initialized.
- **MIR to Wasm:** the encoder emits GC operations and structured loops from
  verified MIR; it performs no layout conversion itself.
- **Source modules:** linked Core modules share normalization and layout keys.
  Independently compiled Wasm artifacts that must share nominal GC type
  definitions need a separate artifact-linking design and are out of scope.
- **Failure:** unsupported open rows, unknown foreign generic aggregates, or
  inconsistent source/target types produce source-spanned diagnostics before
  Wasm emission. A Wasm allocation failure may still trap as specified by the
  runtime.

## Open questions and future work

- Define a safe conversion protocol for open-row records if row-polymorphic
  values become a backend feature.
- If pointer identity, mutation, or cyclic aggregate graphs become observable,
  determine whether reconstruction must preserve sharing and cycles.
- Independent compiled Wasm module linking must define how canonical GC types
  are shared or adapted; source-module linking through shared Core does not
  answer that ABI question.

## References

- [DEC-07: Runtime Representation for Parameterized ADTs](../../../decision/DEC-07-runtime-representation-for-parameterized-adts.md).
- [Polymorphism and erasure](polymorphism-and-erasure.md).
- [Data representation](data-representation.md).
- [CC IR](cc-ir.md) and [MIR](mir.md).
- WebAssembly 3.0 specification: GC struct and array types, subtyping,
  `array.new_default`, `array.get`, `array.set`, and reference casts.

## Implementation notes

The [implementation acceptance checklist](../../../implementation/backend/generic-aggregate-erasure.md)
records the GA-01 through GA-20 evidence and the independent review repairs.
P9 now interns complete conversion plans into shared MIR helpers, and MIR
verification rejects nullable array loads declared non-null and aggregate
conversion paths that can expose incompletely initialized arrays. Optimized
components execute the array, record, ADT, adapter, and capture cases under
required Wasmtime. Empty array backend coverage uses a Typed Core fixture;
source empty literals remain unsupported at P5.
