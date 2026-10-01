# purescript-rs

`purescript-rs` is a learning compiler that compiles a growing subset of
PureScript to portable WebAssembly and runs it on a WASI 0.2 runtime. It is
built as a sequence of small, testable stages and is not a replacement for the
official PureScript compiler.

## Status

Progress is stated as official-suite measurements, not as a summary impression.
The corpus is vendored under [`tests/upstream/`](tests/upstream/) and the full
breakdown, including what remains in each layer, is
[D-04](docs/design/D-04-suite-roadmap.md).

| Gate | Measured | Scope |
| --- | --- | --- |
| L0/L1 lexing, layout, parsing | 904/908 | non-FFI `layout`, `passing`, `failing`, `warning` files; the four differences are recorded DEC-16 intentional differences |
| L2 resolution | 54/70 failing, 36/413 passing | official `errorCode`s |
| L3 kinds | 27/48 failing | official kind `errorCode`s |
| L4 types | not measured | no scoreboard exists |
| L5 classes | not measured | no scoreboard exists |
| L6/M7 runtime | 0/413 passing | no non-FFI corpus program compiles, validates, and runs yet |
| M8 warnings, optimization | not measured | no scoreboard exists |

```sh
PSRS_ORACLE=annotations \
  cargo test -p psrs-driver --test suite -- --ignored --nocapture
```

The scoreboard needs `purs` for the layout and parse boards, and `wasmtime` for
the runtime board. Each board skips cleanly without its tool and fails under
`PSRS_REQUIRE_WASMTIME=1`, so they are opt-in and never block
`cargo test --workspace`.

The compiler currently:

- inspects source through every frontend stage: `lex`, `layout`, `parse`,
  `ast`, `hir`, `check`, `check-program`, and `check-program-kinds`;
- lowers the supported subset through Typed Core, closure-converted CC IR, and
  a MIR/CFG to a validated **WASI 0.2 Component Model** artifact, and prints
  the corresponding WAT;
- exports `wasi:cli/run@0.2.12`, so `main`'s result becomes the process exit
  code through `wasi:cli/exit`; `log` and `error` write to
  `wasi:cli/stdout` and `wasi:cli/stderr`, and `now` reads the monotonic clock;
- resolves, links, and type checks **type classes, superclasses, instances, and
  functional dependencies**, then passes dictionaries through to Wasm — a
  method reached through a superclass constraint returns the right value under
  `wasmtime`, as does a call through a constrained function argument;
- loads the PureScript standard library from [`stdlib/lib`](stdlib/lib) on disk,
  exposing console, clock, random, process arguments and environment, filesystem,
  and sockets over WASI.

Verified working subsets, each with source tests and Wasmtime execution:

- functions with scalar, aggregate, and generic captures; direct and indirect
  calls; explicit adapters between concrete and erased higher-order values;
- `Int`/`Number`/`Boolean`/`Char`/`String`/`Unit`, arithmetic and comparisons,
  `let`, `if`, `case`, and `do`/`ado` desugaring;
- data types and newtypes, including parameterized ADTs with the erased
  representation, multi-field constructor patterns in parameters, and coverage
  analysis that reports non-exhaustive witnesses;
- closed concrete records, canonical generic records, and arrays, with field
  reads, updates, and patterns;
- rank-2 through rank-4 polymorphism, roles and `Coercible`, and `derive
  newtype`.

Not yet supported, with the layer that owns each:

- **surface lowering** — guards on equations and `case` alternatives, the
  ascription `e :: T`, operator and constructor-operator aliases, operator
  sections, type wildcards, pattern bindings, and multi-scrutinee `case`;
- **resolution** — instance declarations are not resolved yet, the `Prim`
  module hierarchy is not provided, and unary minus is not desugared;
- **diagnostics** — the type checker emits official `errorCode`s only for
  `EscapedSkolem`, so most type and class cases cannot be scored;
- **runtime** — open rows have no runtime layout, variants are not implemented,
  and the scalar and numeric operation set is partial
  ([scalars](docs/design/backend/fp/scalars-and-primitives.md));
- **aggregates at the ABI boundary** — the canonical ABI covers the mapped
  scalar, string, list, flags, handle, and variant shapes
  ([canonical ABI](docs/design/backend/wasm/canonical-abi-and-wit.md)), and the
  synthesized aggregate fixtures validate but do not yet execute.

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

`build <file.purs>...` resolves and links every listed module with the standard
library from `stdlib/lib` and writes the artifact. `wat <file.purs>...` renders
the text form. `dump <core|cc|mir> <file.purs>` prints an intermediate IR for
debugging.

`main` must be a zero-argument `Int` declaration; its value becomes the process
exit code.

## Workspace

| Crate | Responsibility |
| --- | --- |
| `psrs-span` | Source text, byte ranges, and line/column mapping. |
| `psrs-syntax` | Lexing, layout insertion, and parsing. |
| `psrs-cst` | Concrete syntax tree with source ranges. |
| `psrs-ast` | Normalized AST and CST-to-AST lowering. |
| `psrs-resolve` | Locals, same-module names, cross-module imports/exports, and whole-program module graphs into HIR. |
| `psrs-hir` | Resolved HIR nodes and stable declaration/local IDs. |
| `psrs-kind` | Kind inference and unification with official kind diagnostics. |
| `psrs-thir` | Typed expressions. |
| `psrs-typecheck` | Rank-1 and higher-rank inference, classes, instances, fundeps, roles, and coercions. |
| `psrs-desugar` | Operator desugaring while preserving HIR. |
| `psrs-core` | Typed Core and its HIR lowering. |
| `psrs-backend` | CC IR, MIR/CFG, structured Wasm encoding, WIT/ABI lowering, validation, and WAT. |
| `psrs-driver` | Wires the compiler passes together and loads `stdlib/lib`. |
| `psrs-cli` | Source inspection, `build`, `wat`, and `dump` commands. |

