# Linear Memory and the Canonical ABI Boundary

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Stable (design)  
**Prerequisites:** the Canonical ABI exchange format (flattened values, the return pointer, `realloc`), WebAssembly linear memory and the wasm32 address model, and Wasm GC as the language heap. Read [canonical ABI and WIT](canonical-abi-and-wit.md), [MIR](../fp/mir.md), and [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md) first.  
**Summary:** Linear memory is retained only as the byte-oriented Canonical ABI and WASI boundary: strings, byte lists, the return area of canonical calls, the bump allocator `cabi_realloc`, and active data segments. It is not a general object heap. Language aggregates, closures, variants, arrays, and erased values use Wasm GC.

## Scope

This document owns the address model, the string and byte-list representation,
the `cabi_realloc` allocator, the scratch return area, data-segment use, and the
MIR byte-operation contract at the boundary. It does not own canonical ABI
adaptation itself ([canonical ABI and WIT](canonical-abi-and-wit.md)), the
structured encoding of memory instructions ([Wasm encoding](encoding-and-structuring.md)),
the choice of GC as the heap ([capability profile](capability-profile.md)), or
the exact GC layouts ([data representation](../fp/data-representation.md)).

## Background

**The Canonical ABI exchange format is bytes.** When a value cannot be passed in
core Wasm values, the Canonical ABI passes a pointer into linear memory to a
record laid out with computed offsets, alignment, and padding. Lists and strings
are passed as `(pointer, length)` pairs, and a return value that does not fit in
one core value is written through a return pointer into a return area. A guest
that receives an allocated buffer needs an exported `cabi_realloc`, whose
standard signature is `(old_ptr: i32, old_len: i32, align: i32, new_len: i32) ->
i32`.

**wasm32 addressing.** In wasm32 every address is a 32-bit byte offset into one
linear memory, indexed in 64 KiB pages. `memory.size` reports the current page
count and `memory.grow` extends it. A pointer is therefore an `i32`, and address
arithmetic wraps modulo 2^32. `memory64` would make addresses `i64`, but it is
disabled in the profile and cannot be lifted into the current component artifact
([capability profile](capability-profile.md)).

**A byte boundary, not a heap.** Wasm GC manages language objects with typed
references, engine tracing, and no manual addresses. Linear memory is still
required because the component boundary is byte-oriented. The design keeps the
two separate: [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md) makes
GC the only language heap and retires the former linear-memory language-heap
planner.

## Model

There is exactly one memory, `MemoryId(0)`, at `MemoryIndex(0)`. Its address
type is `i32`. The boundary instructions are MIR operations that carry the
memory they address:

```text
Load   { destination: ValueId, address: ValueId, memory: MemoryId,
         offset: u32, span }
Load8U { destination: ValueId, address: ValueId, memory: MemoryId,
         offset: u32, span }
Store  { address: ValueId, value: ValueId, memory: MemoryId,
         offset: u32, span }
WrapI64 { destination: ValueId, value: ValueId, span }
WidenI64 { destination: ValueId, value: ValueId, signed: bool, span }
TrapIf { condition: ValueId, span }
```

A `String` value is an `i32` pointer to a length-prefixed UTF-8 buffer: a 4-byte
little-endian length followed by that many bytes. The Canonical ABI consumes a separate `(pointer, length)` pair; the prefix is
the language's internal representation and is converted at each import call.
The reserved scratch region is `[0, 16)`:
`PRINT_SCRATCH = 0` is the return pointer passed to indirect calls and
`SCRATCH_END = 16` is the first free offset. String data begins after it.

MIR's `Load`/`Load8U`/`Store` read and write only `i32`; `offset` is a statically
known constant added by the leaf `MemArg`, and `Load8U` reads one byte. The
`i64` conversions exist only for canonical ABI scalars, not for source values.

### Invariants

- Every boundary access names `MemoryId(0)`; an address and every loaded or
  stored value are `i32`.
- The scratch region `[0, SCRATCH_END)` is never used by data segments or by
  language data; it holds only canonical return areas.
