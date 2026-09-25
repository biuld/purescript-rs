# DEC-02 — Thin Structured Wasm Encoding

**Status:** Accepted  
**Date:** 2026-09-19

## Context

MIR is the lowest long-lived semantic IR and is an arbitrary CFG. WebAssembly,
however, requires structured control flow (`block`/`loop`/`if` plus labels), so
the backend must recover structure before emission. The project also already
depends on `wasm-encoder` for binary generation.

Two broad options were considered: keep MIR as the only backend representation
and encode bytes while structuring, or introduce a full Wasm IR that models the
instruction set. A full opcode IR duplicates `wasm-encoder`, grows with the
Wasm instruction set, and must be kept in sync with it. Encoding directly from
MIR conflates structuring with byte emission and leaves MIR carrying target
structuring hints.

## Decision

Introduce a thin structured Wasm encoding between P10 and P11. It models only:

- the module skeleton: function signatures, functions, and exports;
- structured control flow as nested `if`/`else` regions.

Leaf opcodes are `wasm_encoder::Instruction` values. The encoding does not
re-declare the Wasm instruction set, so it grows with language features rather
than with the Wasm opcode count. MIR remains the lowest long-lived IR and the
architecture keeps six IR families.

## Consequences

- Structuring becomes explicit and independently verifiable, and byte emission
  stays in P11.
- Adding a feature extends the skeleton or the structured nodes, not an opcode
  mirror; `wasm-encoder` owns the instruction set.
- Runtime layouts and calling conventions still belong to MIR, not to the Wasm
  encoding.
- MIR briefly carried a `merge_block` hint for the bootstrap diamond
  structurer. That constraint has been removed: the structurer now derives
  branch and switch joins from the CFG edges, and general structuring handles
  loops and multi-way branches.
- Rejected: a full Wasm IR mirroring opcodes (duplication and unbounded growth);
  encoding directly from MIR (mixes structuring with emission and leaves no
  verifiable structured artifact).
