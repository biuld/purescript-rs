# Foreign Imports

**Feature:** F-02

**Status:** Draft

**Prerequisites:** functional programming, the Component Model's resource
handles, and [modules and resolution](modules-and-resolution.md). Read
[frontend boundaries](../00-ir-boundaries.md),
[kinds](../type-system/kinds.md), and the
[canonical ABI](../../backend/wasm/canonical-abi-and-wit.md) first. Roadmap row
FE-19 tracks this surface.

**Summary:** A `foreign import` names either a WIT function or an opaque type.
`foreign import data T :: Kind` introduces a nominal type with no constructors.
A nullary opaque type is the source image of a WIT resource; `Int` remains
accepted at the ABI boundary so existing integer placeholders keep compiling.
JavaScript FFI is not part of this surface.

## Scope

This document owns foreign value imports and foreign data declarations from
source through resolved, kind-checked, typed terms, and the type mapping those
declarations expose to the canonical ABI. It does not own WIT parsing, canonical
flattening, or handle lifetime. `resource.drop`, borrow release, and
`post-return` live in the
[canonical ABI](../../backend/wasm/canonical-abi-and-wit.md) and
[buffer lifetime](../../backend/wasm/canonical-buffer-allocation-and-lifetime.md)
designs. The `Effect` token's runtime representation is specified in
[effects](../../backend/fp/effects.md); this document only makes the opaque
declaration available for that token.

## Background

PureScript's foreign surface splits values from types. A foreign value is
implemented outside the module. A foreign data declaration is an abstract type
constructor: its kind is written after `::`, and it has no data constructors, so
a source expression cannot invent an inhabitant. Nominal identity matters. Two
declarations with the same kind are still different types, which is what makes
a resource handle distinct from `Int` and from every other handle type.

WIT represents a resource as an `own` or `borrow` handle. Both flatten to one
canonical `i32`. The integer is an index into a table, not a PureScript `Int`.
The source type has to remember which declaration it came from even though the
canonical value is a scalar. Higher-kinded foreign data, such as
`Effect :: Type -> Type`, is an abstract constructor rather than a resource.
Only a saturated type of kind `Type` that is itself a foreign data declaration
corresponds to a handle.

## Model

```text
ForeignValue = { name, binding: Interface "#" Function, type, span }
ForeignData  = { name, kind, span }
OpaqueType   = nominal TypeId with no constructors
SourceResource = { type_id: TypeId }
```

`ForeignValue` is a value-namespace declaration. Its binding text is
`<interface>#<function>` and is metadata, not a term. `ForeignData` is a
type-namespace declaration. The type expression after `::` is the kind, not a
value type. The kind may be `Type` or an arrow such as `Type -> Type`. There
are no type parameters beside that kind, and there is no value constructor.

A reference to foreign data is an opaque nominal constructor, distinct from a
data type, a newtype, a synonym, and a class. Applying it uses ordinary kind
rules. A use in a value type must have kind `Type`. The ABI mapping recognizes
only the unapplied constructor: an application such as `Effect Int` is not a
resource, even when `Effect` is opaque.

At the canonical boundary the source subset gains one case:

```text
SourceType ::= ... | Resource { type_id }
```

A WIT handle parameter or result matches `Resource` or the existing `Int`
placeholder. Other WIT scalars do not match `Resource`. The `type_id` is the
declaring module's type identity, preserved through imports and re-exports.

## Design

Foreign data is lowered as a type declaration, not as a foreign value. The CST
already records the optional `data` keyword and the kind expression; lowering
rejects a WIT binding string on a data declaration, because a resource's
identity is the nominal type rather than a function name. A foreign value
import still requires `<interface>#<function>`.

Resolution allocates a type id and no value symbol. The type name occupies the
uppercase namespace, so it conflicts with another type of the same name and
does not become a constructor. References, including those in a foreign value's
annotation, are recorded as opaque type ids. Importers and re-exporters carry
an opacity flag with the imported type id, so a later module does not need the
original declaration to keep the nominal form.

