# purescript-rs

`purescript-rs` is a learning compiler that compiles a growing subset of
PureScript to portable WebAssembly and runs it on a WASI 0.2 runtime. It is
built as a sequence of small, testable stages and is not a replacement for the
official PureScript compiler.

## Status

The compiler currently:

- inspects source through every frontend stage: `lex`, `layout`, `parse`,
  `ast`, `hir`, `check`, `check-program`, and `check-program-kinds`;
- lowers the supported subset through Typed Core, closure-converted CC IR, and
  a MIR/CFG to a validated **WASI 0.2 Component Model** artifact, and prints
  the corresponding WAT;
- exports `wasi:cli/run@0.2.12`, so `main`'s result becomes the process exit
  code through `wasi:cli/exit`; `log` and `error` write to
  `wasi:cli/stdout` and `wasi:cli/stderr`, and `now` reads the monotonic clock.

The supported subset includes linked source modules, direct and higher-order
functions with scalar-capturing closures, `Int`/`Boolean`/`Number`/`Char`/
`String`/`Unit`, arithmetic and comparisons, scalar `let`, `if`, `case`,
nullary and field data types, newtypes, closed concrete records and record
updates, concrete scalar arrays, parameterized ADTs with the erased
representation, and the console, clock, and random WASI capabilities.

Still open:

- type classes and dictionary passing, open rows, and generic aggregates;
- the complete scalar and numeric operation set
  ([D-09](docs/design/D-09-scalar-and-numeric-lowering.md));
- the unified target-neutral variant representation and the linear-memory path
  for every aggregate ([DEC-08](docs/decision/DEC-08-target-neutral-variant-representation.md),
  [D-10](docs/design/D-10-linear-memory-representation.md));
- the broader canonical ABI and ownership rules
  ([D-07](docs/design/D-07-wit-imports-and-std.md)).

General PureScript compatibility and the official runtime suite remain future
work.

## Quick start

```sh
# Frontend inspection
cargo run -- lex examples/basic.purs
cargo run -- layout examples/basic.purs
cargo run -- parse examples/basic.purs
cargo run -- ast examples/basic.purs
cargo run -- hir examples/resolved.purs
cargo run -- check examples/basic.purs
cargo run -- check-program-kinds examples/basic.purs
cargo run -- dump mir examples/basic.purs

# Build, print, and run
cargo run -- build examples/basic.purs -o /tmp/basic.wasm
wasmtime run /tmp/basic.wasm; echo $?   # prints 42
cargo run -- wat examples/basic.purs -o /tmp/basic.wat

cargo run -- build examples/hello.purs -o /tmp/hello.wasm
wasmtime run /tmp/hello.wasm            # prints "hello world"
```

`build <file.purs>...` resolves and links every listed module with the embedded
`Prelude` and writes the artifact. `wat <file.purs>...` renders the text form.
`dump <core|cc|mir> <file.purs>` prints an intermediate IR for debugging.

## Workspace

| Crate | Responsibility |
| --- | --- |
| `psrs-span` | Source text, byte ranges, and line/column mapping. |
| `psrs-syntax` | Lexing, layout insertion, and parsing. |
| `psrs-cst` | Concrete syntax tree with source ranges. |
| `psrs-ast` | Normalized AST and CST-to-AST lowering. |
| `psrs-resolve` | Locals, same-module names, and whole-program module graphs into HIR. |
| `psrs-hir` | Resolved HIR nodes and stable declaration/local IDs. |
| `psrs-kind` | Kind inference and unification with official kind diagnostics. |
| `psrs-thir` | Typed expressions. |
| `psrs-typecheck` | Rank-1 polymorphic type inference. |
| `psrs-desugar` | Operator desugaring while preserving HIR. |
| `psrs-core` | Typed Core and its HIR lowering. |
| `psrs-backend` | CC IR, MIR/CFG, structured Wasm encoding, validation, and WAT. |
| `psrs-driver` | Wires the compiler passes together. |
| `psrs-cli` | Source inspection, `build`, `wat`, and `dump` commands. |

