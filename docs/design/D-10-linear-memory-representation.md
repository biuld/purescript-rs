# D-10 — Linear Memory and the Canonical ABI Boundary

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Purpose

Define how the backend uses linear memory now that
[DEC-09](../decision/DEC-09-gc-only-language-heap.md) makes Wasm GC the only
language-heap strategy. Linear memory is retained **only** for the
byte-oriented canonical ABI and WASI boundary: strings, byte lists, the return
area for canonical calls, `cabi_realloc`, and active data segments. It is not a
general object heap, and there is no linear-memory planner for language
aggregates, closures, variants, or erased values. This complements
[D-06](D-06-low-level-ir-and-wasm-types.md) (planner contract and IR boundaries),
[D-07](D-07-wit-imports-and-std.md) (canonical ABI adaptation), and
[D-09](D-09-scalar-and-numeric-lowering.md) (scalar semantics).

The earlier version of this document described a linear-memory language heap
with per-shape object layouts, a tag/payload variant encoding, erased boxing,
table-backed closure environments, and a pointer-bounds verifier. Those parts
are superseded by [DEC-09](../decision/DEC-09-gc-only-language-heap.md) and
removed with the `LinearMemoryPlanner`.

## Address model

- The linear target is **wasm32**: every ABI pointer is an `i32` byte offset into
  a single memory. `memory64` is disabled in the capability profile
  ([D-05](D-05-backend-capability.md)), so the address type and the `ValueType`
  of every ABI pointer are `i32`.
- The planner assigns a `MemoryId`; the current profile has exactly one memory
  (`MemoryId(0)`). `multi_memory` is disabled. A `LinearLoad`/`LinearStore`
  carries the `MemoryId` it addresses so a future profile can add memories
  without changing CC.
- The address type is an ABI detail, not a CC concept: CC references are opaque
  handles owned by the GC planner. Memory64 is revisited only when the pinned
  component toolchain lifts 64-bit-memory modules and the WASI host supports
  them, with its own lowering and execution tests.

## Strings

A `String` is an `i32` pointer to a length-prefixed UTF-8 buffer: a 4-byte
little-endian length followed by the bytes. This is the same value the WASI
canonical ABI consumes, so the boundary needs no conversion. String literals
live in active data segments. Because the canonical ABI exchange format is
bytes, strings stay linear-memory values even though the language heap is GC
([D-11](D-11-gc-representation-and-evidence.md)).

## ABI allocator

- `cabi_realloc` is a bump allocator: a free pointer lives in linear memory and
  advances by the aligned size. The allocator aligns the payload pointer it
  returns, then writes the byte-length prefix in the preceding 4 bytes.
- It serves canonical ABI return values and byte buffers. There is no
  reclamation: it is not a language heap, so programs are not expected to
  allocate unbounded language data through it
  ([DEC-09](../decision/DEC-09-gc-only-language-heap.md)).
- A reserved scratch region at the start of memory holds the return-pointer area
  of canonical ABI calls; string and byte data begin after it.
- The allocator grows memory on demand with `memory.grow` (`bulk_memory` is not
  required).

## Instruction contract

P9 lowers canonical ABI adaptation to these MIR instructions. The MIR verifier
checks their value types, memory identity, and alignment, and that a statically
addressed access stays within the planned ABI object extent.

- `LinearAlloc { bytes, alignment }` produces an `i32` pointer for an ABI
  buffer; `bytes` is nonzero and aligned.
- `LinearLoad`/`LinearStore` carry `MemoryId`, a byte `offset`, the planned
  object extent available from the address, and the `ValueType` of the access.
  The verifier checks that `offset + access width` stays within that extent, that
  the access type matches the stored scalar, and that its alignment does not
  exceed the planned alignment.
- Byte-level copy for return areas and byte lists uses MVP loads, stores, and
  structured branches where the canonical ABI requires a copy. Language array
  updates and clones use GC `array.set`/`array.copy`
  ([D-11](D-11-gc-representation-and-evidence.md)), not linear instructions.

`i64` accesses are permitted only for canonical ABI adaptation
([D-07](D-07-wit-imports-and-std.md)); source-level values do not use `i64` yet.

## Capability gating

The retained linear operations require only core MVP; no language-heap
representation exists to select. A future profile that needs a byte-oriented
boundary for a host without GC is out of the supported contract
([DEC-09](../decision/DEC-09-gc-only-language-heap.md)). Any requirement the
selected profile cannot represent receives a source-associated P9 diagnostic
rather than a silent fallback.

## Delivery order

1. Remove the `LinearMemoryPlanner` and its language-heap layouts
   (box/product/variant/array/closure offsets), the linear erased
   boxing/unboxing path, and their lowering entry points.
2. Remove the MIR language-object pointer-bounds verifier and the
   `LinearMemoryCopy`/`ArrayClone` linear path.
3. Keep `LinearAlloc`, `LinearLoad`, `LinearStore`, `MemoryId`, strings, data
   segments, `cabi_realloc`, and the ABI adapter.
4. Retire the MVP linear capability profile and its language-heap execution
   tests; keep only ABI-boundary validation and execution tests.
5. Align [D-05](D-05-backend-capability.md), [D-06](D-06-low-level-ir-and-wasm-types.md),
   [D-11](D-11-gc-representation-and-evidence.md), and
   [DEC-04](../decision/DEC-04-official-test-suite-roadmap.md) with the removal.
