# purescript-rs

`purescript-rs` is a learning compiler project that aims to compile PureScript
programs to portable WebAssembly artifacts and run them through a WASI runtime.
It is being built as a sequence of small, testable compiler stages; it is not a
replacement for the official PureScript compiler.

## Current status

The compiler now builds a small, single-module PureScript subset through the
backend IRs and emits a validated core Wasm module plus WAT. The artifact is a
WASI command: it exports `_start`, imports
`wasi_snapshot_preview1.proc_exit`, and uses `main`'s result as the exit code.
Programs can also print with `log` (a `String -> Unit` runtime function backed
by `wasi_snapshot_preview1.fd_write`). Try the inspection and build commands
with the included examples:

```sh
cargo run -- lex examples/basic.purs
cargo run -- layout examples/basic.purs
cargo run -- parse examples/basic.purs
cargo run -- ast examples/basic.purs
cargo run -- hir examples/resolved.purs
cargo run -- check examples/basic.purs
cargo run -- check-program-kinds examples/basic.purs
cargo run -- build examples/basic.purs -o /tmp/basic.wasm
cargo run -- wat examples/basic.purs -o /tmp/basic.wat
cargo run -- dump mir examples/basic.purs
wasmtime run /tmp/basic.wasm; echo $?   # prints 42 for examples/basic.purs

cargo run -- build examples/hello.purs -o /tmp/hello.wasm
wasmtime run /tmp/hello.wasm            # prints "hello world"
```

The parser handles a module with simple value declarations, `name :: Type`
signatures (function arrows, `forall`, and parentheses), names,
integer/string/character literals, application, infix operators, lambdas,
`if`, and `let`. `parse` displays the concrete syntax tree; `ast` displays the
normalized AST; `hir` displays resolved local and same-module value names.
A module loader resolves a whole program: it assigns stable module IDs, builds
the import graph, reports duplicate, missing, and cyclic modules, and resolves
imported values through unqualified, qualified, and aliased names while
respecting explicit and `hiding` import lists and explicit export lists.
`check-program <file.purs>...` runs that resolution over a set of modules. The
first executable slice supports monomorphic `Int`, `Boolean`, `String`,
`Unit`, direct top-level calls, integer operators, string literals and `log`,
scalar `let`, and value-producing `if`. Type inference adds rank-1
polymorphism: local `let` groups and top-level strongly connected components are
generalized and schemes are instantiated at use sites. The backend rejects
polymorphic declarations until type erasure and dictionary passing exist;
`identity` therefore reports a backend diagnostic. A `psrs-kind` pass infers and
unifies kinds for `data`, `newtype`, `type`, and `class` declarations and
reports the official `KindsDoNotUnify`, `PartiallyAppliedSynonym`,
`CycleInTypeSynonym`, `CycleInKindDeclaration`, `UndefinedTypeVariable`, and
`InfiniteKind` codes. Inference now carries type constructors and type-level
application, and expands type synonyms, so signatures over `Array`, user types,
and synonyms elaborate and unify. Data and newtype constructors are typed as
polymorphic values, so constructor applications type-check, and single-scrutinee
`case` expressions with constructor, variable, and wildcard patterns type-check.
Type-class constraints, pattern lowering to Core, runtime layouts, and rows are
not implemented yet. `build` writes
a validated core Wasm WASI command exporting zero-argument `main` and `_start`;
`wat` renders the corresponding text form. General PureScript compatibility,
type classes, pattern matching, cross-module compilation to Wasm, closures,
aggregate values, and the Component Model layer are not implemented yet.

## Workspace

- `psrs-span` contains source text, byte ranges, and line/column mapping.
- `psrs-cst` owns concrete syntax tree types. Its current tree is a bootstrap
  subset with source ranges for names, binders, and supported punctuation.
- `psrs-ast` owns the normalized AST and the explicit CST-to-AST lowering pass.
- `psrs-hir` owns resolved HIR nodes and stable declaration/local IDs.
- `psrs-resolve` resolves locals, same-module value names, and whole-program
  module graphs (imports, exports, and cross-module values) into HIR.
- `psrs-syntax` implements lexing, layout insertion, and parsing.
- `psrs-thir` and `psrs-typecheck` own typed expressions and rank-1
  polymorphic type inference.
- `psrs-kind` infers and unifies kinds over resolved declarations and reports
  official kind diagnostics.
- `psrs-desugar` lowers resolved operator syntax while preserving HIR.
- `psrs-core` owns Typed Core and its HIR lowering pass.
- `psrs-backend` owns direct-call CC IR, MIR/CFG, the structured Wasm
  encoding, binary emission, validation, and WAT printing from the encoded
  module.
- `psrs-driver` wires the compiler passes together.
- `psrs-cli` provides source inspection, `build`, and `wat` commands.

The compiler architecture defines twelve major passes across six long-lived
IR families. The first Wasm slice is implemented; the WASI runtime and
Component Model linker remain future work. See [D-01](docs/design/D-01-frontend-and-ir-boundaries.md)
and [D-02](docs/design/D-02-wasm-lowering.md).

## Project documents

- [Feature catalog](docs/feature/): user-facing behavior and acceptance
  criteria.
- [Design documents](docs/design/): implementation and IR architecture.
- [Decision records](docs/decision/): only major, durable choices.
- [Repository instructions](AGENTS.md): documentation, code, and validation
  rules for contributors and coding agents.

The main user-facing goals are described by [F-01](docs/feature/F-01-source-inspection.md)
and [F-02](docs/feature/F-02-portable-programs.md). Their implementations are
specified in [D-01](docs/design/D-01-frontend-and-ir-boundaries.md) and
[D-02](docs/design/D-02-wasm-lowering.md). The type system is specified in
[D-03](docs/design/D-03-type-system.md) and the official-suite roadmap in
[D-04](docs/design/D-04-suite-roadmap.md); the corresponding decisions are
[DEC-03](docs/decision/DEC-03-purescript-faithful-type-system.md) and
[DEC-04](docs/decision/DEC-04-official-test-suite-roadmap.md).

## Development checks

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The optional upstream differential test checks the front end against the
official `purs` compiler on a small manifest of cases. It skips when `purs` or
a PureScript checkout is unavailable:

```sh
PURESCRIPT_REPO=/path/to/purescript cargo test -p psrs-driver --test upstream
```
