# D-07 — WIT Imports and the Standard Library

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress (scalar, handle, and byte-list bootstrap implemented)

## Purpose

WASI is the runtime ABI ([DEC-06](../decision/DEC-06-runtime-interface-via-wit.md)):
the PureScript-facing standard library is built on WASI and the compiler lowers
to WASI's canonical ABI. This document defines how a source declaration binds to
a WIT import and how the backend lowers such a call generically, so the compiler
does not implement host functions one by one. It complements
[D-06](D-06-low-level-ir-and-wasm-types.md).

## Problem

The bootstrap implements `log`, `error`, and `now` as compiler code: the host
registry declares them (name, symbol, type), but the backend matches on the host
name and emits a hand-written WASI sequence. Adding a host function means
editing the backend, and the standard library is not library code.

## Design

### Declaring an import

A value provided by WIT is declared in source with a **binding string**:

```purescript
foreign import "wasi:clocks/monotonic-clock#now" now :: Int
```

The string is `<interface>#<function>` and names the WIT interface and
function. The declaration's source name is unrelated to the WIT name, and its
type is the declared type: the backend maps it to the canonical signature
through the standard type mapping. There is a single external kind, `Wit`; the
compiler has no per-function host registry.

### Generic WIT-import lowering

For a `Wit` binding the backend resolves the canonical signature from the
vendored WASI WIT (`WasiRegistry`) and lowers the call by **type-directed
mapping** rather than a per-function recipe:

- Each declared argument is classified against the WIT-level parameter it
  matches. A scalar parameter and a resource handle each flatten to one `i32`
  (or `f64` for `Number`-compatible WIT scalars);
  a 64-bit scalar is widened from `Int` with the extension its WIT signedness
  requires (`i64.extend_i32_s` for `s64`, `i64.extend_i32_u` for `u64`); a
  `String` argument (a `list<u8>`) flattens to the data pointer and length of
  its length-prefixed buffer.
- If the canonical import takes a return pointer, the backend passes a scratch
  address as the last argument.
- The canonical result is mapped to the declared result: `i32` as-is, `i64`
  narrowed to `Int` with `i32.wrap_i64`, no result to `Unit`, and a returned
  `list`/`string` read back from the return pointer.

The output-stream write adapter is the one supported `result`-shaped import.
Its source declaration returns `Unit`, but the one-byte canonical result
discriminant is read with `i32.load8_u` from the return area after the call. A
nonzero discriminant traps instead
of being silently converted to `Unit`; this keeps the current source API
small without losing a host-side write failure. Other result-shaped imports
remain rejected until a source-level error representation is available.

### Returned lists and the allocator

When an import returns a `list`/`string`, the host writes it into guest memory
and passes `(pointer, length)` through the return pointer. Allocating in guest
memory requires an exported `cabi_realloc`, so a module that imports such a
function also exports one. It is a bump allocator whose free pointer lives in a
data segment after the string data and grows memory on demand. Each allocation
is prefixed with its length and the allocated pointer is returned after that
prefix, so a returned `(pointer, length)` is exactly a length-prefixed string
value: the lowering computes `pointer - 4`.

A returned list whose element type is not a byte is rejected with a source
diagnostic; such imports (for example `get-arguments`, which returns
`list<string>`) require aggregate values before they can be enabled.
Returned byte lists are covered by execution tests that feed the recovered
`String` to the ordinary `writeStdout` WIT import and exercise repeated
allocator calls.

### Canonical ABI coverage

The bootstrap classifies only scalars, resource handles, and byte lists. The
completed slice classifies the canonical ABI forms the WASI 0.2 surface uses and
adapts them to the source value representation. Classification stays in the ABI
layer; CC receives only an abstract signature and MIR receives only the
resulting canonical parameters and adapter instructions.

#### Classification

Each WIT function is classified into a canonical signature plus a per-parameter
and per-result **ABI shape** that records how the declared source value flattens
or is read back:

| WIT form | Flattened core form | Read back |
| --- | --- | --- |
| `bool`, `u8`/`s8`, `u16`/`s16`, `u32`/`s32`, `char` | one `i32` | direct |
| `u64`/`s64` | one `i64` | direct |
| `f32`/`f64` | one `f32`/`f64` | direct |
| `enum`, `flags` | one or more `i32` | direct |
| `record`, `tuple` | flattened concatenation of the fields | return pointer if the flattened result has more than one value |
| `variant`, `option`, `result` | `i32` discriminant plus the flattened join of the case payloads | return pointer when it does not fit one value |
| `list<T>`, `string` | `(pointer, length)` pair | return pointer, read as `(pointer, length)` |
| `own<T>`, `borrow<T>`, resource | one `i32` handle | direct |

