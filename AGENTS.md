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
  When the work *is* roadmap work, see [Project Iteration](#project-iteration).
- For a new user-facing feature, maintain the relevant feature and design
  documents under `docs/`. Add a decision record only for a major, durable
  decision. Keep documentation proportional to the change.
- Review the local diff and run the validation relevant to the files changed.

### Commit granularity

- Group a commit by topic, not by file type. Code, tests, and the design or
  acceptance documentation for the same topic belong in one commit.
- Each commit is a self-contained, reviewable unit: it should make sense on its
  own and pass the validation relevant to the files it changes.
- Do not split one logical change into trivial incremental commits, and do not
  mix unrelated topics in one commit. Avoid a history where each commit changes
  only a little of the same work.
- Rewrite local, unpushed history to merge related commits rather than adding a
  corrective commit on top. Do not rewrite commits that are already pushed.

### Merging a pull request

- Merge with a merge commit: `gh pr merge <number>` with no `--squash` and no
  `--rebase`. Squash and rebase merges collapse the branch's commits into one,
  which destroys the topical history this file requires and leaves a second
  description of the work behind the merge commit's message.
- The branch's commits keep their own messages and authorship on `master`, and
  the merge commit names the pull request. A squash merge also silently resolves
  content conflicts, so a wrong number or line can survive into `master` with
  nothing recording it — that is how a stale measurement reached `README.md`.
- Push follow-up fixes as their own commit on the same branch and merge again
  only after reviewing the diff, rather than amending a pushed commit.
- After merging, prune the local view of the branch: `git fetch --prune`.
  `gh pr merge --delete-branch` removes the remote branch but leaves the local
  remote-tracking ref, so the same work appears under two names until it is
  pruned.

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

## Project Iteration

The roadmap is indexed on GitHub and stated normatively in
`docs/design/D-04-suite-roadmap.md`. The document records what is true; GitHub
records what to do next. Advancing the project means moving both.

### What each mechanism owns

| Mechanism | Owns | Do not use it for |
| --- | --- | --- |
| Milestone `Phase N — …` | Phase order and completion percentage | Due dates — phases are ordered by dependency, not schedule |
| `gate:` label | Which D-04 gate row an issue unblocks; one issue may carry several | Status |
| `area:` label | `frontend`, `backend`, `harness`, or `stdlib` | Priority or size |
| Sub-issue | Nesting a slice under its epic; an issue has exactly one parent | Ordering |
| Dependency | Blocking edges a phase number cannot express | Edges that do not exist |
| Project board | `Status` and `Corpus cases` | Re-creating phase, gate, or area as fields |

The board is <https://github.com/users/biuld/projects/1>. It carries one custom
field, `Corpus cases`, because that number is not recoverable from a label.
Everything else there is a label or a milestone already, and duplicating those
as project fields would create a second source of truth.

### Choosing the next item

1. Read the board's `By phase` view and take the lowest phase with an open item.
   Phase 0 is independent of the rest and may be taken at any time.
2. Skip anything whose `Blocked by` dependencies are not all closed.
3. Among the unblocked items in that phase, prefer the highest `Corpus cases`.
   That is a tiebreak, not an algorithm: a small correctness fix that other work
   depends on outranks a large unblocking one.
4. If that phase has nothing unblocked, move to the next phase rather than
   starting blocked work.

### Working an item

- Set the board `Status` to `In Progress` on starting, and to `Blocked` if you
  stop for a reason outside the issue.
- Read the issue's `References` before designing anything; every slice names the
  design document that governs it.
- New syntax and new diagnostics land with official-suite evidence, not only a
  local test. The issue states whether that is a `purs` differential case or a
  scoreboard number.
- Run the issue's `Validation` block. It is the issue-specific superset of the
  workspace validation below.

### Closing an item

Closing an issue asserts something about the corpus, so re-measure first:

- Re-run the scoreboard the issue touches and put the new number in
  `docs/design/D-04-suite-roadmap.md`: the gate table and the relevant
  `Progress` section. Do not close on a passing local test alone.
- Recount `Corpus cases` if the blockers it names moved, and update the field.
  Counting follows the convention in that document: `passing` blockers by first
  blocking stage, `failing` cases per `errorCode`.
- Land any design-document change in the same commit as the code, not as a
  follow-up.
- Set `Status` to `Done`, then close the issue with a comment naming the
  evidence: which test runs, or which number moved.

### GitHub API notes

- Board operations need the `project` scope on the `gh` token; without it the
  Projects v2 GraphQL fails with `INSUFFICIENT_SCOPES`. `gh auth refresh -s
  project` is interactive, so ask the user instead of hanging on the prompt.
- A project **item** id is not an issue node id. Query the project's `items` and
  use the item id when setting field values.
- Issue dependencies are GraphQL-only:
  `addBlockedBy(input: {issueId, blockingIssueId})`.
- View grouping and sort columns cannot be set through the API, only
  `visibleFields`. Ask the user to set those in the UI rather than reporting a
  view as configured when it is not.

## Documentation

Write all documentation in English. Keep project documentation under `docs/`,
except this file and the root `README.md`. The
[authoring guide](docs/authoring-guide.md) is the reference: where a document
goes, how it is named, the topic design document template, how diagrams are
drawn, and how to state a measured number. Two rules that most often decide a
change:

- Keep documentation proportional to the change, and land a design-document
  change in the same commit as the code it describes.
- A number in a document is a measurement: re-measure before changing it, and
  record a decomposition when one total hides several causes.

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

## Iteration Principles

- Before extending a feature, identify the governing design, the owner of each
  semantic rule, and the invariants its inputs and outputs must satisfy. If the
  existing model cannot express the rule, fix that foundation as part of the
  change. Do not build further behavior on a known incorrect invariant.
- Give each semantic fact one authoritative owner. Consumers must use its
  result or shared operations rather than reconstructing it independently.
  Keep syntax interpretation, semantic checking, and runtime representation
  in their respective stages. A module or crate split must preserve this
  ownership and the contracts between stages.
- Extend shared representations and operations before adding feature-specific
  paths. Preserve the structure, identities, and scope needed for substitution
  and composition. Do not replace a general relation with a private restricted
  model, name-based exception, or side table that later passes must reinterpret.
  Specialized rules must have an explicit semantic justification and use the
  same surrounding invariants.
- Enforce invariants at the common operations that can violate them, across
  every entry point. Changes to related solver state must remain consistent;
  speculative work must use a complete rollback contract, and scoped work
  must have a defined entry and exit contract. Avoid feature-local copies of
  these mechanisms.
- Make stage contracts truthful. Publish a result as checked only after its
  required checks succeed, preserving diagnostics and their origins. Missing
  required metadata and invalid shapes are errors; they must not silently
  become fresh unknowns, permissive defaults, or valid empty results. Document
  deliberate approximations and trusted assumptions in the owning contract.
- Preserve semantic evidence across a boundary until an explicit lowering can
  discharge or erase it. State which guarantees a verifier checks and which
  it trusts from the producer; retain enough immutable information for the
  checks it claims to perform.
- Keep incomplete work explicit. A supported subset must preserve the full
  model's invariants and report unsupported cases honestly. Record remaining
  obligations without narrowing the normative design or describing a partial
  implementation as complete.
- Review integration as well as the local feature: equivalent forms must
  receive equivalent treatment, and new behavior must compose with adjacent
  mechanisms and imported declarations. Choose validation that exercises these
  interactions and rejection paths. Passing isolated cases does not establish
  architectural coherence.

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

The suite scoreboards under `crates/psrs-driver/tests/suite/` are the
acceptance evidence for a milestone or a gate row. They are `#[ignore]`d, read
the vendored corpus, and need `purs` for the layout and parse boards:

```sh
PSRS_ORACLE=annotations \
  cargo test -p psrs-driver --test suite -- --ignored --nocapture
```

`PSRS_ORACLE=annotations` classifies `failing` files by the corpus's own
`@shouldFailWith` annotation instead of by invoking `purs`, which is the corpus's
own ground truth and does not depend on the installed libraries. `layout`,
`passing`, and `warning` are still classified with `purs`.

A new syntax form or a new diagnostic is not done until it agrees with the
official suite on the case that exercises it. A local test proves the compiler is
self-consistent; only the scoreboard proves it matches `purs`.

`PSRS_REQUIRE_WASMTIME=1` turns runtime-gated execution tests from a skip into a
failure, so a missing runtime cannot silently pass CI.