- Data segments are active at constant `i32` offsets, aligned to 4 bytes, and do
  not overlap each other or the scratch region.
- A nonzero `cabi_realloc` result is aligned to the requested alignment, its
  payload is preceded by a 4-byte length prefix, and reallocation preserves
  `min(old_len, new_len)` bytes. A zero result is never read as a prefix.
- Language arrays and aggregates never use linear memory; their operations are
  GC `array.*`/`struct.*`.

## Design

### One memory, gated only by the profile

`MemoryId(0)` is the only memory and `multi_memory` is disabled, so a memory
instruction cannot address the wrong memory. Carrying the `MemoryId` anyway
keeps the operation's meaning explicit and lets a future profile add memories
without changing CC. The address type is an ABI detail, not a CC concept: CC
references are opaque handles owned by the GC planner, so a future memory64
profile changes `i32` to `i64` only in MIR and the encoder, not in CC
([IR boundaries](../00-ir-boundaries.md)).

### Strings and data segments

String literals are collected once, deduplicated by content, and placed in
active data segments after the scratch region at 4-aligned offsets. Each segment
holds the 4-byte little-endian length followed by the bytes. A `StringConstant`
lowers to an `i32` constant naming the segment address. Because the same
length-prefixed buffer is what the boundary consumes, a string literal can be
passed to a WIT import without copying.

### The `cabi_realloc` bump allocator

When the module imports a function that returns a `list`/`string`, canonical
lowering must be able to allocate in guest memory. P9 fixes the internal buffer
layout and allocator contract; P10 mechanically synthesizes and exports
`cabi_realloc`. It is a bump allocator:

- a free pointer lives in one further active data segment, after the string data;
- the payload pointer is aligned to `max(align, 4)`, with the 4-byte length
  prefix immediately before it;
- when `old_ptr` is nonzero, the old payload is copied up to the smaller size;
- the new byte length is written at the prefix and the free pointer advances;
- the memory is grown with `memory.grow` (MVP, not `bulk_memory`) when the
  aligned end crosses the current page count; and
- the returned pointer points after the prefix; an imported string result is
  converted back to the internal prefix pointer after checking its length.

There is no reclamation in this design. Repeated imported strings can therefore
grow linear memory without bound even though the language heap uses GC. The
artifact must report allocation failure as a trap; reclaiming canonical buffers
requires a later ownership and lifetime design
([canonical ABI and WIT](canonical-abi-and-wit.md)).

### Byte operations at the boundary

P9 lowers canonical ABI adaptation to the boundary instructions above. The MIR
verifier checks their value types and memory identity. Byte-level copies for
return areas and byte lists use MVP loads, stores, and structured branches where
the Canonical ABI requires a copy. Language array updates and clones use GC
`array.set`/`array.copy`, not linear instructions. `WrapI64`/`WidenI64` narrow or
widen a 64-bit WASI scalar, and `TrapIf` rejects a nonzero canonical status
instead of silently succeeding.

### Rejected alternative

- **A linear-memory language heap.** Rejected by
  [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md): its bump
  allocator has no reclamation, so a garbage-collected source language would
  need a hand-written collector (root and stack maps, tracing, compaction) that
  duplicates the engine's GC, and every new CC operation would need a second
  linear realization and a pointer-bounds verifier. The former
  `LinearMemoryPlanner`, its object layouts, its erased boxing path, and the
  language-object pointer-bounds verifier are removed.

## Algorithms

### `cabi_realloc`

```text
realloc(old_ptr, old_len, align, new_len):
    require align is a nonzero power of two
    if new_len == 0: return 0             # free is a no-op
    require old_ptr == 0 implies old_len == 0
    validate old range against current memory and verify its stored prefix
        if old_ptr != 0
    payload = align_up(checked_add(load(heap_pointer), 4), max(align, 4))
    end = checked_add(payload, new_len)
    require end <= 0xffff_ffff       # the next-free pointer must remain representable
    pages = ceil(end / 65536)
    if pages > memory.size() and memory.grow(pages - memory.size()) == -1:
        trap
    if old_ptr != 0: copy min(old_len, new_len) bytes from old_ptr to payload
    store_i32(payload - 4, new_len)
    store(heap_pointer, end)
    return payload
```

