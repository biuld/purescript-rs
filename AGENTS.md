# Repository Guidance

These instructions apply to the whole repository. Keep changes aligned with the
feature and design documents under `docs/`.

## Documentation

- Write all documentation in English.
- Keep project documentation under `docs/`, except this file and the root
  `README.md`.
- Put user-facing, implementation-independent behavior in
  `docs/feature/F-XX-<slug>.md`. Use stable, zero-padded IDs such as `F-01`.
- Put implementation details in `docs/design/D-XX-<slug>.md`. Every design
  document must identify the feature it implements, for example `D-01` for
  `F-01`.
- Use `docs/decision/` only for major, durable decisions. Do not create a
  decision record for routine implementation choices. Give decision records
  stable IDs such as `DEC-01` and include context, the chosen option, and its
  consequences.
- Keep feature documents free of crate names, libraries, and internal IR
  details. Put those in design documents.

## Code

- Write code comments, doc comments, names, and diagnostics in English.
- A source file must not exceed 500 lines. Before it grows past that limit,
  split it into a module directory with `mod.rs` and focused submodules.
- Keep compiler representations separate. A pass must consume one defined
  representation and produce another through an explicit conversion. Do not
  add type information, resolved IDs, or backend fields to CST nodes.
- Keep crate dependencies directed toward lower-level representations and
  shared source utilities. Avoid dependency cycles and placeholder crates; add
  a crate when it has a real owner and API.
- Preserve source ranges through every representation and lowering pass where
  diagnostics or debugging need them.

## Compiler Representation Boundaries

- **CST** represents parsed source syntax. It keeps the concrete forms and
  token/span information needed for diagnostics, source-oriented tools, and
  lowering. It does not perform name resolution, type checking, or codegen.
- **AST** is a separate, normalized surface-language representation. It may
  discard punctuation and syntax-only distinctions, but it keeps source spans
  and unresolved names. It is not an alias for CST.
- **Resolved AST/HIR** replaces names with stable declaration or local IDs.
- **Typed AST/Core** records checked types and elaborated semantic evidence.
  Core removes surface syntax and has a deliberately smaller expression set.
- **Lowered IRs** each state their own invariants: ANF makes evaluation order
  explicit; closure IR makes captures explicit; representation IR (MIR) fixes
  runtime layouts and is the lowest IR. Wasm is a target encoding emitted from
  MIR, not a separate IR.

See `docs/design/D-01-frontend-and-ir-boundaries.md` and
`docs/design/D-02-wasm-lowering.md` before changing these boundaries.

## Validation

Run formatting and the full workspace test suite for Rust changes:

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The workspace default member is the CLI so `cargo run -- ...` works from the
repository root. Always use `cargo test --workspace` to include library tests.
