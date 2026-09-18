# F-02 — Build Portable Program Artifacts

**Status:** In progress
**Design:** [D-02 — Wasm Lowering](../design/D-02-wasm-lowering.md)

## User need

Users want to build a supported PureScript program once and run it in a
compatible portable environment.

## User-visible behavior

The intended workflow is:

```sh
psrs build src/Main.purs -o main.wasm
<compatible WASI runtime> main.wasm
```

The first platform target is WASI, using the WASI 0.2 Component Model as its
baseline. Programs use the project's PureScript-facing WASI libraries for
platform services. Existing Node.js APIs and JavaScript FFI modules are not
supported compatibility targets. Programs that use unsupported syntax,
types, or platform services receive source-oriented diagnostics rather than a
malformed artifact.

The current compiler can build a restricted, single-module program to a
validated core Wasm module and print its WAT form:

```sh
psrs build src/Main.purs -o main.wasm
psrs wat src/Main.purs -o main.wat
```

The initial slice supports direct top-level functions, integer and boolean
values, integer arithmetic and comparisons, scalar `let`, and `if`. The module
exports a zero-argument integer `main` function. It is not yet a standalone
WASI command or a WASI Component Model artifact, and it cannot yet call WASI
services.

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

- Language and platform support grows in documented increments rather than
  promising compatibility with every existing PureScript program.
- Standard platform services are provided through PureScript-facing WASI
  libraries and a stable runtime interface.
- Optimizations preserve observable program behavior.
- Node.js and JavaScript FFI compatibility, sockets, HTTP, and asynchronous
  WASI services are later or separate work.
