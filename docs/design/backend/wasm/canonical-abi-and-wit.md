# Canonical ABI and WIT

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Draft  
**Prerequisites:** the Component Model and its Canonical ABI (flattening, `lift`/`lower`, `realloc`, the return pointer and post-return), the WIT interface language (interfaces, worlds, records, variants, flags, enums, lists, resources), and the Wasm value types. Read [MIR](../fp/mir.md), the [capability profile](capability-profile.md), and [DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md) first.  
**Summary:** A source `foreign import` names a WIT function by an interface/name binding, and the backend adapts the call to the Canonical ABI generically instead of hand-coding host functions. The ABI layer resolves the canonical signature, flattens declared arguments, reads indirect results, and emits the adaptation as MIR; WIT names never enter CC or MIR.

## Scope

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

Two side tables carry the boundary. Neither is part of CC or MIR.

```text
ExternalBindings = { imports: [ExternalBinding] }
ExternalBinding  = { symbol: SymbolId, interface: String, function: String,
                     signature: Option(SourceSignature) }

SourceSignature = { parameters: [SourceType], result: SourceType, span: TextRange }
SourceType      = Int | Boolean | Number | Char | String | Unit
                | Enum { cases: [String] }
                | Record { fields: [(String, SourceType)] }
                | Array(SourceType)

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
              | Array { element: Box(WasiParamKind) }
              | Handle | List | Unsupported

WasiResultKind = None | Scalar | Boolean | Enum { cases: [String] }
               | Char | List | Result
               | IntegerNarrow { bits: 8 | 16, signed: bool }
               | Array { element: Box(WasiParamKind) }
               | Discarded

WasiField      = { name: String, kind: WasiParamKind }
BoundWasiImport = { import: WasiImport, signature: SourceSignature }
```

`ExternalBindings` is produced from Typed Core by `ExternalBindings::from_core`,
which keeps every `ExternalKind::Wit { interface, function }` and projects its
HIR signature into `SourceSignature`; a signature the source ABI cannot express
stays `None` and is rejected. `BoundWasiImport` pairs a resolved `WasiImport`
with the exact `SourceSignature` so P9 can recover record field order after CC
lowering.

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
- A binding is validated against its source signature before CC or MIR is
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
unrelated to the WIT name, and its declared type is mapped to the canonical
signature through the standard type mapping. There is a single external kind,
`Wit`; the compiler has no per-function host registry. `ExternalBindings` is a
lossless projection of the source externals, checked against Core
(`validate_core`) and against CC's abstract signatures (`validate_cc`).

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
| `string`, `list<u8>` | `String` | a GC byte-sequence value; copied to and from a transient linear `(pointer, length)` buffer at the boundary. |
| other `list<T>` | `Array(T)` | element-wise; not a byte list. |
| `record` | `Record` | canonical field order by label. |
| nullary `enum` | `Enum` | case order must match. |
| `flags` | `Record` of `Boolean` | packed least-significant first. |
| resource handle | opaque handle | `own<T>` transfers ownership with a drop obligation; `borrow<T>` is a call-scoped non-owning reference. |
| `option`, `result`, non-unit `variant`, tuple | none | not a compiler source type. A standard-library wrapper may pass one only as primitive arguments whose flattening matches the canonical signature. A multi-value return stays unsupported. |

Two rules follow from the source language having no unsigned or narrowed integer
types:

- Every WIT integer maps to source `Int` and is bit-preserving. For a
  parameter, lowering masks a narrow (`u8`/`s8`/`u16`/`s16`) argument to its
  width so the canonical value is always in range; a 32-bit argument passes
  unchanged. For a result, the canonical `i32` already carries the in-range
  value, so no conversion is needed. A source program that wants unsigned
  interpretation of a returned bit pattern is outside this contract.
- `Array` is only produced for a non-byte `list<T>` whose element maps. Byte
  lists stay `String`, so `list<u8>` never becomes `Array Int`.

