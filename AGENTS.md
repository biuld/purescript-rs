# Repository Guidance

These instructions apply to the whole repository. Keep changes aligned with the
feature and design documents under `docs/`.

## Workflow

Work in the current checkout by default. Inspect the relevant code and
documents, make the requested change, and report the result. Preserve existing
uncommitted work.

- Create a branch or isolated worktree when the user requests one, or when
  isolation is needed to protect concurrent or unrelated changes. Do not make
  either a prerequisite for routine work.
- Use GitHub issues and pull requests when the user requests GitHub
  collaboration or the task is explicitly tied to an existing issue or PR.
  Do not list issues, create issues, push branches, or open PRs by default.
- For a new user-facing feature, maintain the relevant feature and design
  documents under `docs/`. Add a decision record only for a major, durable
  decision. Keep documentation proportional to the change.
- Review the local diff and run the validation relevant to the files changed.

### Backend topic implementation

- When a backend topic has an execution checklist under
  `docs/implementation/backend/`, use it together with the normative design.
  Audit existing code, maintain requirement-to-test evidence, and continue
  through all required vertical slices before declaring the topic complete.
- Passing existing tests or completing one slice does not establish topic
  completion. Required runtime evidence must actually execute; skipped tests
  leave the relevant requirement unverified. Record blockers and precise
  remaining work for continuation, and keep D-04 consistent with verified
  coverage. Do not narrow the design to close implementation gaps.

## Documentation

- Write all documentation in English.
- Keep project documentation under `docs/`, except this file and the root
  `README.md`.
- Put user-facing, implementation-independent behavior in
  `docs/feature/F-XX-<slug>.md`. Use stable, zero-padded IDs such as `F-01`.
- Put implementation details in `docs/design/`. Root-level overview, tooling,
  roadmap, and cross-cutting documents use `D-XX-<slug>.md` with stable
  zero-padded IDs such as `D-01`. Backend designs live under `docs/design/backend/`: the cross-cutting
  contract at `backend/00-<slug>.md`, functional topics under `backend/fp/<slug>.md`,
  optimization topics under `backend/opt/<slug>.md`, and Wasm/WASI topics under
  `backend/wasm/<slug>.md`. Frontend designs follow the same pattern under
  `docs/design/frontend/`: `00-<slug>.md` for the cross-cutting contract,
  and focused topics under `syntax/`, `semantics/`, and `type-system/` with
  unnumbered slug filenames. Each topic file is self-contained. Every design
  document must identify the feature it
  implements, for example `D-01` for `F-01`.
- Use `docs/decision/` only for major, durable decisions. Do not create a
  decision record for routine implementation choices. Give decision records
  stable IDs such as `DEC-01` and include context, the chosen option, and its
  consequences.
- Keep feature documents free of crate names, libraries, and internal IR
  details. Put those in design documents.
- Draw diagrams with Mermaid fenced blocks (````mermaid`): architecture,
  pipelines, control flow, state machines, and sequence diagrams. A short,
  direct diagram — a simple linear order or a tiny dependency chain — may stay
  in an ordinary fenced block. Keep formal model, grammar, and IR fragments and
  pseudocode as ordinary fenced code blocks either way.

### Frontend and backend topic design document template

Topic documents under `docs/design/frontend/` and `docs/design/backend/`
follow a fixed chapter order so
each one both specifies an implementation and teaches its topic. Short documents
may merge sections, but keep the order and the names.

Front matter:

- `# Title`
- `**Feature:** F-XX`
- `**Status:**` the design's maturity (`Draft` or `Stable`), not an
  implementation phase.
- `**Prerequisites:**` the background a reader needs (functional programming,
  WebAssembly, compilers) and the documents to read first.
- `**Summary:**` two to four sentences on what the topic decides.

Sections, in order:

1. **Scope** — what the document owns, what it does not, and where those live.
2. **Background** — the concepts and theory a reader needs, with references.
3. **Model** — precise definitions: types, grammars, IR shapes, notation, and
   invariants.
4. **Design** — the chosen representation or lowering, including the rejected
   alternatives and why.
5. **Algorithms** — step-by-step procedures, pseudocode, and edge cases.
6. **Code map** — the intended code organization for this topic: the module
   directory structure, each module's responsibility, and the key types and
   entry-point function signatures the implementation must provide. This is a
   design target that guides the code; the code is expected to conform to it,
   not the reverse. Do not describe the current file inventory here.
7. **Invariants and verification** — what must hold and what the verifier
   checks.
8. **Worked example** — a small program or IR fragment traced through the stage.
9. **Boundaries and interfaces** — the contracts with adjacent stages.
10. **Open questions and future work**.
11. **References**.

Describe the complete design, not a bootstrap. Do not frame sections around
"MVP", "bootstrap", or a first implementation slice. Implementation coverage
belongs in `docs/design/backend/wasm/capability-profile.md` and
`docs/decision/DEC-04-official-test-suite-roadmap.md`; a document may end with
short implementation notes that only record deviations from the design.

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
`docs/design/backend/wasm/encoding-and-structuring.md` before changing these boundaries.

## Reference Implementations

When semantics or runtime representation are unclear, consult the official
PureScript implementation at `/Users/biu/Projects/purescript` and the relevant
WebAssembly specifications; adapt the result to this repository's Wasm target.

## Validation

Run formatting and the full workspace test suite for Rust changes:

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The workspace default member is the CLI so `cargo run -- ...` works from the
repository root. Always use `cargo test --workspace` to include library tests.

An optional differential test compares the front end with the official `purs`
compiler. It skips when `purs` or a checkout is unavailable, so it never blocks
`cargo test --workspace`. Run it explicitly with `PURESCRIPT_REPO`:

```sh
PURESCRIPT_REPO=/path/to/purescript cargo test -p psrs-driver --test upstream
```