The allocator accepts canonical reallocations and preserves the old contents.
Freeing is a no-op, so correctness is maintained at the cost of retained memory.

### Passing a string argument

```text
lower_string_argument(string):
    length = Load [0] string          # length prefix
    bytes  = string + 4
    push (bytes, length)              # canonical (pointer, length)
```

### Reading a returned string

```text
read_returned_string(retptr):
    pointer = Load [0] retptr         # returned payload pointer
    length  = Load [4] retptr
    if length == 0 and pointer == 0: return static_empty_string
    require pointer >= 4             # canonical lowering validates payload extent
    require Load [0] (pointer - 4) == length
    value   = pointer - 4             # internal prefix pointer
```

A returned list whose element type is not a byte is rejected before MIR, because
the `String` boundary cannot represent it.

### Edge cases

- A string literal and a returned buffer use the same length-prefixed layout, so
  a literal can be passed through unchanged.
- A zero-size realloc returns `0`; an imported empty string with a null payload
  pointer maps to the static empty-string buffer.
- `memory.grow` returns `-1` on failure; the allocator tests and traps on it.
- `Load8U` is used for a one-byte canonical result discriminant; a nonzero
  discriminant traps.

## Code map

The boundary must be implemented by the following module tree. Each module owns
one part of the contract; no other module may synthesize boundary byte
operations, plan data segments, or emit the allocator.

```text
crates/psrs-backend/src/
  abi.rs
  mir/
    instruction.rs
    verify/instruction/memory.rs
    wit/mod.rs
    wit/parameters.rs
  wasm/
    lower/mod.rs
    lower/realloc.rs
    lower/runtime.rs
    lower/structure/instructions.rs
```

Responsibilities and required entry points:

- `abi.rs` owns the fixed boundary constants. It must define `MemoryId(0)`, the
  scratch constants `PRINT_SCRATCH`, `SCRATCH_SIZE`, and `SCRATCH_END`, and the
  string/byte-list layout constants; no other module may redefine them.
- `mir/instruction.rs` must define the boundary operations as first-class MIR
  instructions: `Load`, `Load8U`, `Store`, `WrapI64`, `WidenI64`, and `TrapIf`,
  each carrying its `MemoryId`, `ValueId`s, `offset`, and `span`. It must not
  carry a language-object address, a GC type, or a dictionary/effect field.
- `mir/verify/instruction/memory.rs` must verify that every `Load`/`Load8U`/
  `Store` names the single memory, that its address and loaded/stored value are
  `i32`, and that `WrapI64`/`WidenI64`/`TrapIf` match their operand and result
  types. Required entry point:
  `fn verify_memory(instruction, types) -> Result<(), Diagnostic>`.
- `wasm/lower/runtime.rs` must collect and deduplicate string literals, lay them
  out as active 4-aligned data segments after the scratch region, and own the
  heap-pointer segment. Required entry point:
  `fn plan_data_segments(strings, layout) -> DataSegments`.
- `wasm/lower/realloc.rs` must synthesize the `cabi_realloc` bump allocator with
  the standard `(old_ptr, old_len, align, new_len) -> i32` signature and export
  it only when the module imports a function that returns a string or byte list.
  Required entry point: `fn synthesize_realloc(layout) -> Function`.
- `wasm/lower/mod.rs` must assemble the memory (minimum pages), the data
  segments, and the allocator export into the thin Wasm module.
- `wasm/lower/structure/instructions.rs` must lower the MIR boundary
  instructions to leaf Wasm load/store/convert instructions carrying a `MemArg`.
- `mir/wit/mod.rs` and `mir/wit/parameters.rs` must adapt strings and byte lists
  to and from the `(pointer, length)` exchange format; a non-byte list must be
  rejected before this layer.

**No language objects in linear memory.** The module tree must not allocate
language aggregates, closures, variants, arrays, or erased values in linear
memory. GC objects are referenced only by opaque GC handles; the only values
that cross this boundary are `i32` addresses, lengths, and canonical scalars.
Any module needing a language-heap operation must depend on the GC lowering path
([IR boundaries](../00-ir-boundaries.md)), not on these modules.

