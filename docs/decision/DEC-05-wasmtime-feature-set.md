# DEC-05 — Target wasmtime's WebAssembly Feature Set

**Status:** Accepted  
**Date:** 2026-09-19

## Context

The project promises a portable WebAssembly artifact that runs under a WASI
host, and [D-02](../design/D-02-wasm-lowering.md) previously implied the
minimal core WebAssembly specification plus a hand-written linear-memory bump
allocator for heap objects. Modern `wasmtime` releases implement the
standardized WebAssembly 3.0 recommendation—which includes garbage collection,
function references, tail calls, exception handling, multiple memories, 64-bit
memory, and relaxed SIMD—and a range of proposals that are still in the
preview/phase-3 stage.

Reproducing engine capabilities in linear memory—manual layout, allocation, and
eventually a collector—duplicates work the engine already does, loses type
safety, and delays every aggregate-level language feature. The project needs an
explicit policy for which WebAssembly features an artifact may require, and how
that requirement is pinned and verified.

## Decision

The execution oracle is a pinned **wasmtime 48** baseline, but the compilation
target is an explicit, per-feature capability profile defined by
[D-05](../design/D-05-backend-capability.md), not every proposal the runtime
can execute. The default profile is
`TargetCapabilities::wasmtime_wasi_0_2()`.

- The backend may emit only features enabled in the selected profile. The
  stable profile requires MVP Wasm plus the GC/reference/function-reference
  subset used by the current runtime representation and the Component Model
  with WASI 0.2. SIMD, tail calls, exceptions, threads, multi-memory,
  memory64, wide arithmetic, async components, and WASI 0.3 remain disabled.
- A proposal is not `Implemented` because Wasmtime accepts it. It needs a
  lowering, profile validation, regression tests, and observable execution
  evidence. A profile flag is an adoption gate, not a claim that the current
  compiler already emits every instruction in that proposal.
- Feature use is confined to the lowest representations. MIR and the structured
  Wasm encoding may carry target types and layouts; CST, AST, HIR, THIR, and
  Typed Core must not mention WebAssembly features, reference types, or offsets.
- Concretely, **Wasm GC is the representation for aggregates and closures**.
  Data types that are not all-nullary use GC `struct` types under an abstract
  supertype, and closures use a `struct` holding a `funcref` and its captures.
  Linear memory remains only for the byte-oriented WASI boundary and is not the
  language heap.
- The artifact assumes the selected profile. Validation is built from the same
  flags, and execution tests run under the pinned `wasmtime`. If a lowering
  needs a disabled capability, compilation fails with a backend diagnostic;
  there is no implicit fallback ABI.

## Consequences

- Aggregate and closure support arrives without a hand-written allocator or
  collector, and with typed references and engine-managed memory.
- "Portable" is scoped to engines that implement the same feature set as the
  baseline, which is broader than the minimal core but narrower than "any Wasm
  engine". The baseline version must be documented and kept current.
- The Wasm encoder and validator must emit and enable the features; WAT output
  and the structured encoding grow with the language, not with a re-declared
  instruction set ([DEC-02](DEC-02-thin-structured-wasm-encoding.md)).
- Because preview proposals can change, the baseline is a deliberate revision
  point: adopting a new `wasmtime` baseline is an update to this decision, and
  a dropped or changed proposal is revisited here rather than worked around in
  the frontend.
- Rejected: staying on minimal core WebAssembly with a linear-memory bump
  allocator as the language heap. It duplicates GC, loses type safety, and
  blocks higher-level language features behind manual layout work.
