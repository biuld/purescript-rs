# D-02 — Wasm Lowering

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Goal

Compile the supported PureScript language subset to a portable WebAssembly
artifact for a WASI host. WASI is the first platform target. The initial
compatibility goal is PureScript language semantics plus the project's
PureScript-facing WASI libraries; Node.js and JavaScript FFI compatibility are
outside this target.

The artifact targets the WebAssembly feature set of a pinned `wasmtime` release
rather than the minimal core specification. It may use the standardized
WebAssembly 3.0 features that release implements—including garbage collection,
function references, tail calls, and exception handling—as well as proposals
that are still in preview. The baseline and its consequences are fixed by
[DEC-05](../decision/DEC-05-wasmtime-feature-set.md), and the concrete feature
list, WASI surface, and verification are defined by
[D-05](D-05-backend-capability.md). Target features are chosen in MIR and never
leak into the frontend IRs.

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
| MIR | Control flow is a graph of basic blocks; values and terminators are explicit; runtime layouts and calling conventions are fixed; its value and type model is the WebAssembly 3.0 type system ([D-06](D-06-low-level-ir-and-wasm-types.md)). |
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
  -> typed MIR / CFG -> structured Wasm -> .wasm and WAT
```

The supported program shape includes top-level direct functions, `Int`,
`Boolean`, `String`, and `Unit`, integer arithmetic and comparisons, string
literals, local scalar `let` bindings, and value-producing `if`. A data type
whose constructors are all nullary lowers to immediate integer tags and `case`
over it to tag comparisons. A non-parameterized data type with fields lowers to
one Wasm GC struct per constructor; construction uses `struct.new`, and field
patterns use `ref.test`, `ref.cast`, and `struct.get`. Top-level lambdas become
direct parameters. The `log` runtime function writes a `String` to standard
output and returns `Unit`. Nested or capturing lambdas, function values,
higher-order calls, records, and array operations are rejected with source
diagnostics. Concrete scalar array literals lower to Wasm GC `array.new_fixed`.
Newtypes are erased to their single field, and
constructor patterns in function parameters are lowered to an explicit
temporary parameter plus `case`. Parameterized ADTs use the erased runtime
representation selected by
[DEC-07](../decision/DEC-07-runtime-representation-for-parameterized-adts.md):
concrete instantiations may use the first erased layout slice, which boxes
parameter-dependent scalar fields as `eqref`; fully polymorphic declarations
and unsupported instantiations remain diagnostics.
Compatible source WIT imports are lowered through the generic canonical-ABI
adapter; mismatched source signatures, non-byte lists, and unsupported
aggregate results are rejected before MIR emission. Type inference supports rank-1 polymorphism: it
generalizes local `let` groups and top-level strongly connected components and
instantiates schemes at use sites. Declarations may carry a `name :: Type`
signature with function arrows and `forall`; the checker elaborates it with
rigid variables and checks the body against it. Because the backend does not
yet erase types or pass dictionaries, it rejects declarations whose checked
type is polymorphic. Type classes remain future work.

The backend is grouped into one bootstrap crate, while CC IR, MIR, and the
structured Wasm encoding remain separate Rust types with their own invariants.
MIR records basic blocks, instructions, branch targets, merge blocks, and block
parameters, and is the lowest long-lived IR. The Wasm structurer turns the
reducible diamonds emitted for expression-level `if` into structured `if`
regions; leaf opcodes reuse `wasm_encoder::Instruction` instead of a duplicate
opcode enum. `wasm-encoder` emits the core module, `wit-component` lifts it
into a component, `wasmparser` validates it, and `wasmprinter` prints WAT.

The CLI commands are `psrs build <file.purs>... [-o output.wasm]` and
`psrs wat <file.purs>... [-o output.wat]`. Multiple source files are resolved,
type checked, and linked together with the embedded `Prelude`. The artifact is
a WASI 0.2 component that exports `wasi:cli/run@0.2.12`. The core module exports the canonical
`wasi:cli/run@0.2.12#run` entry, which calls `main` and passes its result to
`wasi:cli/exit.exit-with-code`, so a compatible runtime such as `wasmtime run`
uses the value as the process exit code. The module exports its linear memory
for the canonical ABI.

