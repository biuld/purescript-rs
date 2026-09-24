# DEC-04 — Frontend and Backend Feature Matrices

**Status:** Accepted  
**Date:** 2026-09-19

## Context

The official PureScript test suite is a useful compatibility oracle, but a
single error-code ladder is not a sufficient implementation roadmap. It mixes
two different kinds of work:

- **Frontend compatibility:** PureScript source syntax, modules, name
  resolution, kinds, type checking, type classes, and diagnostics.
- **Backend capability:** CC and MIR lowering, runtime representation, Wasm
  emission, WIT bindings, WASI integration, and execution.

The current roadmap also makes a parser-only feature look implemented and can
make a backend feature look complete when it only works for one scalar slice.
The project needs a feature inventory that answers both questions separately:
what does PureScript mean, and can the resulting program run on the selected
platform?

The official suite remains pinned to the PureScript `v0.15.16` corpus. Its
passing, failing, warning, optimize, and layout cases are the acceptance
oracle, but the feature matrices in [D-04](../design/D-04-suite-roadmap.md)
are the primary planning artifact. A feature's implementation status and its suite acceptance status are tracked
together; the former cannot silently replace the latter.

## Decision

Maintain two feature matrices and update them whenever a feature moves through
the compiler:

1. The **frontend matrix** tracks source-language support through Typed Core.
2. The **backend matrix** tracks Typed Core through CC/MIR, Wasm, WIT, and
   WASI execution.

A feature is not marked `Implemented` merely because its syntax parses, a type
exists in an IR, or a Wasm opcode can be emitted. `Implemented` means that the
feature works end to end at the boundary named in its row, has a regression
test, and has reached its official-suite gate when the suite exercises that
feature. `Partial` is used for parser-only work, a restricted type/runtime
slice, an incomplete suite gate, or a feature whose representation exists but
is not yet connected through the whole pipeline.

Backend infrastructure that the official suite cannot observe directly—such
as a MIR verifier, a WIT registry, or WAT printing—also requires dedicated
unit/integration tests. Those tests prove the infrastructure contract; they do
not make the language or runtime suite complete. The passing-suite runtime gate
remains the final evidence for end-to-end backend compatibility.

## Matrix ownership

The mutable status legend, official-suite gate table, frontend and backend
feature matrices, target capability matrix, and stage crosswalk live in
[D-04](../design/D-04-suite-roadmap.md). Update those tables as coverage
changes; this record preserves the decision to track the two sides separately.

## Consequences

- Frontend work is planned like a language-compatibility project: each syntax,
  type-system, and diagnostic feature is landed and tracked independently.
- Backend work is planned like a target-integration project: CC/MIR invariants,
  Wasm capabilities, WIT canonical ABI, and WASI services have separate
  acceptance evidence.
- The same PureScript feature can appear in both matrices. For example,
  parameterized ADTs need frontend type checking and backend erased layouts;
  neither side can claim the feature alone is complete.
- The [frontend type-system design](../design/frontend/type-system/README.md)
  owns type representations and semantics. [D-04](../design/D-04-suite-roadmap.md)
  owns implementation coverage; backend designs own Wasm, WIT, and WASI details.
- Official `errorCode` agreement remains mandatory for frontend diagnostics,
  but message text and golden `.out` formatting are not compatibility criteria.
- The matrices make partial support explicit and prevent a parser-only feature,
  a single backend vertical slice, or a runtime-only test from being mistaken
  for full language support.
