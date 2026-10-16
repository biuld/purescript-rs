# Canonical ABI and WIT

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Draft  
**Prerequisites:** the Component Model and its Canonical ABI (flattening, `lift`/`lower`, `realloc`, the return pointer and post-return), the WIT interface language (interfaces, worlds, records, variants, flags, enums, lists, resources), and the Wasm value types. Read [MIR](../fp/mir.md), the [capability profile](capability-profile.md), and [DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md) first.  
**Summary:** A source `foreign import` names a WIT function by an interface/name binding, and the backend adapts the call to the Canonical ABI generically instead of hand-coding host functions. The ABI layer resolves the canonical signature, flattens declared arguments, reads indirect results, and emits the adaptation as MIR; WIT names never enter CC or MIR.

## Scope

Executable provider selection and artifact dependency closure are owned by the
draft [linking and runtime](linking-and-runtime.md) design. Resolving a WIT
interface checks a definition; it does not prove that a host or guest implements
it. ABI lowering supplies checked source/WIT and resource-ownership contracts to
the linker. Changing providers does not authorize a new source-type projection
or pointer/handle reinterpretation.

The same proposal moves compiler-owned WIT source assets and the default world
to `psrs-runtime`, exposed through its immutable target catalog. `psrs-linker`
loads definitions and supplies one resolved-world context; the backend validates
source/WIT contracts and lowers canonical calls against that context. ABI lookup and
component assembly consume the same catalog.

This document owns source WIT bindings, the ABI registry, argument classification
and flattening, result recovery, signature validation, and the ownership rules
at the boundary. It does not own the linear-memory layout and allocator used by
the boundary ([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)),
the structured encoding of the resulting calls ([Wasm encoding](encoding-and-structuring.md)),
which WASI interfaces are enabled ([capability profile](capability-profile.md)),
or the component world and library modules ([WASI platform library](wasi-platform-library.md)).

## Background

**WIT.** WIT is the Component Model's interface definition language. A `world`
declares the interfaces a component imports and exports. An interface declares
`func`s over types such as `bool`, integer and float scalars, `char`, `string`,
`list<T>`, `record`, `tuple`, `variant`, `option`, `result`, `enum`, `flags`,
and resources (`own<T>`, `borrow<T>`). WASI publishes its host interfaces as WIT
packages, for example `wasi:cli`, `wasi:io`, and `wasi:clocks`.

**The Canonical ABI.** A core module and a component exchange aggregate values
by *flattening* them to core Wasm values or, when the flattened form is too
large, by passing a pointer to a record in linear memory. `lift`/`lower` convert
between the component and core views. A flattened result that does not fit in
one core value is written through a trailing *return pointer* to a return area.
A component that receives an allocated list or string needs an exported
`cabi_realloc` so the host can allocate in guest memory. Values a component
receives through the boundary may need explicit release through a post-return
action or a resource `drop`.

**Why a generic adapter.** Hand-coding WASI functions in the compiler bakes a
host ABI, a string representation, and a per-function recipe into the backend,
and adding a service means editing the compiler. [DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md)
instead makes WASI itself the runtime interface and lowers WIT imports
type-directedly, so the library is ordinary source code.

## Model

