# DEC-04 — Official Test Suite Is the Compatibility Roadmap

**Status:** Accepted  
**Date:** 2026-09-19

## Context

[D-03](../design/D-03-type-system.md) lays out a self-defined type-system
roadmap (phases T1–T6). The official PureScript checkout provides a large,
maintained corpus under `tests/purs`:

- `passing`: 440 `.purs` files that must compile and run.
- `failing`: 448 `.purs` files with `-- @shouldFailWith <ErrorCode>` directives
  and `.out` golden diagnostics.
- `warning`: 68 files with expected warnings.
- `optimize`: 10 files with expected CoreFn output.
- `layout`: 15 self-contained lexer/layout goldens with `.out` files.

The official `purs` compiler is available and `purs compile --json-errors`
emits a machine-readable `errorCode` per diagnostic (for example
`TypesDoNotUnify`, `ErrorParsingModule`, `NoInstanceFound`). The corpus and the
oracle together describe exactly what the compiler must accept, reject, and
report.

[F-02](../feature/F-02-portable-programs.md) currently avoids promising broad
compatibility: language support grows in documented increments. Adopting the
official suite changes the project's scope and the meaning of "done", so it
needs an explicit decision.

## Decision

Use the official PureScript test suite as the compiler's compatibility target
and coverage oracle. Self-defined feature roadmaps are subordinate to suite
coverage.

- **Coverage is measured as per-file agreement with `purs`.** For every corpus
  file, compare our outcome against the oracle: accept vs reject, and, for
  rejects, the diagnostic `errorCode`. Progress is a burndown of
  disagreements.
- **The corpus is classified into layers by `purs` error code**, not by
  hand-picked feature lists:
  - L0 layout: `tests/purs/layout` goldens.
  - L1 parse: files the oracle reports as `ErrorParsingModule` must fail to
    parse; all other files must parse.
  - L2 resolve/name: `UnknownName`, `DeclConflict`, `TransitiveExportError`,
    export/import errors.
  - L3 kinds: `KindsDoNotUnify`, `PartiallyAppliedSynonym`, kind errors.
  - L4 types: `TypesDoNotUnify`, `InfiniteType`, `IntOutOfRange`, and related.
  - L5 classes: `NoInstanceFound`, `OverlappingInstances`, `OrphanInstance`.
  - L6 runtime: `passing` files produce the expected observable result.
- **Dependency order is retained.** Parse precedes resolve, resolve precedes
  kinds, kinds precede types, types precede classes, and classes precede
  runtime. The suite defines *what* to cover, not the order of construction.
- **Runtime is the last layer, not the first.** Almost all `passing` files
  import Prelude and platform libraries, so reaching L6 requires module
  loading, imports, the standard library, and the runtime. Parser and
  resolution coverage come first, with runtime vertical slices kept alive in
  parallel where practical.
- **Alignment is by `errorCode` and span, not message text.** Golden `.out`
  files contain formatting and ANSI color and must not be matched literally.
- **JavaScript and Node.js FFI are out of scope.** Suite files that declare
  `foreign import`, ship a `.js` FFI implementation, or expect an FFI-specific
  `errorCode` are excluded from milestone acceptance and are neither agreement
  nor gaps. Programs that use the project's PureScript-facing WASI libraries
  remain in scope; those libraries are implemented by the runtime.
- **The suite must not block the default workspace tests.** Tests that require
  `purs` or a PureScript checkout skip or are opt-in, so
  `cargo test --workspace` stays self-contained.

[D-03](../design/D-03-type-system.md) remains the source of truth for type
representations and internal sequencing; its phases map onto L3–L5.

## Alternatives considered

- **Keep the self-defined T1–T6 roadmap as primary (rejected).** It gives no
  objective coverage signal and risks building features the suite does not
  exercise while missing ones it does.
- **Read `@shouldFailWith` directives instead of running `purs` (rejected as
  primary).** The directives are useful metadata but the oracle is authoritative
  and also classifies files that fail for module-resolution reasons.
- **Vendor the corpus into this repository (rejected for now).** The suite is
  large and the support libraries are fetched, not vendored. Reference a local
  checkout via `PURESCRIPT_REPO` and keep the harness out of the default test
  path.

## Consequences

- The project makes a compatibility commitment that F-02 must restate: the
  official suite, by layer, defines done.
- Modules, imports, and eventually a standard library move earlier, because L2
  and L6 require them.
- The parser becomes a first-class large workstream with an automated
  agreement metric rather than an ad-hoc grammar.
- Every new diagnostic should align to an official `errorCode`; the
  `TypeCheckErrorKind` and resolver kinds gain a documented mapping.
- Risk: over-fitting to error codes or to individual files. Mitigation is to
  derive rules from language semantics, with the suite as regression coverage.
- Follow-up: build the classifier and scoreboard harness, add `parse_source`
  for an L1-only entry point, and update F-02 and D-03 to reference the layers.
