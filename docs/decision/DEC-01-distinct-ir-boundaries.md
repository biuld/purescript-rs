# DEC-01 — Keep Compiler Representations Distinct

**Status:** Accepted  
**Date:** 2026-09-19

## Context

The compiler needs source-oriented syntax, semantic trees, typed forms, and
backend representations. Reusing one broad tree would mix source syntax with
resolved IDs, types, and runtime layout, and would obscure which invariants a
pass may rely on. The project also needs a staged path to WebAssembly without
maintaining a separate IR for every transformation.

## Decision

Use twelve major passes and six long-lived IR families:

1. CST
2. AST
3. Resolved HIR
4. THIR
5. Typed Core
6. CC IR / MIR

Each pass has an explicit input and output representation. CST and AST are
separate; parsing produces CST and a named lowering pass produces AST. Name
resolution, type elaboration, Core lowering, closure conversion, and
representation lowering each establish their own invariants. Passes such as
Core optimization preserve their IR instead of inventing a new representation.
TokenStream is parser input rather than an IR family. CC IR and MIR remain
distinct representation types while belonging to one backend family. MIR is
the lowest IR and is lowered into a thin structured Wasm encoding before
binary emission; Wasm is a target encoding, not an IR family.

## Consequences

- Source punctuation and token ranges stay in CST without becoming semantic
  data in later representations.
- AST names remain unresolved; stable IDs first appear in Resolved HIR.
- THIR holds checked types and explicit class evidence; Typed Core remains a
  typed functional backend boundary.
- Closure conversion and runtime representation are explicit backend passes.
- Pass APIs make invalid stage ordering difficult to express in Rust.
- Crates may be added for implemented boundaries, but empty placeholders are
  not required in advance.

See [D-01](../design/D-01-frontend-and-ir-boundaries.md) and
[D-02](../design/D-02-wasm-lowering.md) for the current contracts.
