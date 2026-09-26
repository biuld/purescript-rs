# DEC-10 — Canonical ABI Buffer Ownership and Lifetime

**Status:** Accepted
**Date:** 2026-09-26

## Context

[DEC-09](DEC-09-gc-only-language-heap.md) made Wasm GC the only language heap
but kept strings in linear memory and accepted the `cabi_realloc` bump
allocator's "no reclamation" behavior. A bump allocator with no reclamation is
an MVP artifact, not a complete design: Wasm GC has no finalizers, so a
language string or list held in linear memory cannot be freed automatically
when the owning GC value becomes unreachable. A complete design therefore
cannot leave language values in linear memory unless it also writes a
collector, which DEC-09 rejected.

The repository's documentation principle requires the design to describe the
complete target rather than a bootstrap. This record fixes the canonical ABI
buffer model accordingly and revises the string clause of DEC-09.

## Decision

**All language values are GC-managed; linear memory is transient canonical ABI
scratch.** Dynamic strings, byte lists, and every other source value live on
the GC heap. Linear memory holds only buffers whose lifetime is bounded by a
canonical call or by an export's `post-return`; every such buffer has exactly
one owner and is freed when its lifetime ends.

- **String and byte-list representation.** A source `String` is a GC
  byte-sequence value (a GC array of bytes with its length). It is not an
  `i32` linear pointer. Static string literals are materialized with
  `array.new_data` from passive data segments, so a literal is never copied
  through linear memory. Language arrays and aggregates already use GC.
- **`cabi_realloc` is a general aligned allocator.** It implements the full
  canonical `realloc(old_ptr, old_len, align, new_len)` contract: `new_len == 0`
  frees, `old_ptr == 0` allocates, otherwise it resizes with copy. Freed blocks
  are coalesced and reused; allocation is aligned to `align` and never overlaps
  live static regions. `memory.grow` failure or a representable-address overflow
  traps. It is not a bump allocator.
- **Buffer ownership and lifetime.** Every canonical buffer is one of:
  - *static* — passive data segments and read-only literal storage; program
    lifetime, never freed;
  - *call-local* — indirect parameter records and import return areas; owned by
    the calling function and freed when the canonical call returns;
  - *import result* — host-written through the guest `allocator`; owned by the
    guest. The ABI layer copies the contents into a GC value and frees the
    buffer before returning control to source, so no linear buffer outlives the
    call that produced it;
  - *export result* — guest-allocated for a value returned from a guest export;
    owned by the host until it is lifted, then freed by the export's synthesized
    `post-return`.
- **`post-return`.** Every guest export whose lift requires linear memory (any
  non-scalar result) gets a synthesized `cabi_post_<name>` that frees the return
  area and any guest-allocated buffers it owns. `wasi:cli/run` returns no
  aggregate, so the current only export needs none, but the design requires one
  wherever the shape demands it.
- **Resource handles.** An `own<T>` handle is a guest-owned index into a
  resource table with a drop obligation; a `borrow<T>` handle is a
  non-owning, call-scoped reference whose borrow must not outlive the call.
  Lowering inserts `resource.drop` for owned handles when the value is consumed
  and releases borrows at call return; `post-return` releases handles owned by
  export results.
- **Memory and pointer width are profile parameters.** The canonical ABI
  adapter selects the ABI memory; a wasm32 profile uses `i32` addresses and one
  memory, a memory64 profile uses `i64`, and a profile that enables
  `multi_memory` may name more than one ABI memory. CC references stay opaque,
  so this changes only MIR and the encoder.

## Rejected alternatives

- **Dual language-level strings (`linear` or `GC` as a tagged union).** Rejected.
  Every string operation (equality, concatenation, dictionary keys, captures,
  boxing) would branch on the representation, and the two representations would
  need one equality and hash contract. Escape analysis could not soundly remove
  either form, and a linear string stored in a GC structure would reintroduce
  the uncollectable-buffer problem this record fixes. There is one semantic
  string type; linear exists only as a transient ABI encoding of it.
- **Keeping the linear string pointer as an alternate target profile.** Rejected.
  DEC-09 retired the linear language heap; a second string representation would
  need a second set of layout, verifier, and reclamation rules for no gain.
- **A cached linear view beside a GC string.** Not part of the model. An
  implementation may later cache a linear view to avoid repeated copies on
  repeated boundary calls, but the semantic type stays one GC string and the
  cache is an internal optimization with its own lifetime rules.

## Consequences

- Reclamation is automatic and complete: a linear buffer exists only while a
  call or a `post-return` owns it, so growth is bounded by call frequency, not
  by allocation history. No collector, root map, or finalizer is needed.
- [linear ABI boundary](../design/backend/wasm/linear-memory-and-canonical-abi-boundary.md)
  and [canonical ABI and WIT](../design/backend/wasm/canonical-abi-and-wit.md)
  are rewritten to this model; [data representation](../design/backend/fp/data-representation.md),
  [scalars and primitives](../design/backend/fp/scalars-and-primitives.md),
  [polymorphism and erasure](../design/backend/fp/polymorphism-and-erasure.md),
  and [CC IR](../design/backend/fp/cc-ir.md) update the string shape from an
  `i32` ABI pointer to a GC value.
- The DEC-09 clause "linear memory is retained only as the canonical ABI
  boundary: strings, byte lists, ..." is narrowed: strings and byte lists cross
  the boundary but are GC-managed after adaptation, not stored in linear
  memory.
- Implementation notes record the remaining deviation: strings are GC
  `(array (mut i16))` values materialized from passive segments and transcoded
  at the boundary, but `cabi_realloc` is still a bump allocator, `post-return`
  is not synthesized, and resource handles are not lowered. The allocator,
  buffer lifetime, and `post-return` contract is designed and tracked by
  [canonical buffer allocation and lifetime](../design/backend/wasm/canonical-buffer-allocation-and-lifetime.md)
  and its [checklist](../implementation/backend/canonical-buffer-allocation.md);
  feature-row coverage stays in [D-04](../design/D-04-suite-roadmap.md).
