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

WasiRegistry = { resolve: wit_parser::Resolve,
                 imports: [WasiImport],
                 keys: [(String, String) -> usize],
                 target: TargetCapabilities }

WasiImport = { symbol: SymbolId, module: String, name: String,
               parameters: [ValueType], param_kinds: [WasiParamKind],
               result: Option<ValueType>, result_kind: WasiResultKind,
               unsupported: Option<String>, retptr: bool }

WasiParamKind = Integer32 | Boolean | Char | Scalar64 { signed: bool }
              | Float32 | Float64
              | Enum { cases: [String] }
              | Flags { names: [String] }
              | Record { fields: [WasiField] }
              | Handle | List | Unsupported

WasiResultKind = None | Scalar | Boolean | Enum { cases: [String] }
               | Char | List | Result | Discarded

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
- `param_kinds` flatten to exactly the canonical parameter count plus `retptr`;
  a mismatch is recorded as an `unsupported` reason, never approximated.
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
  (`f64 -> f32`); `Scalar64` widens (`i64.extend_i32_s/u` by WIT signedness); a
  `String`/`list` pushes its data pointer and length; a `Record` projects fields
  in WIT order and recurses; `Flags` packs Boolean fields into one or more `i32`
  words in WIT declaration order.
- **Return pointer.** A scratch address (`PRINT_SCRATCH = 0`, inside the 16-byte
  reserved scratch region) is passed as the last argument.
- **Results.** A scalar `i64` is wrapped to `Int`, an `f32` widened to `Number`,
  an `i32`/`f64` used directly, and a `list`/`string` read from the return area
  as `(pointer, length)`, checked against the guest allocator's prefix, and
  converted to the internal string pointer. A zero-length null result uses the
  static empty-string buffer. A
  `result<_, _>` with a unit success payload reads the one-byte discriminant and
  traps on a nonzero status rather than silently succeeding.

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

lower_parameter(arg, source, kind, flat):
    Integer32 | Boolean | Char | Float64 | Handle | Enum
        => flat.push(arg)
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
    Record { fields }
        => for field in fields (WIT order):
               index = position of source_field_name(field.name) in source fields
               value = project(arg, index)
               lower_parameter(value, source_field, field.kind, flat)
    Unsupported => error
```

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
        Scalar | Boolean | Enum | Char =>
            match import.result:
                I64 => destination = WrapI64(Call(import, flat))
                F32 => destination = F32ToF64(Call(import, flat))
                I32 | F64 => destination = Call(import, flat)
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
      parameters.rs    parameter flattening, records, flags
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
  payloads, and non-byte `list<T>` need memory-layout computation and read-back.
- **Resources.** `own`/`borrow` handles need `drop` insertion and lifetime rules;
  ownership and post-return reclamation are specified but not implemented.
- **Source integration.** The type checker still rejects record type signatures,
  so the record and flags paths are not yet reachable from source.
- **Allocator reclamation.** Returned lists and owned resources are currently
  served by a bump allocator and leak
  ([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)).
- **Filesystem loader.** The standard library is embedded in the driver; a real
  module loader would let it be discovered like any module.

## Implementation notes

Exact mappings are implemented for `bool`, `s32`, `s64`/`u64`, `f32`/`f64`,
`char`, nullary enums, resource handles, byte lists, the unit-success `result`,
direct records (including nested records with byte-list fields), and flags
words. Indirect parameters, non-byte lists, aggregate results, `u32` and
narrower integers, tuples, and `own`/`borrow` drop rules are specified but not
yet produced. These are coverage gaps in this design, not a change to it.

## References

- [WebAssembly Component Model specification](https://github.com/WebAssembly/component-model/blob/main/design/mvp/Explainer.md#canonical-abi): WIT and the Canonical ABI
  (`lift`/`lower`, flattening, `realloc`, return pointer, post-return).
- WASI 0.2 WIT interfaces (`wasi:cli`, `wasi:io`, `wasi:clocks`, `wasi:random`).
- `wit-parser` `Resolve::wasm_signature`, `AbiVariant`.
- [DEC-06 — Runtime Interface via WASI and the Component Model](../../../decision/DEC-06-runtime-interface-via-wit.md),
  [DEC-05 — Target wasmtime's WebAssembly Feature Set](../../../decision/DEC-05-wasmtime-feature-set.md).
- [MIR](../fp/mir.md), [capability profile](capability-profile.md),
  [linear memory boundary](linear-memory-and-canonical-abi-boundary.md),
  [WASI platform library](wasi-platform-library.md).