String literals are placed in active data segments; a `String` value is the
address of a length-prefixed UTF-8 buffer. A program that calls `log` imports
`wasi:cli/stdout` and `wasi:io/streams`; the standard-library lowering reads the
buffer's length and calls `blocking-write-and-flush` with the bytes and then a
newline. The component imports only the WASI interfaces the program uses. See
`examples/hello.purs`. File, environment, and argument services are not
implemented yet; monotonic clock and random bytes are implemented. `psrs dump
<core|cc|mir> <file.purs>` prints any
intermediate IR for debugging.

## Type-system sequence

Build the fully typed THIR before Typed Core lowering. Grow type support in
stages:

1. Monomorphic integer, boolean, string, unit, and function types.
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

Keep target representation decisions in P9. Aggregates and closures use Wasm GC
per [DEC-05](../decision/DEC-05-wasmtime-feature-set.md); linear memory is
reserved for the byte-oriented WASI boundary, not for the language heap.
Establish a small ABI before adding services:

- `Int` uses signed 32-bit values; `Number` uses 64-bit floating point.
- `Boolean` uses an integer zero/one representation and `Char` a Unicode scalar.
- `String` values remain linear-memory pointers to a length-prefixed UTF-8
  buffer for the WASI boundary; literals live in data segments. A GC string
  representation can replace it later without changing source semantics.
- `Unit` has no payload and uses the integer zero.
- A data type whose constructors are all nullary uses immediate integer tags and
  allocates nothing.
- A data type with fields uses a `rec` group of GC `struct` types: one subtype
  per constructor under an abstract supertype, with each field a reference or a
  scalar. Constructor application allocates with `struct.new`, and pattern
  matching uses `br_on_cast`/`ref.test`.
- A valid single-field `newtype` is represented by its field. Its constructor
  and pattern are semantic Core operations but do not allocate a GC wrapper.
- Parameterized ADTs use the runtime-erasure policy in
  [DEC-07](../decision/DEC-07-runtime-representation-for-parameterized-adts.md):
  parameter-dependent fields use boxed erased references, while independent
  fields may remain unboxed after layout verification.
- Closures are GC `struct` values holding a `funcref` and their captures, called
  with `call_ref`.
- Records use a GC `struct` with a compile-time field shape, and arrays use a GC
  `array`.

The exact Wasm reference strategy may evolve, but it must not leak into CST,
AST, HIR, THIR, or Typed Core.

## WASI platform model

The platform target is a WASI 0.2 Component Model release; the choice and its
rationale are in [D-05](D-05-backend-capability.md). **WASI is the runtime ABI**:
the project does not define a separate host ABI
([DEC-06](../decision/DEC-06-runtime-interface-via-wit.md)). The PureScript-facing
standard library is built on WASI (for example `print` writes to
`wasi:cli/stdout`), and the backend lowers to WASI's canonical ABI. Keep the
layers separate:

```text
PureScript-facing standard library -> WASI interfaces -> host
```

MIR declares the WASI imports a program uses in an import table with their
canonical signatures; the backend emits calls to those imports and
`wit-component` lifts the core module into a component. A component imports only
the WASI capabilities the program uses. The console, clock, and exit
capabilities are implemented: `log` writes to `wasi:cli/stdout`, `error` to
`wasi:cli/stderr`, `now` reads `wasi:clocks/monotonic-clock`, and `main`'s
result exits through `wasi:cli/exit`.

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
| M2 | Partial: stable IDs, a module graph, imports/exports, and value resolution across modules; type and constructor namespaces pending |
| M3 | Partial: monomorphic `Int`, `Boolean`, function inference, and THIR |
| M4 | Implemented Typed Core lowering and verifier; optimization is pending |
| M5 | Implemented direct-style integer Wasm through MIR/CFG, a WASI command entry, binary validation, and WAT output |
| M6 | Partial: nullary data types, non-parameterized field constructors, newtype erasure, constructor argument patterns, a first erased concrete parameterized-ADT slice, and concrete scalar array literals lower and run through Wasm GC; fully polymorphic ADTs, records, and array operations pending |
| M7 | ANF, closure conversion, and higher-order functions |
| M8–M9 | Type classes, records, rows, and broader PureScript semantics |
| M10 | WASI runtime and PureScript-facing base libraries |