`option`, `result`, non-unit `variant`, and tuple are not compiler source types.
The lowerer does not add a source type for them and does not recognize `Maybe`,
`Either`, or tuples. A standard-library wrapper may pass those forms only as a
sequence of primitive arguments (`Int`, `Boolean`, `Number`, `Char`, `String`,
`Unit`, with a handle declared as `Int`) whose flattening equals
`Resolve::wasm_signature` for that function
([primitive FFI and the standard library](primitive-ffi-and-stdlib.md),
[DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md)).
A canonical result that is several values stays unsupported until that whole
result is one primitive. The unit-success `result` keeps its existing `Unit`
mapping (trap on failure).

### Resolving and validating

For each binding, P9 asks the `WasiRegistry` to resolve the interface and
function against the vendored WASI 0.2.12 WIT. Resolution:

- finds the package and interface, then the WIT function;
- computes the canonical signature with `Resolve::wasm_signature`;
- classifies each WIT parameter and the result;
- records an `unsupported` reason when the shape has no source mapping, when a
  list is not byte-valued, when a `result` has a payload on success, when
  parameters are passed indirectly, when flattening does not agree with the
  canonical signature, or when the interface's package is disabled by the
  target; and
- interns the import and returns it.

P9 then validates the source declaration against the resolved import
(`WasiRegistry::validate_signature`). A failure is reported against the
declaration's span; a declaration fails even when dead code never calls it,
because the side table is validated eagerly.

### Lowering a call

`mir::wit::lower` lowers a call from the declared arguments, the source
signature, and the import's classification. It flattens arguments into canonical
values, appends a return pointer when `retptr` is set, emits the call, and
recovers the result. All adaptation instructions are ordinary MIR operations:

- **Parameters.** Scalars and handles push directly; `Float32` narrows
  (`f64 -> f32`); `Scalar64` widens (`i64.extend_i32_s/u` by WIT signedness);
  `IntegerNarrow` masks to its width; a `String`/`list` copies its bytes into a
  transient linear buffer and pushes `(pointer, length)`; a `Record` projects
  fields in WIT order and recurses; `Flags` packs Boolean fields into one or
  more `i32` words in WIT declaration order. The buffer is freed when the call
  returns.
- **Return pointer.** A scratch address (`PRINT_SCRATCH = 0`, inside the 16-byte
  reserved scratch region) is passed as the last argument.
- **Results.** A scalar `i64` is wrapped to `Int`, an `f32` widened to `Number`,
  an `i32`/`f64` used directly, and a `list`/`string` read from the return area
  as `(pointer, length)`, copied into a fresh GC value, and the linear buffer
  freed. A
  `result<_, _>` with a unit success payload reads the one-byte discriminant and
  traps on a nonzero status rather than silently succeeding.

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

A resource handle is an index into a guest-owned resource table. `own<T>`
transfers ownership: the receiver must eventually `resource.drop` it, and the
lowering inserts that drop when the owning value is consumed. `borrow<T>` is a
non-owning reference whose borrow must not outlive the call; the lowering
releases the borrow when the call returns. A handle owned by an export result is
released by the export's `post-return`. Handle types are declared in source with
`foreign import data`, which mirrors a WIT resource. The drop and borrow-release
timing relative to the buffer ownership classes is fixed by
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
lower_parameters(import, source_signature, args, flat):
    require len(args) == len(source_signature.parameters) == len(import.param_kinds)
    for (arg, source, kind) in zip(args, source_signature.parameters, import.param_kinds):
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
    Array { element }
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
        Array { element } =>
            # Non-byte list result: the host writes (pointer, count) into the
            # return area; read it back into a fresh source array element-wise.
            CallVoid(import, flat)
            pointer = Load(PRINT_SCRATCH)
            count   = Load(PRINT_SCRATCH + 4)
            destination = recover_array(pointer, count, element)
        None =>
            CallVoid(import, flat); destination = Constant(0)
        Result =>
            CallVoid(import, flat)
            status   = Load8U(PRINT_SCRATCH)
            failed   = status != 0
            TrapIf(failed)
            destination = Constant(0)
        Discarded => error