When the flattened parameters exceed the canonical ABI limit, the import takes a
single pointer to a memory record instead; when the flattened result exceeds one
value, the import takes a trailing return pointer. Both cases are represented by
the same `retptr` and ABI-shape data the bootstrap already carries.

#### Memory layout

The ABI layer computes the canonical memory layout (field offsets, alignment,
padding, variant discriminant placement, and list/string length prefixes) for
every aggregate form that is passed or returned indirectly. The layout is used
only to emit MIR loads, stores, and pointer arithmetic; it is not stored in CC
or MIR as metadata.

#### Source mapping

A source declaration is validated against the resolved WIT function by mapping
its source type to a WIT form:

- `Int` to `s32`, `Number` to `f64`, `Boolean` to `bool`, `Char` to `char`,
  `String` to `string`, `Unit` to `unit`;
- a closed record to a `record` with matching field names and types;
- a data type with field constructors to a `variant` with matching case tags;
- `Array a` to `list<...>` with a byte element for the current `String`
  boundary, and to other element types once aggregate lists are supported; and
- a tuple to `tuple`.

A source type that does not match its WIT form is rejected with a
source-associated diagnostic before CC lowering, as the bootstrap already does
for scalars.

#### Ownership and post-return

- A returned `list`/`string` is owned by the guest after the call. When the
  source value is consumed and no longer referenced, the adapter releases it
  through the `cabi_realloc`-compatible allocator or a `post-return` action
  once reclamation exists. Until then the allocator is a bump allocator and the
  release is a no-op, which is sound but leaks.
- An `own` handle returned to the guest must be dropped when the source value is
  dropped; a `borrow` handle must not be dropped. The adapter inserts the
  required drop calls for `own` handles once resource types are exposed.
- A guest that passes a `borrow` handle must keep the resource alive across the
  call; the lowering evaluates the argument before the call and does not drop it.

#### Delivery order

1. `enum`, `flags`, and direct scalar records and tuples.
2. Indirect records and tuples through the return pointer.
3. `option`/`result`/`variant` with an `i32` discriminant and payload join.
4. Non-byte `list<T>` and `list<string>` with aggregate memory layout.
5. `own`/`borrow` handles and their drop rules; resource types.

Each step adds classification, adapter lowering, a validation test, and a
Wasmtime execution test against a vendored WIT function.

### The standard library

The standard library is source code that declares its WIT imports and defines
platform services over them. No host function is implemented in the compiler;
the backend only knows the generic WIT-import call. The platform-independent
`Prelude` defines `Effect a`, `pure`, `bind`, and `runEffect`. `WASI.Console`
defines `log :: String -> Effect Unit` and `error :: String -> Effect Unit`,
while `WASI.Clock` defines `now :: Effect Int`.

The library is a real module. It is embedded in the driver because there is no
filesystem module loader yet, but it is resolved, type checked, and linked like
any other module; a program reaches its values with explicit `import` lines. It
declares `getStdout`, `getStderr`, `writeStdout`, and `monotonicNow` with WIT
bindings, and defines the effectful services in ordinary source code:

```purescript
log s = \token -> let ignored = writeStdout getStdout s in writeStdout getStdout "\n"
```

`Effect a` is currently represented by the bootstrap compiler as `Boolean -> a`.
This is a temporary representation for deferred construction and explicit
execution, not a dedicated effect runtime. Constructing an effect only creates
a closure; `runEffect` supplies the execution token, and `bind` invokes the
left action before the continuation. Scheduling, cancellation, and asynchronous
runtime support are separate future work.

There is no `Write` composition left in the compiler: the newline is an ordinary
string literal and becomes a data segment. After linking, declarations the
program does not reach from `main` are pruned, so a program that does not use
`log` or `error` does not import the output streams.

## Consequences

- Adding a compatible WIT import is a `foreign import` declaration, not a
  per-function backend recipe. The compiler rejects source signatures that do
  not match the canonical WIT shape.
- The compiler still owns the string representation (length-prefixed buffer in
  linear memory); a library sees strings, and the generic lowering adapts them.
- Modules are linked at the Core level: type IDs are renumbered into one table
  and unreachable declarations are pruned. A module's values are exported by
  their declared types, so an unannotated declaration is not visible to other
  modules yet.
- Imports that return a `list`/`string` need an allocator, so the module exports
  `cabi_realloc`. Other returned aggregates (`list<string>`,
  `list<tuple<...>>`) additionally need aggregate values in the backend.

## Open items

- A filesystem module loader, so user modules and libraries are discovered and
  selected instead of the driver providing the standard library source.
- The canonical ABI coverage above, so imports like `get-arguments` and
  `get-environment` can be exposed.
- A reclaiming allocator, so returned lists and owned resources can be released
  instead of leaked by the bump allocator.
