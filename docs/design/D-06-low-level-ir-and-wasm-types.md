# D-06 — Low-Level IR and Wasm 3.0 Value Model

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Purpose

Define the language-agnostic low-level half of the compiler: the same-level
representation below Typed Core and the thin Wasm encoding, expressed in terms
of the WebAssembly 3.0 value and type system. Layout, ABI, and target features
live here once, so the frontend never depends on them and the backend can grow
without knowing PureScript. This complements [D-05](D-05-backend-capability.md)
(which features are available) and [D-02](D-02-wasm-lowering.md) (the pass
pipeline).

## Boundary

- PureScript semantics—modules, names, algebraic data types, classes, source
  patterns, `do`, records—end at Typed Core. Below Core, an IR models general
  typed values, control flow, closures, and Wasm types; the only
  source-language names that survive are runtime ABI symbols and stable IDs.
- **CC IR** makes evaluation order, captures, and direct-versus-indirect calls
  explicit (ANF).
- **MIR** is a typed control-flow graph in SSA form with block parameters; it
  owns runtime types, layouts, and calling conventions and is the lowest
  long-lived IR.
- **Thin Wasm encoding** models the module skeleton, structured control, and the
  Wasm type section; leaf opcodes are delegated to `wasm_encoder`, per
  [DEC-02](../decision/DEC-02-thin-structured-wasm-encoding.md).

## Value and type model

The shared target model is the WebAssembly 3.0 type system, not a
source-language type system:

- **Value types:** `i32`, `i64`, `f32`, `f64`, and references `(ref null? ht)`.
- **Heap types:** the abstract `func`, `extern`, `any`, `eq`, `i31`, `struct`,
  `array`, and defined-type indices.
- **Type definitions:** recursion groups of defined types—`func`, `struct`, and
  `array`—with declared supertypes and finality (`sub`), and fields with a
  storage type and mutability.
- A logical value (for example a logical boolean) maps to a Wasm value type at
  the MIR boundary. MIR and the Wasm encoding never carry a source type.

MIR owns the defined-type table; the thin Wasm encoding emits it. The type
index space is the single Wasm type index space shared with function types. The
lowering emits the defined-type prefix first. This prefix may contain GC types
and nominal function types used by typed function references; plain function
types used only by fallback calls or ABI entries are appended after that
prefix. Defined types must come first because a function signature may
reference a defined type. This rule keeps type indices stable for instructions
such as `struct.new` and `call_ref` and is recorded here because both MIR and
the Wasm encoding depend on it.

## Instructions

The instruction set is target-level and curated; it is not a mirror of the Wasm
opcode set. A first set covers:

- numeric constants and arithmetic; local and global access;
- linear-memory loads and stores (for the WASI byte boundary);
- control flow: `if`, and later `block`/`loop`/`br`/`br_table`;
- calls: direct `call` and indirect `call_ref`;
- references and GC: `ref.null`, `ref.func`, `ref.test`, `ref.cast`,
  `br_on_cast`, `i31.new`/`i31.get`, `struct.new`/`struct.get`/`struct.set`,
  `array.new`/`array.get`/`array.set`/`array.len`.

Leaf instructions reuse `wasm_encoder::Instruction`; the IR adds structure
(regions), ownership, and layout.

## Control flow and structuring

MIR is a CFG whose block parameters replace phis (the Cranelift model), and
jumps pass arguments. The thin Wasm encoding is structured; the current
structurer recovers only expression-level `if` diamonds. General loops and
multi-way branches require `block`/`loop`/`br_table`, which is required
follow-up work rather than a permanent limitation.

## ABI and runtime boundary

Direct calls and indirect (`call_ref`) calls are distinct. Closures are GC
`struct` values holding a `funcref` and their captures, per
[D-02](D-02-wasm-lowering.md). Host services are **WASI** itself: the project
does not define its own host ABI
([DEC-06](../decision/DEC-06-runtime-interface-via-wit.md)). MIR declares WASI
imports in an import table with their canonical ABI signature (derived from the
vendored WASI WIT) and calls them like any other function, including a void
call for imports with no result.

MIR keeps only that canonical signature. A WIT import is referenced by its ABI
symbol; the interface and function names, the return-pointer convention, and
`cabi_realloc` live in the ABI layer (`psrs-backend::abi`), which names each
import when the Wasm encoding is emitted. Adapting a value to the canonical ABI
(for example a string to a `(ptr, len)` pair, or narrowing a 64-bit result) is
emitted as ordinary MIR instructions by a dedicated adaptation module, not by
the generic MIR lowering, so no component-model detail enters the IR. The
PureScript-facing standard library (`print`/`error`, `now`, program exit, and
later files/random) is built on these WASI imports.

## Layout ownership

P9 (Representation Lowering) chooses how higher-level values map onto the target
model: which sums become integer tags versus GC structs, how closures are
shaped, field order, and which scalars stay unboxed. The low-level IR does not
know about constructors, classes, or records.

## Invariants

- A pass consumes one representation and produces the next; no layout or type
  from a higher representation enters a lower one.
- Every IR has a verifier, run after lowering.
- Source spans remain where diagnostics and debugging need them.
- No source-language names appear below Core except runtime ABI symbols and
  stable IDs.

## Best-practice alignment

The design follows established compiler practice rather than inventing a
pipeline: MLIR-style progressive lowering with per-level verifiers; SSA/CFG
with block parameters as in Cranelift IR and LLVM IR; a Wasm-focused typed IR
in the spirit of Binaryen; ANF and closure conversion from the functional
compiler literature; and Wasm GC layouts as used by Kotlin/Wasm, Dart, MoonBit,
and OCaml's Wasm backend.

## Implementation phases

1. Add the target value/type model and let the thin Wasm encoding declare and
   emit defined (GC) types. *(implemented)*
2. Give MIR its own value/type model and defined-type table, with verification
   and lowering into the Wasm type section. *(implemented)*
3. Add MIR reference and GC instructions with lowering and verification.
   *(implemented for `ref.null`, `ref.is_null`, `ref.test`, `ref.cast`,
   `ref.func`, typed `call_ref`, `i31.new`/`i31.get`,
   `struct.new`/`get`/`set`, and `array.new`/`get`/`set`/`len`; `br_on_cast`
   still needs general structured control.)*
4. Lower data types, records, and closures into the model in P9.
5. Generalize structured control flow beyond `if` diamonds.

The runtime is a separate track: MIR has an import table, void calls, and
linear-memory load/store, and `psrs-backend::abi` resolves the WASI imports the
standard library uses into canonical ABI signatures from the vendored WASI WIT.
Componentization uses `wit-component`: the compiler vendors WASI 0.2.12 WIT,
resolves a `command` world that exports `wasi:cli/run@0.2.12` and imports
`wasi:cli/stdout` and `wasi:io/streams`, annotates a core module with world
metadata, and lifts it into a component. A core module that imports those
interfaces, calls `get-stdout` and `blocking-write-and-flush`, and exports
`run` under the legacy core name `wasi:cli/run@0.2.12#run` componentizes and
writes through WASI under `wasmtime`. The frontend `log` and `main` emit this
WASI sequence and `build` emits a component; the Preview 1 emitter is gone.

## Open items

- The exact sum encoding (abstract supertype per data type versus a tag field
  with a payload) is a P9 decision recorded when data types are lowered.
- Whether `array`/`i31` are used for specific values is a layout choice, not a
  change to this model.
- Structure recovery for loops and multi-way branches.
