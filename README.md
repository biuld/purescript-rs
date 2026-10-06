# purescript-rs

`purescript-rs` is a learning compiler that compiles a growing subset of
PureScript to portable WebAssembly and runs it on a WASI 0.2 runtime. It is
built as a sequence of small, testable stages and is not a replacement for the
official PureScript compiler.

## Quick start

```sh
# Build, print, and run
cargo run -- build examples/basic.purs -o /tmp/basic.wasm
wasmtime run /tmp/basic.wasm; echo $?   # prints 42

cargo run -- build examples/hello.purs -o /tmp/hello.wasm
wasmtime run /tmp/hello.wasm            # prints "hello world"

# Frontend inspection, one stage at a time
cargo run -- lex examples/basic.purs
cargo run -- parse examples/basic.purs
cargo run -- ast examples/basic.purs
cargo run -- hir examples/resolved.purs
cargo run -- check examples/basic.purs
cargo run -- check-program-kinds examples/basic.purs
cargo run -- dump mir examples/basic.purs
```

The local default expects `../psrs-stdlib` at the revision/content recorded in
`stdlib.lock.json`. Set `PSRS_STDLIB_ROOT` for an explicit development package.
See [package and conformance setup](docs/workflow/stdlib-conformance.md).

`build <file.purs>...` resolves and links every listed module with the standard
library from the locked `psrs-stdlib` package and writes the artifact.
`wat <file.purs>...` renders the text form. `dump <core|cc|mir> <file.purs>` prints an intermediate IR for
debugging. A selected `main :: Int` returns its value as the process exit code.
A selected `main :: Effect Unit` runs that action once and returns 0; a trap
still propagates.

For a guided, interactive walkthrough of P0 through P11, use the React
application in [`psrs-explorer/`](psrs-explorer/). It labels compact teaching
forms as curated and links to CLI commands for real compiler output.

## Status

Progress is stated as official-suite measurements, not as a summary impression.
The corpus is vendored under [`tests/upstream/`](tests/upstream/), and
[D-04](docs/design/D-04-suite-roadmap.md) carries the full breakdown, including
what remains in each layer.

| Gate | Measured | Scope |
| --- | --- | --- |
| L0/L1 lexing, layout, parsing | 904/908 | non-FFI `layout`, `passing`, `failing`, `warning` files; the four differences are recorded DEC-16 intentional differences |
| L2 resolution | 71/72 failing, 386/413 passing | official `errorCode`s; 23 passing files stop at P3 and 4 at P0 |
| L3 kinds | 39/48 failing | official kind `errorCode`s |
| L4 types | 39/50 failing | official `errorCode`s |
| L5 classes | 58/81 failing | official `errorCode`s |
| L6/M7 runtime | 210/413 passing | all 210 exit 0; 203 do not agree, including 47 with no selected `main` |
| M8 warnings, optimization | not measured | no scoreboard exists |

Run the scoreboards yourself:

```sh
PSRS_ORACLE=annotations PSRS_REQUIRE_WASMTIME=1 \
  cargo test -p psrs-driver --test suite -- --ignored --nocapture
```

### What works

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
- loads the PureScript standard library from the locked `psrs-stdlib` package,
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

### What does not

- **surface lowering** — guards on equations and `case` alternatives, operator
  and constructor-operator aliases, operator sections, unary minus, type
  wildcards, pattern bindings, and multi-scrutinee `case`; 106 `passing` files
  stop here;
- **resolution** — instance declarations are not resolved yet, the `Prim`
  module hierarchy is not provided, and unary minus is not desugared;
- **diagnostics** — the type checker now emits official `errorCode`s for a type
  mismatch, an occurs check, an out-of-range literal, a missing or overlapping
  instance, a missing class member, an escaping skolem, and an ambiguous
  constraint, so 49 M4/M5 cases are measurable; the eight class checks D-04
  lists as `0/n` do not exist yet;
- **runtime** — open rows have no runtime layout, variants are not implemented,
  and the scalar and numeric operation set is partial
  ([scalars](docs/design/backend/fp/scalars-and-primitives.md));
- **aggregates at the ABI boundary** — the canonical ABI covers the mapped
  scalar, string, list, flags, handle, and variant shapes
  ([canonical ABI](docs/design/backend/wasm/canonical-abi-and-wit.md)), and the
  synthesized aggregate fixtures validate but do not yet execute;
- **the standard library** — the independent `psrs-stdlib` package holds the
  pinned official core libraries (215 source modules). A module the compiler owns,
  such as `Safe.Coerce`,
  resolves through its primitive interface rather than the vendored file, which
  stays faithful to upstream; `Unsafe.Coerce.unsafeCoerce` has no interface yet
  and is a recorded gap.

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
| `psrs-driver` | Wires the compiler passes together and loads the locked `psrs-stdlib` package. |
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

## Development checks

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The scoreboards under `crates/psrs-driver/tests/suite/` are `#[ignore]`d and
read the vendored corpus. They need `purs` for the layout and parse boards and
`wasmtime` for the runtime board; each skips cleanly without its tool:

```sh
PSRS_ORACLE=annotations \
  cargo test -p psrs-driver --test suite -- --ignored --nocapture
```

`PSRS_REQUIRE_WASMTIME=1` turns the runtime gate from a skip into a failure, so
CI cannot let an execution test silently pass without a runtime.

The optional upstream differential test checks the front end against the
official `purs` compiler on a small manifest of cases. It skips when `purs` or a
PureScript checkout is unavailable:

```sh
PURESCRIPT_REPO=/path/to/purescript cargo test -p psrs-driver --test upstream
```

## Documentation

[`docs/`](docs/README.md) indexes the project documents, and the
[authoring guide](docs/authoring-guide.md) says where a new one goes and how it
is named.

| Location | Content |
| --- | --- |
| [`docs/feature/`](docs/feature/) | User-facing behavior and acceptance criteria: [F-01](docs/feature/F-01-source-inspection.md), [F-02](docs/feature/F-02-portable-programs.md), [F-03](docs/feature/F-03-interactive-ir-explorer.md). |
| [`docs/design/`](docs/design/) | Implementation and IR architecture. Start at [D-01](docs/design/D-01-frontend-and-ir-boundaries.md) for the pass pipeline, [D-04](docs/design/D-04-suite-roadmap.md) for the official-suite roadmap, then the [frontend](docs/design/frontend/README.md) and [backend](docs/design/backend/README.md) topics. |
| [`docs/decision/`](docs/decision/) | Only major, durable choices, from the [policy](docs/decision/README.md) onward. |
| [`docs/implementation/`](docs/implementation/) | Per-topic acceptance checklists and the evidence behind each. |
| [`tests/upstream/`](tests/upstream/) | The pinned official suite. |
| [`AGENTS.md`](AGENTS.md) | Workflow, code, validation, and project-iteration rules. |

## Project iteration

[D-04](docs/design/D-04-suite-roadmap.md) is the normative record of what is
true; GitHub records what to do next. The work is tracked on the
[PureScript→Wasm roadmap](https://github.com/users/biuld/projects/1) project
board, where a milestone per phase carries the order, `gate:` and `area:` labels
carry the cross-cutting view, dependencies carry the blocking edges, and the
`Corpus cases` field records how much of the official suite an issue recovers.
[AGENTS.md](AGENTS.md#project-iteration) describes how to pick, work, and close
an item, and
[DEC-04](docs/decision/DEC-04-official-test-suite-roadmap.md) records why the
matrices are maintained.