Kind checking uses the inline kind as the constructor's scheme. There are no
fields to check. An unsaturated use fails the ordinary kind check when a value
type is required. Type checking elaborates the opaque constructor as a nominal
user type. It registers no constructor scheme, so the type name is not a value,
a literal of another type does not inhabit it, and two opaque types do not
unify.

The canonical ABI reads that nominal form off the foreign value's resolved
signature. A nullary opaque type becomes `Resource { type_id }`. Validation
accepts it for a WIT handle and rejects it for an integer, boolean, or other
non-handle shape. `Int` is still accepted for a handle so declarations written
against the integer placeholder, including the current standard library, keep
their behavior. The placeholder and the opaque type are not the same PureScript
type; only the ABI adapter treats both as one `i32` handle.

Rejected alternatives:

- Treating foreign data as an empty `data` declaration. An empty data type is
  uninhabited and is not a resource. Opacity has to be explicit.
- Mapping every handle to `Int` only. That erases the nominal type the resource
  declaration exists to provide.
- Rejecting `Int` for handles immediately. Existing source still uses that
  placeholder, and changing it is outside this declaration's scope.
- Storing the WIT resource name on the foreign data declaration. The handle's
  WIT identity is fixed by the function that produces or consumes it. The
  source declaration only supplies the nominal type.
- Inserting `resource.drop` or a borrow scope in this layer. Lifetime is a
  lowering concern and is not decided by the type declaration.

## Algorithms

```text
lower_foreign(cst):
    if cst has data:
        reject a binding string
        emit ForeignData { name, kind: lower(cst.kind), span }
    else:
        require a "<interface>#<function>" binding
        emit ForeignValue { name, binding, type, span }

resolve_module:
    allocate a type id for every type declaration
    if the declaration is ForeignData, record the id as opaque and allocate no constructor
    resolve foreign value annotations; a name whose id is opaque becomes Opaque(id)
    export the opacity flag with the type
    an importer copies that flag onto the imported type id

kind_check(ForeignData):
    the scheme is the declared kind
    a value signature that uses the constructor must have kind Type

abi_source_type(type):
    Opaque(id) -> Resource { type_id: id }
    Application(Opaque(_), _) -> unsupported
    Named(id) -> the existing enum or aggregate mapping, never a resource

validate_handle(source):
    accept Resource or Int
    reject every other source type
```

Edge cases: a binding string on foreign data is a lowering error; a missing
binding on a foreign value is a lowering error; a duplicate type name is a
declaration conflict; an imported opaque type stays opaque under an alias; a
higher-kinded foreign constructor used without enough arguments fails kind
checking rather than becoming a handle.

## Code map

Foreign declarations cross the frontend as their own declaration form.

- Surface lowering turns a CST foreign declaration into either a foreign value
  or a `ForeignData` type declaration. The entry is
  `lower_foreign_data(declaration) -> Result<TypeDeclaration, LowerError>`.
  A foreign value stays
  `ForeignImport { name, annotation, binding, span }`.
- Resolved HIR records the type as `TypeDeclarationKind::Foreign` with the kind
  in `declared_kind`, no parameters, and no constructors. A reference is
  `TypeKind::Opaque(TypeId)`. Imports and exports carry `opaque: bool`.
- Kind checking consumes that declared kind through the existing scheme
  builder. It does not synthesize constructors.
- Type checking elaborates `Opaque` and `Named` with the same nominal
  constructor path and does not register a value scheme for `Foreign`.
- The ABI adapter's `source_signature` maps `TypeKind::Opaque(id)` to
  `SourceType::Resource { type_id }`. Handle parameters and results use
  `WasiParamKind::Handle` and `WasiResultKind::Handle`. Both accept `Resource`
  or `Int`. The abstract calling convention of a `Resource` is one integer,
  which is the canonical handle, not a license to treat the type as `Int`
  inside the program.