```

### Signature validation

```text
validate_signature(import, signature):
    require len(signature.parameters) == len(import.param_kinds)
    for (source, kind) in zip(signature.parameters, import.param_kinds):
        require source_parameter_matches(source, kind)
    match import.result_kind:
        None      => require signature.result is Unit
        Scalar    => I64/I32 -> Int, F32/F64 -> Number
        Boolean   => Boolean
        Enum      => the source cases equal the WIT cases in order
        Char      => Char
        List      => String (byte list)
        IntegerNarrow { .. } => Int
        Array { element } => Array(source) whose element matches `element`
        Result    => Unit
        Discarded => always reject
```

### Edge cases

- A method parameter list includes the receiver handle, so the declared arity
  and the canonical arity differ by one; classification keeps them aligned.
- A `list<u8>` flattens to `(pointer, length)` while a scalar pushes one value.
  The same expansion applies recursively to byte-list fields in directly
  flattened records; `flattened_parameter_count` is compared with the canonical
  signature to reject a shape the direct ABI cannot express.
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
  abi.rs               WasiRegistry, WasiImport, SourceSignature,
                       validate_signature, package gating
  abi/
    classification.rs  WIT type classification and value types
  bindings.rs          ExternalBindings side table and boundary checks
  mir/
    wit/
      mod.rs           canonical call lowering and result recovery
      parameters/
        mod.rs         direct parameter flattening, records, flags
        indirect.rs    Canonical ABI parameter-record layout and stores
  component.rs         vendored WIT loading and component packaging
```

`abi.rs` and `abi/` own resolving source WIT bindings and must define:

- `ExternalBindings`, `ExternalBinding`, `SourceSignature`, and `SourceType` —
  the side table of the Model section, with
  `ExternalBindings::from_core` keeping every `ExternalKind::Wit` binding and
  projecting its HIR signature. It must provide `validate_core` and
  `validate_cc` for the P8/P9 boundary checks.
- `WasiRegistry` — the interned `(interface, function)` registry, holding the
  `TargetCapabilities` it was loaded with. It must provide:
  - `load() -> Result<Self, String>` and
    `load_with_capabilities(target) -> Result<Self, String>`;
  - `import(&mut self, interface: &str, function: &str) -> Result<WasiImport, String>`,
    which interns on first use;
  - `imports(&self) -> &[WasiImport]`, `symbol_name(&self, symbol: SymbolId) -> Option<(&str, &str)>`,
    and `has_list_result(&self, symbol: SymbolId) -> bool`;
  - `validate_signature(&self, import: &WasiImport, signature: &SourceSignature) -> Result<(), String>`.
- `WasiImport::has_indirect_parameters()` identifies when the resolved canonical
  signature requires lowering the WIT parameter tuple through linear memory.
- `WasiImport`, `WasiParamKind`, `WasiResultKind`, and `WasiField` — the
  resolved canonical descriptor. `param_kinds` must stay aligned with the
  WIT-level parameter list, including a method receiver, and `unsupported` must
  record any shape outside the supported subset instead of approximating it.
- `abi/classification.rs` owns `param_kind`, `result_kind`, `source_signature`,
  `value_type`, and `unsupported_shape`. Classification and flattening must
  agree with `Resolve::wasm_signature`, or the import must be rejected as
  unsupported.

`mir/wit/` owns the Canonical ABI adaptation and must provide:

