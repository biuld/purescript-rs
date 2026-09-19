# D-07 — WIT Imports and the Standard Library

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** Implemented (bootstrap)

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
  matches. A scalar parameter and a resource handle each flatten to one `i32`;
  a 64-bit scalar is widened from `Int` with the extension its WIT signedness
  requires (`i64.extend_i32_s` for `s64`, `i64.extend_i32_u` for `u64`); a
  `String` argument (a `list<u8>`) flattens to the data pointer and length of
  its length-prefixed buffer.
- If the canonical import takes a return pointer, the backend passes a scratch
  address as the last argument.
- The canonical result is mapped to the declared result: `i32` as-is, `i64`
  narrowed to `Int` with `i32.wrap_i64`, no result to `Unit`, and a returned
  `list`/`string` read back from the return pointer.

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

### The standard library

The standard library is source code that declares its WIT imports and defines
`log`, `error`, `now`, and later `args`, `env`, `clock`, `random`, and
`filesystem` over them. No host function is implemented in the compiler; the
backend only knows the generic WIT-import call.

The library is a real module. It is embedded in the driver because there is no
filesystem module loader yet, but it is resolved, type checked, and linked like
any other module; a program reaches its values with `import Prelude`. It
declares `getStdout`, `getStderr`, `writeStdout`, and `monotonicNow` with WIT
bindings, exposes `now`, and defines:

```purescript
log s = let a = writeStdout getStdout s in writeStdout getStdout "\n"
```

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
- Aggregate values in the backend, so imports like `get-arguments` and
  `get-environment` can be exposed, and a real allocator with reclamation.