## Invariants and verification

The MIR memory verifier checks that each `Load`/`Load8U`/`Store` names
`MemoryId(0)`, that the address is `i32`, and that the loaded, stored, or
converted value has the required `i32`/`i64` type; `WrapI64` and `WidenI64` are
checked for the matching `i64`/`i32` operand and result. The thin-IR verifier and
the WebAssembly validator then check the emitted leaf instructions and the
allocator body ([Wasm encoding](encoding-and-structuring.md)). The verifier does
not currently validate static access extents; offset validation remains deferred
([MIR](../fp/mir.md) open questions). Execution tests run returned buffers
through the ordinary `writeStdout` import and exercise repeated allocator calls.

## Worked example

Suppose string data ends at offset `40` and the heap-pointer segment is
initialized to `40`. An imported function returns a 5-byte list; the lowering
calls `cabi_realloc(0, 0, 1, 5)`:

```text
free    = 40
aligned = ((40 + 4 + 5 - 1) & -1) - 4 = 44
end     = 44 + 5 + 4 = 53
pages   = ceil(53 / 65536) = 1  (no grow)
store [44] = 5
store [heap_pointer] = 53
return 48
```

The host writes the 5 bytes at `[48, 53)`. The return area at address `0` holds
`(48, 5)`, and the lowering computes `48 - 4 = 44` as the `String` value. A
subsequent `writeStdout` reads the length from `[44]` and the bytes from `[48]`,
exactly as it would for a string literal.

## Boundaries and interfaces

- **Input:** MIR byte operations and canonical ABI adaptation produced by P9;
  string literals from MIR.
- **Output:** the memory, its data segments, and (conditionally) the
  `cabi_realloc` export, all carried in the thin Wasm module.
- **To the Canonical ABI layer:** the string and byte-list representation and the
  `pointer - 4` convention.
- **To GC lowering:** no interaction; language objects never cross into linear
  memory ([DEC-09](../../../decision/DEC-09-gc-only-language-heap.md)).

## Open questions and future work

- **Reclamation.** A reclaiming allocator and post-return release for returned
  lists and owned resources.
- **Memory layout.** Canonical memory layout for indirect records, tuples, and
  variants beside the current scalar/byte shapes.
- **Memory64.** Revisited only when the component toolchain and WASI host support
  it ([capability profile](capability-profile.md)).
- **Extent verification.** Static access-extent checking at the boundary.

## Implementation notes

The synthesized `cabi_realloc` validates power-of-two alignment, checks each
wasm32 address addition before committing allocator state, traps when
`memory.grow` fails, verifies the old range against the pre-growth memory and
checks its length prefix, and copies the preserved bytes with MVP byte loads and
stores. Execution coverage checks alignment, growth and shrink reallocation,
zero-sized frees, invalid alignment, address overflow, old-range bounds, and
growth failure. Because the allocator stores its next
free byte as an `i32`, it traps if an allocation's exclusive end would be
`2^32`; the final byte of the wasm32 address space is consequently unavailable
to allocator payloads. There is still no reclamation, and the MIR verifier does
not statically check memory access extents; these remain coverage gaps rather
than changes to the boundary design.

## References

- WebAssembly 3.0 specification: memories, `memory.size`, `memory.grow`, load
  and store instructions, data segments.
- [WebAssembly Component Model Canonical ABI](https://github.com/WebAssembly/component-model/blob/main/design/mvp/Explainer.md#canonical-abi):
  `realloc`, return pointer, list and string passing.
- [DEC-09 — GC-Only Language Heap](../../../decision/DEC-09-gc-only-language-heap.md),
  [DEC-05 — Target wasmtime's WebAssembly Feature Set](../../../decision/DEC-05-wasmtime-feature-set.md).
- [canonical ABI and WIT](canonical-abi-and-wit.md),
  [capability profile](capability-profile.md),
  [data representation](../fp/data-representation.md).
