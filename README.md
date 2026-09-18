# purescript-rs

`purescript-rs` is a learning compiler project that aims to compile PureScript
programs to portable WebAssembly artifacts and run them through a WASI runtime.
It is being built as a sequence of small, testable compiler stages; it is not a
replacement for the official PureScript compiler.

## Current status

The compiler now builds a small, single-module PureScript subset through the
backend IRs and emits validated core Wasm plus WAT. Try the inspection and build
commands with the included examples:

```sh
cargo run -- lex examples/basic.purs
cargo run -- layout examples/basic.purs
cargo run -- parse examples/basic.purs
cargo run -- ast examples/basic.purs
cargo run -- hir examples/resolved.purs
cargo run -- build examples/basic.purs -o /tmp/basic.wasm
cargo run -- wat examples/basic.purs -o /tmp/basic.wat
cargo run -- dump mir examples/basic.purs
```

The parser handles a module with simple value declarations, names,
integer/string/character literals, application, infix operators, lambdas,
`if`, and `let`. `parse` displays the concrete syntax tree; `ast` displays the
normalized AST; `hir` displays resolved local and same-module value names.
The first executable slice supports monomorphic `Int`, `Boolean`, direct
top-level calls, integer operators, scalar `let`, and value-producing `if`.
`build` writes a validated core Wasm module exporting zero-argument `main`;
`wat` renders the corresponding text form. General PureScript compatibility,
polymorphic inference, imports, closures, aggregate values, and the WASI
runtime/component layer are not implemented yet.

## Workspace

- `psrs-span` contains source text, byte ranges, and line/column mapping.
- `psrs-cst` owns concrete syntax tree types. Its current tree is a bootstrap
  subset with source ranges for names, binders, and supported punctuation.
- `psrs-ast` owns the normalized AST and the explicit CST-to-AST lowering pass.
- `psrs-hir` owns resolved HIR nodes and stable declaration/local IDs.
- `psrs-resolve` resolves locals and same-module value names into HIR.
- `psrs-syntax` implements lexing, layout insertion, and parsing.
- `psrs-thir` and `psrs-typecheck` own typed expressions and monomorphic type
  inference.
- `psrs-desugar` lowers resolved operator syntax while preserving HIR.
- `psrs-core` owns Typed Core and its HIR lowering pass.
- `psrs-backend` owns direct-call CC IR, MIR/CFG, Wasm binary
  encoding, validation, and WAT printing from the encoded module.
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
[D-02](docs/design/D-02-wasm-lowering.md).

## Development checks

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
