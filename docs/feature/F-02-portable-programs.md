# F-02 — Build Portable Program Artifacts

**Status:** In progress
**Design:** [D-02 — Wasm Lowering](../design/D-02-wasm-lowering.md)

## User need

Users want to build a supported PureScript program once and run it in a
compatible portable environment.

## User-visible behavior

The intended workflow is:

```sh
psrs build src/Helper.purs src/Main.purs -o main.wasm
<compatible WASI runtime> main.wasm
```

The first platform target is WASI 0.2 on the Component Model, whose synchronous
interfaces match the runtime; WASI 0.3's async model is later work. Programs
use the project's PureScript-facing WASI libraries for platform services.
Existing Node.js APIs and JavaScript FFI modules are not supported compatibility
targets. Programs that use unsupported syntax, types, or platform services
receive source-oriented diagnostics rather than a malformed artifact.

The compiler emits a WASI 0.2 Component Model artifact. The legacy WASI
Preview 1 module ABI is not part of the supported output contract.

The executable baseline is a documented, pinned WebAssembly runtime
(`wasmtime`). An artifact may require the WebAssembly features that baseline
enables, from the standardized WebAssembly 3.0 set (garbage collection,
function references, tail calls, and exception handling) to proposals still in
the preview stage, so portability is defined against engines that implement the
same feature set rather than against the minimal core specification.

Compatibility is measured against the official PureScript test suite, layer by
layer, as decided in [DEC-04](../decision/DEC-04-official-test-suite-roadmap.md):
layout, parse, name resolution, kinds, types, classes, then runtime. Coverage
is per-file agreement with the official compiler on accept/reject and
diagnostic code, and the suite defines the minimum target rather than a
separate feature list. Suite files that require JavaScript or Node.js FFI are
excluded from the target and count as neither coverage nor gaps.

The current compiler can build a restricted program, including linked source
modules, to a validated WASI component and print its WAT form:

```sh
psrs build src/Helper.purs src/Main.purs -o main.wasm
psrs wat src/Helper.purs src/Main.purs -o main.wat
```

The initial slice supports direct top-level functions, integer and boolean
values, integer arithmetic and comparisons, scalar `let`, `if`, nullary enum
tags, non-parameterized data constructors with scalar or nested aggregate
fields, single-field `newtype` values, constructor patterns in `case` and
function parameters, including nested constructor patterns,
a restricted parameterized ADT slice with erased scalar
fields, concrete scalar array literals and indexing, closed concrete records,
field reads, record updates, and closed concrete record patterns with variable
or wildcard field bindings, function values including scalar-capturing closures,
higher-order calls, and the
implemented WASI console, clock, and random capabilities. The selected entry
must be a zero-argument integer `main` function. Fully polymorphic
fully polymorphic declarations, open rows, and unsupported WIT shapes receive source-oriented
diagnostics.

## Acceptance criteria

- A supported source program produces a validated core Wasm module at the
  requested output path.
- The compiler prints WAT or writes it at the requested output path.
- The eventual WASI artifact runs in a compatible WASI runtime and produces
  the program's expected observable result.
- The artifact runs in a compatible WASI runtime and produces the program's
  expected observable result.
- Unsupported constructs fail with a source-oriented diagnostic.
- Each added platform service has documented behavior and executable tests.

## Initial scope

- Language and platform support grows in documented increments, with the
  official test suite as the compatibility baseline rather than promising
  compatibility with every existing PureScript program at once.
- Standard platform services are provided through PureScript-facing WASI
  libraries and a stable runtime interface.
- Optimizations preserve observable program behavior.
- Node.js and JavaScript FFI compatibility, sockets, HTTP, and asynchronous
  WASI services are later or separate work.