```rust
pub(super) fn lower<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    source_signature: &abi::SourceSignature,
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

`component.rs` owns vendored WIT loading and packaging and must provide:

```rust
pub fn load_vendored_wasi(resolve: &mut Resolve) -> Result<(), String>;
pub fn command_world() -> Result<(Resolve, WorldId), String>;
pub fn componentize(core: &[u8], resolve: &Resolve, world: WorldId) -> Result<Vec<u8>, String>;
```

It also owns the supported-interface set. `abi.rs` owns the
`wasi_interface_enabled(target, interface)` package gate consulted before
resolving a WASI import ([capability profile](capability-profile.md)).
Dependencies must stay one-directional: `abi` and `mir/wit` may depend on
`capability` and shared types; `component` may depend on `abi` and `capability`;
none may depend on the front end.

## Invariants and verification

- Every source WIT external appears exactly once in `ExternalBindings`; a missing,
  extra, or duplicate binding is a P8/P9 error (`validate_core`, `validate_cc`).
- The declared source signature matches the resolved WIT form in arity and type;
  otherwise P9 fails before CC/MIR emits anything.
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
  writeStdout :: Int -> String -> Unit
```

and a call `writeStdout handle message`. Resolution yields `module =
"wasi:io/streams@0.2.12"`, `name =
"[method]output-stream.blocking-write-and-flush"`, `param_kinds = [Handle,
List]` (the `String` is a byte list), `retptr = true`, and `result_kind =
Result` (the WIT function returns `result<_, stream-error>` with a unit success
payload). Lowering `writeStdout handle message` emits:

```text
v_len   = Load [0] message            # message[0..4] is the byte length
v_bytes = message + 4
v_ret   = Constant 0                  # PRINT_SCRATCH return pointer
CallVoid writeStdout(handle, v_bytes, v_len, v_ret)
v_stat  = Load8U [0] 0                # canonical result discriminant
v_bad   = v_stat != 0
TrapIf v_bad
result  = Constant 0                  # Unit
```

If the same program also used a `list`-returning import, P10 would additionally
synthesize and export `cabi_realloc` ([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)).

## Boundaries and interfaces

- **Input:** Typed Core externals projected into `ExternalBindings`, the vendored
  WASI WIT, and the target profile.
- **Output:** a set of `BoundWasiImport`s and the `WasiRegistry` P9 hands to P10;
  MIR imports carry only the canonical symbol, parameters, and result.
- **To MIR:** canonical calls and adaptation instructions. The ABI layer decides
  how values flatten; MIR only executes the resulting operations.
- **To P10/P11:** the registry names each interned symbol as a core import
  `(module, field)`; the componentizer lifts the module against the app world
  ([WASI platform library](wasi-platform-library.md)).

## Open questions and future work