The architecture defines twelve major passes across six long-lived IR families;
see [D-01](docs/design/D-01-frontend-and-ir-boundaries.md).

## Target and capability profile

The artifact contract is an explicit capability profile for a pinned `wasmtime`
release, not every feature a runtime happens to support. The stable profile
enables Wasm GC, reference types, typed function references, and the synchronous
WASI 0.2 Component Model path; SIMD, tail calls, exceptions, threads, memory64,
and WASI 0.3 stay disabled until their lowerings and tests land. Wasm GC is the
only language heap; linear memory is reserved for the canonical ABI boundary
([DEC-09](docs/decision/DEC-09-gc-only-language-heap.md)). See
[DEC-05](docs/decision/DEC-05-wasmtime-feature-set.md) and
[capability profile](docs/design/backend/wasm/capability-profile.md).

## Project documents

- [Feature catalog](docs/feature/): user-facing behavior and acceptance
  criteria.
- [Design documents](docs/design/): implementation and IR architecture.
- [Decision records](docs/decision/): only major, durable choices.
- [Implementation records](docs/implementation/): per-topic acceptance
  checklists and the evidence behind each.
- [Vendored test corpus](tests/upstream/): the pinned official suite.
- [Repository instructions](AGENTS.md): documentation, code, validation, and
  project-iteration rules for contributors and coding agents.

The user-facing goals are [F-01](docs/feature/F-01-source-inspection.md) and
[F-02](docs/feature/F-02-portable-programs.md). Their implementations are
specified by [D-01](docs/design/D-01-frontend-and-ir-boundaries.md) and
[wasm encoding](docs/design/backend/wasm/encoding-and-structuring.md). The backend is split across
[capability profile](docs/design/backend/wasm/capability-profile.md) (capability profile),
[IR boundaries](docs/design/backend/00-ir-boundaries.md) (IR boundaries and
verification), [canonical ABI](docs/design/backend/wasm/canonical-abi-and-wit.md) (WIT imports and
canonical ABI), [erasure](docs/design/backend/fp/polymorphism-and-erasure.md)
(generic values), [scalars](docs/design/backend/fp/scalars-and-primitives.md)
(scalar and numeric lowering), and
[linear ABI boundary](docs/design/backend/wasm/linear-memory-and-canonical-abi-boundary.md) (canonical ABI
boundary), with the concrete GC layouts and execution-evidence matrix in
[data representation](docs/design/backend/fp/data-representation.md). The
[frontend design](docs/design/frontend/README.md) includes the
[PureScript type system](docs/design/frontend/type-system/README.md);
[D-04](docs/design/D-04-suite-roadmap.md) tracks feature coverage and official-suite progress
under [DEC-04](docs/decision/DEC-04-official-test-suite-roadmap.md).

For a guided, interactive overview of P0 through P11, use the React application
in [`psrs-explorer/`](psrs-explorer/). It labels compact teaching forms as curated
and links to CLI commands for real compiler output.

## Project iteration

[D-04](docs/design/D-04-suite-roadmap.md) is the normative record of what is
true; GitHub records what to do next. The work is tracked on the
[PureScript→Wasm roadmap](https://github.com/users/biuld/projects/1) project
board, where a milestone per phase carries the order, `gate:` and `area:` labels
carry the cross-cutting view, dependencies carry the blocking edges, and the
`Corpus cases` field records how much of the official suite an issue recovers.
[AGENTS.md](AGENTS.md#project-iteration) describes how to pick, work, and close
an item.

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
- [DEC-09](docs/decision/DEC-09-gc-only-language-heap.md) — GC-only language
  heap
- [DEC-10](docs/decision/DEC-10-canonical-abi-buffer-lifetime.md) — canonical ABI
  buffer lifetime
- [DEC-11](docs/decision/DEC-11-primitive-ffi-stdlib-wrappers.md) — primitive FFI
  and standard-library wrappers
- [DEC-12](docs/decision/DEC-12-resolved-wit-bindings.md) — WIT bindings by
  resolved type identity
- [DEC-13](docs/decision/DEC-13-wit-to-source-type-mapping.md) — WIT to source
  type mapping
- [DEC-14](docs/decision/DEC-14-resource-handle-ownership.md) — resource handle
  ownership
- [DEC-15](docs/decision/DEC-15-unified-type-representation.md) — unified type
  representation

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

The suite scoreboards under `crates/psrs-driver/tests/suite.rs` are `#[ignore]`d
and read the vendored corpus. They need `purs` for the layout and parse boards:

```sh
PSRS_ORACLE=annotations \
  cargo test -p psrs-driver --test suite -- --ignored --nocapture
```

`PSRS_REQUIRE_WASMTIME=1` turns the runtime gate from a skip into a failure, so
CI cannot let an execution test silently pass without a runtime.
