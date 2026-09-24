# DEC-09 — GC-Only Language Heap

**Status:** Accepted  
**Date:** 2026-09-25

## Context

[DEC-05](DEC-05-wasmtime-feature-set.md) selected Wasm GC as the runtime
representation for aggregates and closures and explicitly rejected "staying on
minimal core WebAssembly with a linear-memory bump allocator as the language
heap". It stated that linear memory "remains only for the byte-oriented WASI
boundary and is not the language heap".

Later work moved away from that decision. [D-10](../design/D-10-linear-memory-representation.md)
reclassified linear memory as "a real language-heap strategy", P9 gained a
second planner (`LinearMemoryPlanner`) that lowers products, boxes, arrays,
variants, and table-backed closures to allocator and typed load/store MIR, and
the MIR verifier grew a large pointer-bounds analysis to justify those accesses.
The CAP rows and the [D-05](../design/D-05-backend-capability.md) audit tracked
that path as a supported alternative profile.

That linear path cannot be a complete execution strategy for PureScript. Its
allocator is a bump allocator with **no reclamation**: `D-10 §Allocator` states
"there is no reclamation yet; a program that allocates without bound will exhaust
memory and trap". A garbage-collected source language run under it therefore
requires a hand-written collector (root/stack maps, tracing, and eventually
compaction) on linear memory — a large, error-prone subsystem that duplicates
what the engine already provides. Maintaining a second language-heap planner and
its pointer-bounds verifier also carries a permanent cost for every new CC
operation.

## Decision

**Wasm GC is the only language-heap strategy.** This reaffirms
[DEC-05](DEC-05-wasmtime-feature-set.md) and retires the linear-memory language
heap:

- The GC planner is the sole representation planner. Products, variants, arrays,
  boxes, closures, and erased values are realized only with GC `struct`/`array`
  types, typed function references, and `call_ref`.
- **Linear memory is retained only as the canonical ABI boundary**: strings,
  byte lists, the return area for canonical calls, `cabi_realloc`, active data
  segments, and the `LinearLoad`/`LinearStore` byte operations that service
  them. It is not a general object heap for the language.
- The `LinearMemoryPlanner`, its language-heap object layouts (box/product/
  variant/array/closure-environment offsets), the linear erased boxing/unboxing
  path, and the MIR pointer-bounds verifier for language objects are removed.
  The ABI-boundary allocator (`cabi_realloc`) and string representation remain.
- The MVP linear capability profile and its language-heap execution evidence are
  retired from the supported contract. CC remains target-neutral; the decision
  removes a realizing planner, it does not make CC GC-specific.
- Target-neutral requirements and CC operations introduced for the second
  planner remain valid, because they are what keep CC independent of GC; only
  their linear realization is removed.

## Consequences

- The backend owns no collector and never has to grow one, matching DEC-05's
  rejection of the linear-memory language heap.
- The linear planner, its layouts, and the language-object pointer-bounds
  verifier — the largest and most intricate part of the current backend — are
  deleted, and every future CC operation is implemented and verified once.
- The supported artifact requires a Wasm GC-capable runtime. MVP-only hosts are
  no longer a supported target. This is already the default profile
  (`TargetCapabilities::wasmtime_wasi_0_2()`, `gc: true`); the MVP-only profile
  remains useful only for isolated ABI-boundary tests, if at all.
- The wasm32 linear address model, strings, data segments, and `cabi_realloc`
  stay, so the canonical ABI and WASI boundary are unchanged.
- [D-10](../design/D-10-linear-memory-representation.md) is rewritten to describe
  only the canonical ABI boundary; the linear-language-heap parts of
  [D-05](../design/D-05-backend-capability.md),
  [D-06](../design/D-06-low-level-ir-and-wasm-types.md),
  [D-11](../design/D-11-gc-representation-and-evidence.md), and
  [DEC-08](DEC-08-target-neutral-variant-representation.md) are superseded.
- Reversing this decision would mean reintroducing a second planner **and**
  eventually a hand-written collector, so it is deliberately expensive to undo.