The architecture defines twelve major passes across six long-lived IR families;
see [D-01](docs/design/D-01-frontend-and-ir-boundaries.md).

## Target and capability profile

The artifact contract is an explicit capability profile for a pinned `wasmtime`
release, not every feature a runtime happens to support. The stable profile
enables Wasm GC, reference types, typed function references, and the synchronous
WASI 0.2 Component Model path; SIMD, tail calls, exceptions, threads, memory64,
and WASI 0.3 stay disabled until their lowerings and tests land. The backend also
provides a core-MVP linear-memory planner that consumes the same CC IR. See
[DEC-05](docs/decision/DEC-05-wasmtime-feature-set.md) and
[D-05](docs/design/D-05-backend-capability.md).

## Project documents

- [Feature catalog](docs/feature/): user-facing behavior and acceptance
  criteria.
- [Design documents](docs/design/): implementation and IR architecture.
- [Decision records](docs/decision/): only major, durable choices.
- [Repository instructions](AGENTS.md): documentation, code, and validation
  rules for contributors and coding agents.

The user-facing goals are [F-01](docs/feature/F-01-source-inspection.md) and
[F-02](docs/feature/F-02-portable-programs.md). Their implementations are
specified by [D-01](docs/design/D-01-frontend-and-ir-boundaries.md) and
[D-02](docs/design/D-02-wasm-lowering.md). The backend is split across
[D-05](docs/design/D-05-backend-capability.md) (capability profile),
[D-06](docs/design/D-06-low-level-ir-and-wasm-types.md) (IR boundaries and
verification), [D-07](docs/design/D-07-wit-imports-and-std.md) (WIT imports and
canonical ABI), [D-08](docs/design/D-08-generic-wasm-representation.md)
(generic values), [D-09](docs/design/D-09-scalar-and-numeric-lowering.md)
(scalar and numeric lowering), and
[D-10](docs/design/D-10-linear-memory-representation.md) (linear-memory
representation), with the concrete GC layouts and execution-evidence matrix in
[D-11](docs/design/D-11-gc-representation-and-evidence.md). The type system is
in [D-03](docs/design/D-03-type-system.md) and the official-suite roadmap in
[D-04](docs/design/D-04-suite-roadmap.md).

For a guided, interactive overview of P0 through P11, use the React application
in [`psrs-explorer/`](psrs-explorer/). It labels compact teaching forms as curated
and links to CLI commands for real compiler output.

Decision records:

- [DEC-01](docs/decision/DEC-01-distinct-ir-boundaries.md) — distinct IR
  boundaries
- [DEC-02](docs/decision/DEC-02-thin-structured-wasm-encoding.md) — thin
  structured Wasm encoding
- [DEC-03](docs/decision/DEC-03-purescript-faithful-type-system.md) —
  PureScript-faithful type system and effect encoding
- [DEC-04](docs/decision/DEC-04-official-test-suite-roadmap.md) — frontend and
  backend feature matrices
- [DEC-05](docs/decision/DEC-05-wasmtime-feature-set.md) — Wasmtime feature set
- [DEC-06](docs/decision/DEC-06-runtime-interface-via-wit.md) — runtime
  interface via WIT
- [DEC-07](docs/decision/DEC-07-runtime-representation-for-parameterized-adts.md) —
  parameterized-ADT representation
- [DEC-08](docs/decision/DEC-08-target-neutral-variant-representation.md) —
  target-neutral variant representation

## Development checks

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The optional upstream differential test checks the front end against the
official `purs` compiler on a small manifest of cases. It skips when `purs` or a
PureScript checkout is unavailable:

```sh
PURESCRIPT_REPO=/path/to/purescript cargo test -p psrs-driver --test upstream
```
