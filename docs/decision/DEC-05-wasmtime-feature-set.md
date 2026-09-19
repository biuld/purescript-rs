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

The compilation target is the WebAssembly feature set implemented by a pinned
`wasmtime` release, not the minimal core specification. The current baseline is
**wasmtime 48**, and the concrete, per-feature profile is defined by
[D-05](../design/D-05-backend-capability.md).

- The backend may use any feature that the capability profile lists for the
  baseline: the standardized WebAssembly 3.0 set (garbage collection, function
  references, tail calls, exception handling, multiple memories, 64-bit memory,
  relaxed SIMD) and, once adopted there, proposals still in preview such as
  stack switching.
- Feature use is confined to the lowest representations. MIR and the structured
  Wasm encoding may carry target types and layouts; CST, AST, HIR, THIR, and
  Typed Core must not mention WebAssembly features, reference types, or offsets.
- Concretely, **Wasm GC is the representation for aggregates and closures**.
  Data types that are not all-nullary use GC `struct` types under an abstract
  supertype, and closures use a `struct` holding a `funcref` and its captures.
  Linear memory remains only for the byte-oriented WASI boundary and is not the
  language heap.
- The artifact assumes the documented baseline. Validation enables the same
  features, and execution tests run under the pinned `wasmtime`. The artifact
  may require these features to load.

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