Two side tables carry the boundary. Neither is part of CC or MIR. The type
checker produces a checked `ExternalType` for each WIT external import; aliases
are expanded there, and `ForAll` quantifiers remain in the checked type graph.
P8 joins that scheme to the raw external's target binding and produces one
[`ResolvedExternal`](#resolved-externals) per WIT declaration, pairing the
checked source type with the WIT descriptor. The declaring `source_module`
is captured before linking and preserved in the external binding: foreign
symbols use the reserved intrinsic namespace, which is not their source owner.
Binding diagnostics use this explicit owner and the original source span.

Class-constrained WIT value signatures are currently unsupported: the checker
reports `UnsupportedType` on the source signature rather than dropping its
constraint evidence or inventing a canonical ABI for dictionary parameters.
Quantified signatures without class constraints retain their `ForAll` structure.

```text
ExternalType = { symbol: SymbolId, source_module: ModuleId, ty: TypeId }
ExternalBindings = { imports: [ResolvedExternal] }
ResolvedExternal = { symbol: SymbolId, interface: String, function: String,
                     checked_type: TypeId, import: WasiImport }

WasiRegistry = { resolve: wit_parser::Resolve,
                 imports: [WasiImport],
                 keys: [(String, String) -> usize],
                 target: TargetCapabilities }

WasiImport = { symbol: SymbolId, module: String, name: String,
               parameters: [ValueType], param_kinds: [WasiParamKind],
               result: Option<ValueType>, result_kind: WasiResultKind,
               unsupported: Option<String>, retptr: bool }

WasiParamKind = Integer32 | IntegerNarrow { bits: 8 | 16, signed: bool }
              | Boolean | Char | Scalar64 { signed: bool }
              | Float32 | Float64
              | Enum { cases: [String] }
              | Flags { names: [String] }
              | Record { fields: [WasiField] }
              | ValueList { element: Box(WasiParamKind) }
              | Handle | List | Unsupported

WasiResultKind = None | Scalar | Boolean | Enum { cases: [String] }
               | Char | List | Result
               | IntegerNarrow { bits: 8 | 16, signed: bool }
               | ValueList { element: Box(WasiParamKind) }
               | Discarded

WasiField      = { name: String, kind: WasiParamKind }
```

P8 joins each `ExternalKind::Wit { interface, function }` to its checked
`ExternalType` by `SymbolId`, resolves the vendored WIT once, and produces a
`ResolvedExternal`. `checked_type` is the synonym-expanded source scheme from
the Core type table, with its `ForAll` quantifiers preserved, so CC can derive
its layout from the shared representation table instead of re-elaborating the
raw HIR annotation. `import` is the WIT descriptor that carries the ABI
facts the source type cannot: numeric width, `string` versus `list<u8>`,
flattening, `retptr`, and `own`/`borrow` ownership. A declaration the
source ABI cannot express yields no `ResolvedExternal` and is rejected.

`List` is WIT `string`. It flattens to a `(pointer, length)` pair of UTF-8
bytes and its source type is `String`
([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
`list<u8>` is not that kind: it is a `ValueList` of bytes whose source type is
`Array Int`. `ValueList { element }` is any `list<T>`, including `list<u8>`.
It also flattens to `(pointer, length)`, but the pointer addresses an array of
canonically laid-out `element` values. Its resolved source type is
`Array(element)`; the descriptor is named `ValueList` rather than `Array` so it
is not confused with the GC array that carries the source value.

### Invariants

- A `WasiImport` is interned once per `(interface, function)`; its `symbol` is
  stable in the reserved intrinsic module above the runtime ranges.
- `WasiImport::parameters` is the canonical signature from
  `Resolve::wasm_signature(AbiVariant::GuestImport)`; `param_kinds` is aligned
  with the WIT-level parameter list, including a method's receiver.
- For direct parameters, `param_kinds` flatten to the canonical parameter
  count. When `Resolve::wasm_signature` selects indirect parameters, the core
  signature contains one pointer, followed by the return pointer when `retptr`
  is set; any mismatch is recorded as `unsupported`, never approximated.
- A binding is validated against its resolved source type before CC or MIR is
  emitted; a mismatched arity or type is a source diagnostic.
- WIT interface names, function names, and canonical signatures never appear in
  CC or MIR; MIR records only the interned symbol and its canonical value types.

## Design

### Declaring an import

A value provided by WIT is declared with a binding string in source:

```purescript
foreign import "wasi:clocks/monotonic-clock#now" now :: Int
```

The string is `<interface>#<function>`. The declaration's source name is
unrelated to the WIT name, and its checked external type is mapped to the canonical
signature through the standard type mapping. There is a single external kind,
`Wit`; the compiler has no per-function host registry. `ExternalBindings` is
built from Core's checked `ExternalType` schemes and target-binding metadata,
then checked against Core (`validate_core`) and CC's abstract signatures
(`validate_cc`). It never re-elaborates the raw HIR type annotation.

### Source type mapping

The source ABI maps WIT types to source types with bit-preserving semantics; it
never invents source values.

| WIT type | Source type | Notes |
| --- | --- | --- |
| `bool` | `Boolean` | canonical `0`/`1`. |
| `s8`, `s16`, `s32` | `Int` | signed; a narrower width sign-extends at the boundary. |
| `u8`, `u16`, `u32` | `Int` | unsigned bits in the low 32 bits; a caller passes the low `bits`. |
| `s64`, `u64` | `Int` | widened to/from `i64`; `Int` keeps the low 32 bits on return. |
| `f32`, `f64` | `Number` | `f32` narrows/widens at the boundary. |
| `char` | `Char` | both are canonical `i32`; an `Int` is rejected. |
| `string` | `String` | a Unicode scalar sequence stored as canonical UTF-8; incoming bytes are strictly validated and valid bytes are copied without transcoding. |
| `list<u8>` | `Array Int` | byte elements are zero-extended on input and range-checked to `0..255` before narrowing on output; never decoded as text. |
| other `list<T>` | `Array(T)` | element-wise; not a byte list. |
| `record` | `Record` | canonical field order by label. |
| nullary `enum` | `Enum` | case order must match. |
| `flags` | `Record` of `Boolean` | packed least-significant first. |
| resource handle | opaque handle | `own<T>` transfers ownership with a drop obligation; `borrow<T>` is a call-scoped non-owning reference. |
| `tuple<A, B, ...>` | closed record | `{ _1 :: A, _2 :: B, ... }`. |
| `option<T>` | `Data.Maybe.Maybe T` | recognized by the qualified type name. |
| `result<O, E>` | `Data.Either.Either E O` | recognized by the qualified type name; the error is `Left`, the ok value `Right`. |
| other `variant { ... }` | source data type | constructors in WIT case order. |

Two rules follow from the source language having no unsigned or narrowed integer
types:

- Every WIT integer maps to source `Int` and is bit-preserving. For a
  parameter, lowering masks a narrow (`u8`/`s8`/`u16`/`s16`) argument to its
  width so the canonical value is always in range; a 32-bit argument passes
  unchanged. For a result, the canonical `i32` already carries the in-range
  value, so no conversion is needed. A source program that wants unsigned
  interpretation of a returned bit pattern is outside this contract.
- `Array` is produced for WIT `list<T>`, including `list<u8>`. For `list<u8>`,
  each canonical byte is zero-extended to an `Int` when recovering an array; a
  source `Array Int` is range-checked element by element before an outgoing
  value is narrowed to one byte. String validation and operations never apply
  to this list.

[DEC-13](../../../decision/DEC-13-wit-to-source-type-mapping.md) maps the
aggregate WIT forms to the idiomatic library types: a tuple to a closed record,
`option<T>` to `Data.Maybe.Maybe T`, `result<O, E>` to
`Data.Either.Either E O`, and a non-unit `variant` to a source data type whose
constructors follow the WIT case order. `Maybe` and `Either` are recognized by
their qualified names, never by constructor shape. Every `result` maps to an
`Either` with the error on `Left`, including a unit-success or unit-error one:
an absent payload position is a nullary WIT case whose source field is `Unit`,
so `result<_, E>` is `Either E Unit`, `result<O, _>` is `Either Unit O`, and
plain `result` is `Either Unit Unit`. A canonical `result` orders its cases
`[ok, err]` while `Either` orders its constructors `Left`(err), `Right`(ok), so
lowering swaps the discriminant. There is no `Unit`/trap special case. A
standard-library
wrapper may still pass one of those forms as a sequence of primitive arguments
(`Int`, `Boolean`, `Number`, `Char`, `String`, `Unit`, with a handle declared as
`Int`) whose flattening equals `Resolve::wasm_signature` for that function
([primitive FFI and the standard library](primitive-ffi-and-stdlib.md)).
A canonical result that is several values stays unsupported until that whole
result is one primitive or a mapped aggregate.

### Resolving and validating

The linking stage resolves each `ExternalKind::Wit` import against the vendored
WASI 0.2.12 WIT once, before CC lowering. Resolution:

- finds the package and interface, then the WIT function;
- computes the canonical signature with `Resolve::wasm_signature`;
- classifies each WIT parameter and the result into the WIT descriptor;
- joins the external's checked `ExternalType` by symbol instead of rebuilding
  its source scheme from the HIR annotation;
- records an `unsupported` reason when the shape has no source mapping, when a
  list is not byte-valued, when flattening does not agree with the canonical
  signature, or when the interface's package is disabled by the target; and
- interns the import and returns a `ResolvedExternal`.

The stage validates the checked source type against the WIT descriptor. A
failure is reported against the declaration's source location; a declaration
fails even when dead code never calls it, because the side table is validated
eagerly.

### Lowering a call

`mir::wit::lower` lowers a call from the declared arguments, the resolved source
type, and the import's WIT descriptor. It flattens arguments into canonical
values, appends a return pointer when `retptr` is set, emits the call, and
recovers the result. All adaptation instructions are ordinary MIR operations:

- **Parameters.** Scalars and handles push directly; `Float32` narrows
  (`f64 -> f32`); `Scalar64` widens (`i64.extend_i32_s/u` by WIT signedness);
  `IntegerNarrow` masks to its width; a `String`/`list` copies its bytes into a
  transient linear buffer and pushes `(pointer, length)`; a `Record` projects
  fields in WIT order and recurses; `Flags` packs Boolean fields into one or
  more `i32` words in WIT declaration order. The buffer is freed when the call
  returns.
- **Return pointer.** A return area that fits the 16-byte reserved scratch
  region uses the scratch address (`PRINT_SCRATCH = 0`) as the last argument. A
  larger aggregate return area is allocated through `cabi_realloc`, its pointer
  is passed as the last argument, and it is freed once the result is read.
- **Results.** A scalar `i64` is wrapped to `Int`, an `f32` widened to `Number`,
  an `i32`/`f64` used directly, and a `list`/`string` read from the return area
  as `(pointer, length)`, copied into a fresh GC value, and the linear buffer
  freed. A `result` reads the one-byte canonical discriminant, decodes the
  selected payload when present, and builds the mapped `Either`: the canonical
  `ok` case (discriminant `0`) builds `Right`, and the canonical `err` case
  (discriminant `1`) builds `Left`, so the source error lands on `Left`. An
  absent payload position builds the corresponding `Unit` field. This is exactly
  like any other mapped result except for the swapped tag.
- **Aggregate parameters and results.** A mapped `option`, `result`, or
  `variant` carries a canonical discriminant followed by the joined payload
  slots. As a parameter the lowering branches on the guest constructor tag,
  flattens the payload that tag selects, pushes placeholder slots for the other
  case, and writes the canonical discriminant: `option`/`variant` keep tag
  order, while a `result` swaps it (`Left`/err is guest tag `0` and canonical
  discriminant `1`). As a result the lowering branches on the return-area
  discriminant, reads the selected payload, and builds the source value with
  `VariantNew`, using the swapped tag for a `result`. The payload
  is recovered into the stored `Maybe`/`Either`/data-type field; a scalar is
  boxed and a reference is cast, matching the erased aggregate field protocol.
  Each branch joins at a merge block that carries the canonical slots. A payload
  that is itself a record, byte list, or mapped aggregate is lowered
  recursively: the linking stage threads a `PayloadNode` tree with each
  position's concrete shape, so a nested record projects or builds its fields
  and a nested variant repeats the tag branch at its own payload offset.
- **Indirect aggregates.** When a mapped aggregate participates in an indirect
  parameter record, its canonical `(size, align, slots)` is computed the same
  way as any other WIT value and written through the record pointer.

### Exports and `post-return`

A guest export that returns a non-scalar value is lifted through the same
canonical ABI in reverse: the guest writes the value into its linear memory
through `cabi_realloc`, returns the return-area pointer, and the host reads and
copies the value. The compiler then synthesizes a `cabi_post_<name>` function
for that export that frees the return area and every buffer it owns. The
`wasi:cli/run` entry returns no aggregate, so it needs no `post-return`, but any
future list-returning export does. The `post-return` contract and its buffer
ownership are fixed by
[canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md).

### Resources and handles

A resource handle is an index into a guest-owned resource table. Under
[DEC-14](../../../decision/DEC-14-resource-handle-ownership.md) the standard
library owns the lifetime discipline: the compiler does not drop or release a
handle on its own and does not track ownership through aggregates. It exposes
the canonical `resource.drop` to source, so a `foreign import
"<interface>#[resource-drop]<resource>"` names the drop and the library calls it
at the right point. A handle nested in an aggregate is an ordinary value of the
aggregate; the library wrapper that destructures it drops each owned handle it
extracts. A `borrow<T>` in a result is rejected because the borrow scope is the
call that produced it, which has ended; a `borrow<T>` parameter stays
host-managed within the call. A handle owned by an export result is released by
the export's `post-return`. Handle types are declared in source with
`foreign import data`, which mirrors a WIT resource. The drop timing relative to
the buffer ownership classes is fixed by
[canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md).

### Rejected alternatives

- **A per-function host registry.** Rejected: adding a host function would edit
  the backend, and the standard library would not be library code
  ([DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md)).
- **A project-specific WIT runtime ABI (`psrs:runtime`).** Rejected: WASI already
  plays that role, and a second ABI would need its own design, versioning, and
  adaptation.
- **Hand-coding WASI Preview 1 (`fd_write`) in the backend.** Rejected: it bakes
  a WASI version, a string representation, and an iovec layout into the
  compiler.
- **Storing WIT metadata in CC or MIR.** Rejected: it would couple the
  target-neutral IRs to the component boundary. The names live only in
  `ExternalBindings` and the `WasiRegistry`.

## Algorithms

### Parameter flattening

```text
lower_parameters(import, resolved_source, args, flat):
    require len(args) == arity(resolved_source) == len(import.param_kinds)
    for (arg, source, kind) in zip(args, parameters(resolved_source), import.param_kinds):
        lower_parameter(arg, source, kind, flat)
    if canonical_signature.indirect_params:
        (pointer, size, align) = layout_parameter_record(import.param_kinds)
        address = cabi_realloc(0, 0, align, size)
        store_each_flattened_value(address, pointer, flat)
        flat = [address]

lower_parameter(arg, source, kind, flat):
    Integer32 | Boolean | Char | Float64 | Handle | Enum
        => flat.push(arg)
    IntegerNarrow { bits, signed }
        => flat.push(MaskToWidth(arg, bits))   # zero the bits above `bits`
    Float32
        => flat.push(F64ToF32(arg))
    Scalar64 { signed }
        => flat.push(WidenI64(arg, signed))
    Flags { names }
        => for each chunk of 32 names:
               word = 0
               for bit, name in chunk:
                   index = position of source_field_name(name) in source fields
                   boolean = project(arg, index)
                   word |= BoolToI32(boolean) << bit
               flat.push(word)
    List
        => length = Load(arg)                 # 4-byte length prefix
           bytes  = arg + 4
           flat.push(bytes); flat.push(length)
    ValueList { element }
        => # Non-byte list: copy the source array's elements into a fresh
           # linear-memory buffer via cabi_realloc, then pass (pointer, count).
           address = copy_array_to_memory(arg, element)
           flat.push(address); flat.push(ArrayLength(arg))
    Record { fields }
        => for field in fields (WIT order):
               index = position of source_field_name(field.name) in source fields
               value = project(arg, index)
               lower_parameter(value, source_field, field.kind, flat)
    Unsupported => error

layout_parameter_record(kinds):
    # Use Canonical ABI SizeAlign in WIT parameter order. Records recurse;
    # strings/lists occupy pointer and length words; flags use their canonical
    # integer representation. Each value's store width and alignment match its
    # in-memory Canonical ABI representation, including padding between fields.
    return the aligned tuple size, maximum field alignment, and field offsets
```

`resolved_source` is the declaration's resolved type at `type_id`. `arity`,
`parameters`, and `source_field` read its Core function and record structure; no
source-type mirror is consulted.

If `retptr` is set, the return-area pointer is appended after this indirect
parameter pointer. The allocated parameter bytes remain live for the duration
of the guest import call and are freed when the call returns
([DEC-10](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md)).

### Result recovery

```text
lower_result(import, destination, flat):
    if import.retptr: flat.push(Constant(PRINT_SCRATCH))
    match import.result_kind:
        List =>
            CallVoid(import, flat)
            pointer = Load(PRINT_SCRATCH)
            length = Load(PRINT_SCRATCH + 4)
            destination = validate_and_recover_internal_string(pointer, length)
        Scalar | Boolean | Enum | Char | IntegerNarrow { .. } =>
            match import.result:
                I64 => destination = WrapI64(Call(import, flat))
                F32 => destination = F32ToF64(Call(import, flat))
                I32 | F64 => destination = Call(import, flat)
        ValueList { element } =>
            # Non-byte list result: the host writes (pointer, count) into the
            # return area; read it back into a fresh source array element-wise.
            CallVoid(import, flat)
            pointer = Load(PRINT_SCRATCH)
            count   = Load(PRINT_SCRATCH + 4)
            destination = recover_array(pointer, count, element)
        None =>
            CallVoid(import, flat); destination = Constant(0)
        Result =>
            # A mapped result, decoded through the return-area discriminant.
            # The canonical cases are [ok, err]; the source Either swaps them,
            # so canonical ok builds Right and canonical err builds Left.
            CallVoid(import, flat)
            status = Load8U(PRINT_SCRATCH)
            switch status
                case 0 => destination = build_right(ok_payload_at PRINT_SCRATCH)
                case 1 => destination = build_left(err_payload_at PRINT_SCRATCH)
        Discarded => error
```

### Signature validation

```text
validate_import_signature(import, module, type_id):
    require arity(type_id) == len(import.param_kinds)
    for (source, kind) in zip(parameters(type_id), import.param_kinds):
        require core_matches_kind(module, source, kind)
    match import.result_kind:
        None      => require result(type_id) is Unit
        Scalar    => I64/I32 -> Int, F32/F64 -> Number
        Boolean   => Boolean
        Enum      => the source cases equal the WIT cases in order
        Char      => Char
        List      => String (WIT `string`, canonical UTF-8)
        IntegerNarrow { .. } => Int
        ValueList { element } => Array(source) whose element matches `element`
        Result    => the mapped Either source type (`Either err ok`)
        Discarded => always reject
```

### Edge cases

- A method parameter list includes the receiver handle, so the declared arity
  and the canonical arity differ by one; classification keeps them aligned.
- A `list<u8>` flattens to `(pointer, length)` while a scalar pushes one value.
  Its source type is `Array Int`, not `String`. The same expansion applies
  recursively to byte-list fields in directly flattened records;
  `flattened_parameter_count` is compared with the canonical signature to
  reject a shape the direct ABI cannot express.
- A WIT `char` maps only to source `Char` even though both use the canonical
  `i32` core type; an `Int` is rejected.
- A WIT `f32` parameter/result is narrowed/widened at the boundary because the
  source `Number` is `f64`; these are explicit MIR conversions.
- A source type that does not match its WIT form is rejected during binding
  validation, before MIR emits the call.

## Code map

The ABI layer is split between the WIT-binding side table, the MIR-side
adapter, and component packaging; [IR boundaries](../00-ir-boundaries.md) fixes
the crate-level tree. The implementation must conform to this organization:

```text
backend/src/
  abi/
    mod.rs             WasiRegistry, WasiImport, package gating
    classification.rs  WIT type classification and value types
    link/
      mod.rs            target binding lookup and conformance entry
      conformance.rs    checked Core type versus WIT signature
  bindings/
    mod.rs             ExternalBindings side table and boundary checks
  mir/
    wit/
      mod.rs           canonical call lowering and result recovery
      parameters/
        mod.rs         direct parameter flattening, records, flags
        indirect.rs    Canonical ABI parameter-record layout and stores
  linking/            checked target requirements and plan/diagnostic mapping
```

WIT definition loading and component packaging now live in `psrs-linker`, over
the pinned definitions owned by `psrs-runtime/wit/`.

`abi.rs` and `abi/` own resolving source WIT bindings and must define:

- `ExternalBindings` — the side table of the Model section, holding one
  `ResolvedExternal` per `ExternalKind::Wit` binding. `ResolvedExternal` pairs
  the declaration's checked Core `TypeId` from `Module.external_types` with its
  resolved `WasiImport`
  descriptor. It must provide `validate_core` and `validate_cc` for the P8/P9
  boundary checks, and `validate_conformance`, which resolves and validates
  every binding against the resolved Core type where Core is available.
- `abi/link/` — the target-aware conformance stage. It consumes the checked
  Core type ID, resolves the WIT import once, and validates the two sides with
  `validate_import_signature(import, module, checked_type)`. It does not parse
  or re-intern raw HIR type annotations.
- `WasiRegistry` — the interned `(interface, function)` registry, holding the
  `TargetCapabilities` it was loaded with. It must provide:
  - `load() -> Result<Self, String>` and
    `load_with_capabilities(target) -> Result<Self, String>`;
  - `import(&mut self, interface: &str, function: &str) -> Result<WasiImport, String>`,
    which interns on first use;
  - `imports(&self) -> &[WasiImport]`, `symbol_name(&self, symbol: SymbolId) -> Option<(&str, &str)>`,
    and `has_list_result(&self, symbol: SymbolId) -> bool`.
- `WasiImport::has_indirect_parameters()` identifies when the resolved canonical
  signature requires lowering the WIT parameter tuple through linear memory.
- `WasiImport`, `WasiParamKind`, `WasiResultKind`, and `WasiField` — the
  resolved canonical descriptor. `param_kinds` must stay aligned with the
  WIT-level parameter list, including a method receiver, and `unsupported` must
  record any shape outside the supported subset instead of approximating it.
- `abi/classification.rs` owns `param_kind`, `result_kind`, `value_type`, and
  `unsupported_shape`. Classification and flattening must agree with
  `Resolve::wasm_signature`, or the import must be rejected as unsupported.

`mir/wit/` owns the Canonical ABI adaptation and must provide:

```rust
pub(super) fn lower<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    type_id: TypeId,
    module: &CoreModule,
    destination: ValueId,
    arguments: &[ValueId],
    span: TextRange,
    current: BlockId,
) -> Result<(), Vec<BackendError>>;
```

It flattens arguments into canonical parameters, appends the return pointer when
`retptr` is set, emits the call, and recovers the result; `parameters.rs` owns
record and flags flattening. Every adaptation instruction must be an ordinary
MIR operation, and no WIT name, interface, or canonical signature may enter MIR.

`psrs-linker` owns WIT definition loading and composition and provides:

```rust
pub fn resolve_default_definitions() -> Result<ResolvedWorldContext, LinkErrors>;
pub fn compose(context: &ResolvedWorldContext, plan: &CheckedLinkPlan, application: &[u8])
    -> Result<LinkedArtifact, LinkErrors>;
```

The backend's `abi` module loads the same pinned WIT from the `psrs-runtime`
catalog for isolated ABI fixtures and derives the supported-interface set from
the resolved default world. `abi` owns the
`wasi_interface_enabled(target, interface)` package gate consulted before
resolving a WASI import ([capability profile](capability-profile.md)).
Dependencies must stay one-directional: `abi` and `mir/wit` may depend on
`capability` and shared types; the backend `linking` module may depend on `abi`
and `capability`; none may depend on the front end.

## Invariants and verification

- Every source WIT external appears exactly once in `ExternalBindings`; a missing,
  extra, or duplicate binding is a P8/P9 error (`validate_core`, `validate_cc`).
- The declared source signature matches the resolved WIT form in arity and type;
  otherwise the linking boundary fails before CC/MIR emits anything.
- Flattening is checked against the canonical signature, so an unsupported shape
  is rejected rather than emitted with a lossy approximation.
- MIR verifier checks canonical import calls against the MIR import table, and
  memory operations use `MemoryId(0)` with an `i32` address
  ([MIR](../fp/mir.md), [linear memory boundary](linear-memory-and-canonical-abi-boundary.md)).
- WIT names, interfaces, and worlds are confined to `ExternalBindings`, the
  `WasiRegistry`, and component metadata; CC and MIR contain only interning
  symbols and canonical value types ([IR boundaries](../00-ir-boundaries.md)).

## Worked example

Consider an import

```purescript
foreign import "wasi:io/streams#[method]output-stream.blocking-write-and-flush"
  writeStdout :: Int -> String -> Either StreamError Unit
```

and a call `writeStdout handle message`. Resolution yields `module =
"wasi:io/streams@0.2.12"`, `name =
"[method]output-stream.blocking-write-and-flush"`, `param_kinds = [Handle,
List]` (the `String` is a byte list), `retptr = true`, and `result_kind =
Result` (the WIT function returns `result<_, stream-error>`; DEC-13 maps it to
`Either StreamError Unit`). Lowering `writeStdout handle message` emits:

```text
v_len   = Load [0] message            # message[0..4] is the byte length
v_bytes = message + 4
v_ret   = Constant 0                  # PRINT_SCRATCH return pointer
CallVoid writeStdout(handle, v_bytes, v_len, v_ret)
v_stat  = Load8U [0] 0                # canonical result discriminant
switch v_stat
  case 0: result = VariantNew Right [Constant 0]       # ok -> Right ()
  case 1: payload = Decode stream-error at v_ret       # the error payload
          result = VariantNew Left [payload]           # err -> Left
```

A nonzero status no longer traps: the error payload is decoded and wrapped in
`Left`, so the source program observes the failure as an `Either` value with the
error first.

If the same program also used a `list`-returning import, P10 would additionally
synthesize and export `cabi_realloc` ([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)).

## Boundaries and interfaces

- **Input:** Core `ExternalType` schemes joined with target-binding metadata to
  produce `ExternalBindings`, the vendored WASI WIT, and the target profile.
- **Output:** a set of `ResolvedExternal`s and the `WasiRegistry` P9 hands to P10;
  MIR imports carry only the canonical symbol, parameters, and result.
- **To MIR:** canonical calls and adaptation instructions. The ABI layer decides
  how values flatten; MIR only executes the resulting operations.
- **To P10/P11:** the registry names each interned symbol as a core import
  `(module, field)`; the componentizer lifts the module against the app world
  ([WASI platform library](wasi-platform-library.md)).

## Open questions and future work

- **Aggregate ABI.** `option`/`result`/`variant` are classified and validated
  against `Maybe`/`Either`/a source data type, CC derives their variant
  representation and the concrete shape of every nested payload, and MIR
  branches on each tag and rebuilds the source value recursively
  ([Aggregate parameters and results](#lowering-a-call)). A payload is lowered
  when it is a scalar of any width (`s8`..`u64`, `f32`/`f64`), a byte or
  non-byte list, an enum, `flags`, a handle, a closed record, or a nested
  `option`/`result`/variant; the payload tree recurses through record fields and
  through a `list<record>`/`list<flags>` element, so a field or element that is
  itself an aggregate is encoded and decoded. A large aggregate return area is
  allocated through `cabi_realloc` rather than the fixed scratch region. A
  resource handle in a result is rejected with a named diagnostic. An indirect
  parameter record carries a mapped aggregate as its discriminant and joined
  payload. Narrowed and
  unsigned WIT integers and non-byte `list<T>` results have a source mapping and
  are lowered ([Source type mapping](#source-type-mapping)). A non-byte `list<T>`
  is copied element-wise between a source GC array and the canonical
  `(pointer, length)` buffer; `string` elements are validated UTF-8 copies and
  `list<u8>` elements are uninterpreted bytes. Their element buffers are freed.
- **Resources.** `own`/`borrow` handles are lowered
  ([Resources and handles](#resources-and-handles)): the compiler does not drop
  or release a handle on its own and exposes `resource.drop` to source, so the
  standard library owns the lifetime discipline
  ([DEC-14](../../../decision/DEC-14-resource-handle-ownership.md)). An export
  whose result is `own<T>` releases the handle in `cabi_post_<name>`; a
  `borrow<T>` result is rejected; a handle nested in an aggregate is an ordinary
  value the library wrapper drops. A source-declared
  `[resource-drop]<resource>` import lowers to the canonical drop.
- **Source integration.** Parsed source can declare `Array` foreign signatures
  and reaches the ABI boundary; non-byte lists of supported elements (scalars,
  `bool`, `char`, and `string`) are lowered, as is `list<u8>` as `Array Int`.
  Record and flags foreign signatures are not known to be reachable from
  parsed source.
- **Aggregate source mapping.** `option`, `result`, and non-unit `variant` map
  to `Data.Maybe.Maybe`, `Data.Either.Either`, and a source data type, and a
  tuple maps to a closed record
  ([DEC-13](../../../decision/DEC-13-wit-to-source-type-mapping.md)). Their
  classification, Core conformance validation, and CC representation are
  implemented; MIR/Wasm lowering remains. A standard-library wrapper may still
  pass one of those forms as a sequence of primitive arguments whose flattening
  matches the canonical signature; multi-value returns stay unsupported.
- **Filesystem loader.** The standard library remains embedded; the driver's
  loader discovers user modules from the entry files' directories and follows
  the import graph ([WASI platform library](wasi-platform-library.md)). Loading
  the standard library from disk stays future work.
- **Compositional, type-directed lowering.** The descriptor-classification and
  per-shape lowering internals described here — `WasiParamKind`,
  `WasiResultKind`, `FlatSlot`, `ListElement`, the CC payload tree, and the
  per-shape MIR modules — are superseded by one normalized recursive canonical
  type with generic flatten, size/align, store/load, lower/lift, and free-plan
  operations, specified in
  [compositional canonical ABI lowering](canonical-abi-compositional.md). That
  document replaces only those shape-enumerating internals; this document
  remains the owner of the WIT binding contract, the source type mapping, and
  the boundary validation.

### GC canonical ABI

The linear-memory canonical ABI exists because it must also serve non-GC
languages and hosts. The Component Model has an open pre-proposal
([WebAssembly/component-model#525](https://github.com/WebAssembly/component-model/issues/525))
to lower component types *directly to core Wasm GC types* behind a `gc`
canonical option plus a `core-type` option that names a component's at-rest core
function type. When it is present, a value crosses the boundary as a typed
reference and the receiver reads it with `struct.get`/`array.get`; there is no
linear buffer, no `cabi_realloc`, and no `post-return` for those values, and the
GC heap owns their lifetime. The proposed lowerings are:

| WIT | GC core type |
| --- | --- |
| `string` (utf8 / utf16) | `(ref null? (array (mut? i8)))` / `(ref null? (array (mut? i16)))` |
| `list<T>` | `(ref null? (array (mut? T')))` |
| `record`, `tuple`, `variant`, `option`, `result` | `(ref null? (struct ...))`, with subtyping |
| `own`, `borrow`, `future`, `stream`, `error-context` | `externref` |

This backend uses the utf8 row only. A source `String` is canonical UTF-8 in
`(array (mut i8))`, and the utf16 row is recorded here because it is part of the
proposal being summarized, not because the compiler accepts a second string
encoding ([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).

Zero copy is not automatic: mutability, rec-group identity, and not-yet-defined
width/depth subtyping can still force a copy when the two components' at-rest
representations disagree, which is exactly what the `core-type` option lets each
side declare. The extension is opt-in, so a component may use it while another
keeps the linear-memory ABI.

This project's `String` is the proposed UTF-8 GC array
(`(array (mut i8))`)
([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)), so if
the `gc` option and a supporting toolchain land, the string path could move to
GC lowering and drop linear memory and `cabi_realloc` for strings. Until then,
the linear-memory boundary and its
allocator ([canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md))
remain the required path. WASI 0.3 (async, `stream`, `future`) does not include
this extension, and `wit-component` 0.245 has no `gc` canonical option.

## Implementation notes

The current code deviates from the complete design in these ways; the gaps are
tracked on BE-11 and BE-17..BE-20 in [D-04](../../D-04-suite-roadmap.md) and are
implementation coverage, not design choices. The allocator, buffer free, and
`post-return` gaps are tracked specifically by
[canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md):

- The accepted `String` is a GC array of canonical UTF-8 bytes, copied at the
  boundary without transcoding, and a WIT `list<u8>` is `Array Int`
  ([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)). The
  backend implements both: GC strings are `(array (mut i8))`, the boundary
  validates text strictly, and a `list<u8>` is copied element by element with a
  `0..255` range check. The transient buffer is freed at the boundary either
  way.
- `cabi_realloc` is a reclaiming allocator. `wasi:cli/run` still returns a
  scalar, so that export has no `post-return`. An export whose canonical
  result is `own<T>` gets `cabi_post_<name>`, which calls `resource.drop` on
  the returned handle. An export whose result is lifted through a return area
  gets a `cabi_post_<name>` that frees the result's data buffer and the return
  area; the synthesis is implemented and verified with a synthesized
  `string`-returning export because no source construct names a non-scalar
  export yet.
- The compiler never drops or releases a handle on its own; the standard
  library calls `resource.drop` explicitly
  ([DEC-14](../../../decision/DEC-14-resource-handle-ownership.md)). A
  source-declared `[resource-drop]<resource>` import lowers to the canonical
  drop. A handle returned as `Int`, or nested in an aggregate, is an ordinary
  value; the library wrapper that consumes it drops it. The verifier only
  rejects dropping or transferring the same owned handle twice. A `borrow<T>`
  result is rejected because its scope has ended; a `borrow<T>` parameter stays
  host-managed within the call.
- `option`/`result`/`variant` are classified and validated against
  `Data.Maybe.Maybe`, `Data.Either.Either`, and a source data type, CC derives
  their variant representation and a `PayloadNode` tree with each payload's
  concrete shape, and MIR branches on the tag and rebuilds the source value
  recursively. A scalar payload of any width (`s8`..`u64`, `f32`/`f64`), a byte
  or non-byte list, an enum, `flags`, a handle, a closed record, and a nested
  `option`, `result`, or `variant` are lowered; the tree recurses through record
  fields and a `list<record>`/`list<flags>` element. A large aggregate return
  area is allocated through `cabi_realloc` and freed after the read; a resource
  handle in a result is rejected with a named diagnostic. Non-byte `list<T>` of a
  supported element is lowered.

Resolved bindings ([DEC-12](../../../decision/DEC-12-resolved-wit-bindings.md)):
each external's checked, synonym-expanded scheme is produced during type
checking and carried in `Module.external_types`, with `ForAll` quantifiers
preserved. `ExternalBindings::from_core` joins it to the WIT target binding and
carries its Core type identity beside that binding. CC derives its whole
abstract signature, including record and array representations, directly from
that Core type; the structural re-search (`core_type_matches_source`) and the
source-signature comparison are removed. WIT conformance validation runs at P8
against the checked Core type (`ExternalBindings::validate_conformance`,
`abi/link::conformance::validate_import_signature`). MIR lowering reads the
declaration's CC `Signature` (`ValueShape`) and projects record and flags fields
by label from the planned representation table; the WIT descriptor drives
canonical adaptation. `SourceType` and `SourceSignature` are deleted; the ABI
unit tests validate against the resolved Core type. The refactor is behavior
preserving and does not change the source language.

Implemented today: direct mappings for `bool`, `s32`, `s64`/`u64`, `f32`/`f64`,
`char`, narrowed/unsigned integers, nullary enums, byte lists (`String`), direct
records with nested byte-list fields, and flags words; indirect parameter tuples
through `cabi_realloc`; non-byte lists of scalars, `bool`, `char`, strings, nullary enums, flags, resource handles, and directly flattened records of scalar or string fields;
every `result` as a mapped `Either`, including unit-success/unit-error results,
and scalar/list result paths.
Regression tests cover those shapes.

## References

- [WebAssembly Component Model specification](https://github.com/WebAssembly/component-model/blob/main/design/mvp/Explainer.md#canonical-abi): WIT and the Canonical ABI
  (`lift`/`lower`, flattening, `realloc`, return pointer, post-return).
- WASI 0.2 WIT interfaces (`wasi:cli`, `wasi:io`, `wasi:clocks`, `wasi:random`).
- `wit-parser` `Resolve::wasm_signature`, `AbiVariant`.
- [DEC-06 — Runtime Interface via WASI and the Component Model](../../../decision/DEC-06-runtime-interface-via-wit.md),
  [DEC-10 — Canonical ABI Buffer Ownership and Lifetime](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md),
  [DEC-11 — Primitive Foreign Imports and Standard-Library Wrappers](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md),
  [DEC-12 — Resolved WIT Bindings and Descriptor-Based Lowering](../../../decision/DEC-12-resolved-wit-bindings.md),
  [DEC-05 — Target wasmtime's WebAssembly Feature Set](../../../decision/DEC-05-wasmtime-feature-set.md).
- [MIR](../fp/mir.md), [capability profile](capability-profile.md),
  [linear memory boundary](linear-memory-and-canonical-abi-boundary.md),
  [WASI platform library](wasi-platform-library.md).
