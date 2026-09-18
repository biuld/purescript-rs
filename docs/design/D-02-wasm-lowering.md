# D-02 — Wasm Lowering

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Goal

Compile the supported PureScript language subset to a portable WebAssembly
artifact for a WASI host. WASI is the first platform target. The initial
compatibility goal is PureScript language semantics plus the project's
PureScript-facing WASI libraries; Node.js and JavaScript FFI compatibility are
outside this target.

The backend starts from Typed Core, the stable frontend/backend boundary. Wasm
is the primary backend. A native executable, if added, should embed or run the
Wasm artifact rather than introduce an independent code generator.

## Pipeline and contracts

The full pass order is specified in [D-01](D-01-frontend-and-ir-boundaries.md).
The backend stages are:

```text
P6  THIR -> Typed Core
P7  Typed Core -> optimized Typed Core
P8  Typed Core -> ANF / CC IR
P9  CC IR -> MIR / CFG
P10 MIR -> structured Wasm encoding
P11 structured Wasm encoding -> Wasm/WASI artifact
```

| Stage | Required invariant |
| --- | --- |
| Typed Core | Types and semantic IDs remain explicit; source sugar and source patterns are lowered. |
| CC IR | Evaluation order, closure captures, and direct versus indirect calls are explicit. |
| MIR | Control flow is a graph of basic blocks; values and terminators are explicit; runtime layouts and calling conventions are fixed. |
| Structured Wasm encoding | The module skeleton is explicit and control flow is structured; leaf opcodes are `wasm_encoder::Instruction` values, not a re-declared instruction set. |
| Artifact | The encoded module validates, uses the selected target layout, and declares the WASI interfaces it uses. |

CC IR and MIR are separate representations in one backend IR family. ANF is a
form within CC IR, not an additional long-lived IR. MIR is the lowest
long-lived IR and owns runtime layout decisions; it lowers into a thin
structured Wasm encoding that P11 emits. Wasm is a target encoding, not a
separate IR family, and the encoding does not mirror the Wasm instruction set.

No backend stage may infer semantic identity from source text. Source and
typed-source stages may not depend on memory offsets, Wasm indices, or target
calling conventions.

## First executable slice

Start with a module containing `main` and support integer and boolean values,
arithmetic, direct function calls, `let`, and `if`. The bootstrap may combine
implementations of passes that have no independent capability in this slice,
but it must preserve the Typed Core boundary and lower through an explicit
MIR/CFG before Wasm emission. Add algebraic data types and pattern matching
next; add captured closures and higher-order functions after that.

Use a Wasm encoder and validator for binary generation. Keep readable dumps of
Typed Core, CC IR, and MIR. Test observable behavior rather than binary byte
identity.

## Implemented vertical slice

The bootstrap compiler now lowers this source subset through every IR family:

```text
module source -> resolved HIR -> THIR -> Typed Core -> direct-call CC IR / ANF
  -> scalar MIR / CFG -> structured Wasm -> .wasm and WAT
```

The supported program shape includes top-level direct functions, `Int` and
`Boolean`, integer arithmetic and comparisons, local scalar `let` bindings,
and value-producing `if`. Top-level lambdas become direct parameters. Nested or
capturing lambdas, function values, higher-order calls, imports, and aggregate
values are rejected with source diagnostics. Type inference is monomorphic;
generalization and type classes remain future work.

The backend is grouped into one bootstrap crate, while CC IR, MIR, and the
structured Wasm encoding remain separate Rust types with their own invariants.
MIR records basic blocks, instructions, branch targets, merge blocks, and block
parameters, and is the lowest long-lived IR. The Wasm structurer turns the
reducible diamonds emitted for expression-level `if` into structured `if`
regions; leaf opcodes reuse `wasm_encoder::Instruction` instead of a duplicate
opcode enum. `wasm-encoder` emits the binary and `wasmparser` validates it
before the compiler reports success. `wasmprinter` prints WAT from the
validated binary.

The CLI commands are `psrs build <file.purs> [-o output.wasm]` and
`psrs wat <file.purs> [-o output.wat]`. The generated core Wasm module exports
a zero-argument `Int` function as `main`. It is not yet a standalone WASI
command or a Component Model artifact: imports, runtime services, WASI
adapters, component linking, and a start function have not been implemented.
`psrs dump <core|cc|mir> <file.purs>` prints any intermediate IR for
debugging.

## Type-system sequence

Build the fully typed THIR before Typed Core lowering. Grow type support in
stages:

1. Monomorphic integer, boolean, and function types.
2. Hindley–Milner inference, unification, occurs check, generalization, and
   instantiation.
3. Algebraic data types and constructor applications.
4. Kinds and row-polymorphic records.
5. Type classes, instance resolution, and explicit dictionary evidence.
6. Advanced PureScript behavior such as higher-rank types and functional
   dependencies.

Later type-system features must enter typed representations or explicit
lowering passes. Do not add type-system special cases to the Wasm emitter.

## Runtime and representation

Keep target representation decisions in P9. The first runtime can use a bump
allocator for heap objects. Establish a small ABI before adding services:

- `Int` uses signed 32-bit values; `Number` uses 64-bit floating point.
- `Boolean` uses an integer zero/one representation and `Char` a Unicode scalar.
- Strings use pointer and length in linear memory for the initial profile.
- Algebraic data values carry a constructor tag and payload.
- Closures pair a table entry with an environment reference.
- Records use a compile-time shape and a runtime layout selected during MIR
  lowering.

The exact Wasm reference strategy may evolve, but it must not leak into CST,
AST, HIR, THIR, or Typed Core. A production garbage collector and Wasm GC
objects are later work.

## WASI platform model

Use WASI 0.2 Component Model/WIT as the initial platform baseline. The
PureScript-facing library calls a stable compiler runtime ABI; the runtime
adapter maps that ABI to WASI interfaces. Keep the three layers separate:

```text
PureScript WASI library -> compiler runtime ABI -> WASI host interface
```

Initial library capabilities grow as testable modules for console, arguments,
environment, files, clock, and randomness. Networking and HTTP are later
work. A later WASI profile may expose asynchronous streams and futures without
changing frontend or Core representations.

The compiler frontend recognizes language symbols and runtime intrinsics; it
does not implement Node APIs. Existing `.js` FFI modules and Node built-ins
are not compatibility targets. User-defined foreign interfaces need an
explicit, target-aware ABI and are not mixed into Typed Core.

## Validation

- Unit-test each lowering pass with small input/output cases and invariant
  checks.
- Verify generated Wasm modules before reporting successful compilation.
- Keep dedicated dumps for Core, CC IR, and MIR available.
- Compare source acceptance with the official `purs` compiler as language
  coverage grows.
- Run execution tests that compare observable results under a compatible WASI
  runtime.

## Milestones

| Milestone | Capability |
| --- | --- |
| M0–M1 | Implemented source inspection, CST, and normalized AST subset |
| M2 | Partial: same-module value resolution and stable IDs; module graph is pending |
| M3 | Partial: monomorphic `Int`, `Boolean`, function inference, and THIR |
| M4 | Implemented Typed Core lowering and verifier; optimization is pending |
| M5 | Implemented direct-style integer Wasm through MIR/CFG, binary validation, and WAT output |
| M6 | Algebraic data types and pattern matching |
| M7 | ANF, closure conversion, and higher-order functions |
| M8–M9 | Type classes, records, rows, and broader PureScript semantics |
| M10 | WASI runtime and PureScript-facing base libraries |