## Invariants and verification

- A foreign data declaration has no value constructor and does not inhabit the
  value namespace. Using its name as an expression is an unknown name.
- The elaborated type is nominal. It does not unify with `Int`, with another
  opaque type, or with a value of its argument when the constructor is
  higher-kinded.
- A value signature that mentions the type kind-checks only when the use has
  kind `Type`.
- Imports and re-exports preserve opacity. An imported reference is
  `Opaque` with the original type id.
- `source_signature` yields `Resource` for a nullary opaque type, never for an
  application of one and never for an ordinary data type.
- ABI validation accepts that `Resource` for a WIT handle and rejects it for a
  non-handle shape. A WIT handle still accepts `Int`.
- No stage inserts `resource.drop`, releases a borrow, or synthesizes
  `post-return` from this declaration.

Tests cover a source declaration, rejection of a constructed value and of the
type name used as a value, opacity across an import, and the handle mapping for
a vendored WIT resource function.

## Worked example

```purescript
module Streams where

foreign import data OutputStream :: Type

foreign import "wasi:cli/stdout#get-stdout" getStdout :: OutputStream

keep :: OutputStream -> OutputStream
keep stream = stream

bad :: OutputStream
bad = 1
```

P1 records `foreign import data` with the kind `Type`, and the value import
with the binding `wasi:cli/stdout#get-stdout`. P2 produces a foreign data
declaration and a foreign value whose annotation is the unresolved name
`OutputStream`. P3 assigns `OutputStream` a type id, records the annotation as
`Opaque` of that id, and gives `getStdout` a WIT external symbol. `bad = 1`
resolves, but P5 rejects it: `OutputStream` and `Int` are different nominal
types. `keep` checks, because it only passes an existing value through.

The ABI adapter classifies `getStdout`'s result as `Resource { type_id }`.
Vendored `wasi:cli/stdout#get-stdout` returns a resource handle, so validation
accepts the signature. The canonical result is still one `i32`. Nothing in this
pipeline drops that handle.

`foreign import data Effect :: Type -> Type` followed by a use `Effect Int`
kind-checks as a type, and `Effect` alone does not. `Effect Int` is not a
`Resource`.

## Boundaries and interfaces

P1 and P2 produce the declaration. P3 resolves names and preserves the binding
string and the opacity bit. P5 checks kinds and types without adding a runtime
layout. The backend's external-binding table reads the resolved signature and
validates it against the vendored WIT function. CC and MIR see an integer shape
for that boundary value only. They do not learn a WIT name from the opaque
type, and they do not yet give a source function whose result is the opaque
type a runtime layout of its own. That layout, and the drop and borrow actions,
belong to the canonical ABI lowering.

The effect library may later declare `Effect` with this form. Until that
declaration replaces the compiler's internal token, `Effect` in the embedded
prelude remains the existing abstract type, not a foreign data declaration.

## Open questions and future work

- Representing an opaque value inside CC and MIR, including a source function
  that returns one, without making it unify with `Int`.
- Inserting `resource.drop`, releasing borrows, and freeing an exported handle
  in `post-return`.
- Choosing `own` versus `borrow` from source. The current mapping treats every
  matched handle as one `i32` and does not track ownership.
- Retiring the `Int` placeholder once source declarations use opaque types.
- JavaScript `foreign import` implementations. They stay out of scope.

## References

- [Modules and resolution](modules-and-resolution.md),
  [kinds](../type-system/kinds.md), and
  [type inference](../type-system/type-inference.md) for the frontend stages.
- [Canonical ABI and WIT](../../backend/wasm/canonical-abi-and-wit.md),
  especially resources and handles.
- [Effects](../../backend/fp/effects.md) for the `Effect` token that this
  declaration can name.
- PureScript's `foreign import data` form, as in `Effect :: Type -> Type`.