- **Aggregate ABI.** Indirect records and tuples, `option`/`result`/`variant`
  results, `option`/`result`/`variant` payloads, and non-byte `list<T>` need
  result memory-layout computation and read-back. Narrowed and unsigned WIT
  integers and non-byte lists of scalars now have a source mapping
  ([Source type mapping](#source-type-mapping)); indirect parameter records are
  implemented for the currently classified parameter kinds.
- **Resources.** `own`/`borrow` handles are lowered
  ([Resources and handles](#resources-and-handles)): an owned import result is
  dropped with `resource.drop` when the receiving function does not return it
  and does not pass it to an `own` parameter; a borrow result is released when
  that call returns; an export whose result is `own<T>` releases the handle in
  `cabi_post_<name>`. An owned handle returned as `Int` from a non-export
  function is not tracked in the caller. Handles nested in an unsupported
  aggregate are not dropped.
- **Source integration.** Parsed source can declare `Array` foreign signatures
  and reaches the ABI boundary, but non-byte lists are not lowered yet.
  Record and flags foreign signatures are not known to be reachable from
  parsed source.
- **No compiler source type for option/result/variant/tuple.** Those WIT forms
  are not compiler source types
  ([DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md)).
  A standard-library wrapper may pass them only as a sequence of primitive
  arguments whose flattening matches the canonical signature; multi-value
  returns stay unsupported.
- **Filesystem loader.** The standard library remains embedded; the driver's
  loader discovers user modules from the entry files' directories and follows
  the import graph ([WASI platform library](wasi-platform-library.md)). Loading
  the standard library from disk stays future work.

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

Zero copy is not automatic: mutability, rec-group identity, and not-yet-defined
width/depth subtyping can still force a copy when the two components' at-rest
representations disagree, which is exactly what the `core-type` option lets each
side declare. The extension is opt-in, so a component may use it while another
keeps the linear-memory ABI.

This project's `String` is already the proposed UTF-16 GC array
(`(array (mut i16))`), so if the `gc` option and a supporting toolchain land,
the string path could move to GC lowering and drop linear memory and
`cabi_realloc` for strings. Until then, the linear-memory boundary and its
allocator ([canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md))
remain the required path. WASI 0.3 (async, `stream`, `future`) does not include
this extension, and `wit-component` 0.245 has no `gc` canonical option.

## Implementation notes

The current code deviates from the complete design in these ways; the gaps are
tracked on BE-11 and BE-17..BE-20 in [D-04](../../D-04-suite-roadmap.md) and are
implementation coverage, not design choices. The allocator, buffer free, and
`post-return` gaps are tracked specifically by
[canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md):

- A source `String` is now a GC byte-sequence value; the ABI adapter transcodes
  it to and from the component's UTF-8 through the reclaiming `cabi_realloc`,
  and frees the transient buffer at the boundary.
- `cabi_realloc` is a reclaiming allocator. `wasi:cli/run` still returns a
  scalar, so that export has no `post-return`. An export whose canonical
  result is `own<T>` gets `cabi_post_<name>`, which calls `resource.drop` on
  the returned handle. An export whose result is lifted through a return area
  gets a `cabi_post_<name>` that frees the result's data buffer and the return
  area; the synthesis is implemented and verified with a synthesized
  `string`-returning export because no source construct names a non-scalar
  export yet.
- An owned handle that a non-export function returns as `Int` is not dropped
  in that function and is not tracked after the return. Handles nested inside
  an unsupported aggregate are not dropped. A borrow result is released by
  `resource.drop` immediately after the import returns; a later use is
  rejected. An owned handle is dropped once in the function that received it,
  unless that function returns the index or passes it to an `own` parameter.
  A second drop, or a use after the borrow release, is rejected.
- Non-byte `list<T>`, `option`/`result`/`variant` payload read-back, and tuple
  source types are not lowered.

Implemented today: direct mappings for `bool`, `s32`, `s64`/`u64`, `f32`/`f64`,
`char`, narrowed/unsigned integers, nullary enums, byte lists (`String`), direct
records with nested byte-list fields, and flags words; indirect parameter tuples
through `cabi_realloc`; the unit-success `result` and scalar/list result paths.
Regression tests cover those shapes.

## References

- [WebAssembly Component Model specification](https://github.com/WebAssembly/component-model/blob/main/design/mvp/Explainer.md#canonical-abi): WIT and the Canonical ABI
  (`lift`/`lower`, flattening, `realloc`, return pointer, post-return).
- WASI 0.2 WIT interfaces (`wasi:cli`, `wasi:io`, `wasi:clocks`, `wasi:random`).
- `wit-parser` `Resolve::wasm_signature`, `AbiVariant`.
- [DEC-06 — Runtime Interface via WASI and the Component Model](../../../decision/DEC-06-runtime-interface-via-wit.md),
  [DEC-10 — Canonical ABI Buffer Ownership and Lifetime](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md),
  [DEC-05 — Target wasmtime's WebAssembly Feature Set](../../../decision/DEC-05-wasmtime-feature-set.md).
- [MIR](../fp/mir.md), [capability profile](capability-profile.md),
  [linear memory boundary](linear-memory-and-canonical-abi-boundary.md),
  [WASI platform library](wasi-platform-library.md).
