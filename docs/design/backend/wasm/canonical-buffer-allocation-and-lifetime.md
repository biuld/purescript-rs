# Canonical Buffer Allocation and Lifetime

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Draft  
**Prerequisites:** the Canonical ABI `realloc` contract (allocate, free, resize), WebAssembly linear memory and `memory.grow`, Wasm GC as the language heap with no finalizers. Read [linear memory and the canonical ABI boundary](linear-memory-and-canonical-abi-boundary.md), [canonical ABI and WIT](canonical-abi-and-wit.md), and [DEC-10](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md) first.  
**Summary:** Linear memory carries only transient canonical ABI buffers, so those buffers must be reclaimed by construction. This document fixes the complete `cabi_realloc` allocator — allocation, free, in-place resize, alignment, size headers, free lists, coalescing, growth and overflow traps — the heap-state region, the four buffer ownership classes, export `post-return` synthesis, and the `cabi_realloc` provenance rule for dynamic stores.

## Scope

This document owns the lifetime and storage of every canonical ABI buffer: the
`cabi_realloc` allocator contract and its implementation strategy, block headers
and alignment, free lists, coalescing and reuse, the heap-state region, memory
growth and overflow trapping, the four buffer ownership classes (static,
call-local, import-result, export-result), export `post-return` synthesis, and
the allocator-provenance rule that admits a dynamic MIR store.

It does not own:

- the address model, pointer width, byte operations, data segments, and the
  static access-extent algorithm — those stay in
  [linear memory and the canonical ABI boundary](linear-memory-and-canonical-abi-boundary.md);
- the canonical ABI adaptation itself (flattening, result recovery, signature
  validation) — [canonical ABI and WIT](canonical-abi-and-wit.md);
- `own`/`borrow` handle representation and classification — also
  [canonical ABI and WIT](canonical-abi-and-wit.md); this document owns only the
  handle drop/borrow-release timing at buffer-owning boundaries;
- the structured encoding of the allocator body — [Wasm encoding](encoding-and-structuring.md);
- the choice of GC as the language heap — [capability profile](capability-profile.md)
  and [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md).

## Background

**The Canonical ABI exchange is byte-oriented.** A component and its host pass
aggregate values through linear memory. A guest that receives an allocated
buffer must export `cabi_realloc` with the standard signature
`(old_ptr: i32, old_len: i32, align: i32, new_len: i32) -> i32`, so the host can
allocate inside guest memory. Lists and strings cross as `(pointer, length)`
pairs; a result that does not fit in one core value is written through a return
pointer into a return area; a `post-return` action releases whatever the guest
allocated for an export result.

**Wasm GC has no finalizers.** [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md)
made Wasm GC the only language heap. The engine reclaims language objects, but
it does not trace linear memory. A language string or byte list held in a linear
buffer therefore cannot be freed automatically when its last GC reference dies.
[DEC-10](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md) resolves
this by keeping every language value on the GC heap and admitting linear memory
only for buffers whose lifetime is bounded by a canonical call or a
`post-return`. That bounding is what this document implements: if every buffer
has exactly one owner and is freed when the owner's lifetime ends, linear
memory is reclaimed without a collector, root map, or finalizer.

**A bump allocator cannot satisfy the model.** A bump allocator never reuses or
frees a block, so its growth is bounded by allocation history, not call
frequency. It is an MVP artifact. The complete design needs a general aligned
allocator that implements the full canonical `realloc` contract.

**Linear memory holds unmanaged bytes.** A GC `struct`/`array` lives on the GC
heap and is reached through a typed reference; it cannot be addressed by an
`i32` pointer, and a linear address cannot name a GC object. Bytes in linear
memory are therefore raw, untraced, and never finalized. This is why every
buffer has an explicit owner and an explicit free rather than relying on
collection, and why a GC reference may never be stored in a linear buffer.
Toolchains whose language heap already lives in linear memory may instead
*adopt* a lifted buffer (using the same allocator) rather than copy it; a
GC-native language cannot, so it copies a received value into the GC heap and
frees the linear buffer.

**Two addresses, one memory.** The canonical ABI allocator and the language
heap are different storage. The ABI memory is a profile parameter: wasm32 uses
`i32` addresses and one memory, memory64 uses `i64`, and a profile that enables
`multi_memory` may name more than one ABI memory ([capability profile](capability-profile.md)).
CC references stay opaque, so pointer width and memory count change only MIR
and the encoder.

## Model

### Buffer ownership classes

Every canonical buffer has exactly one owner, and its lifetime ends when the
owner's obligation is discharged:

| Class | Where it comes from | Owner | Freed by |
| --- | --- | --- | --- |
| *static* | passive literal data segments | the program | never; program lifetime, read-only |
| *call-local* | indirect parameter records and import return areas | the calling function | the function when the canonical call returns |
| *import result* | host-written through the guest `allocator` | the guest | the ABI layer, after copying the bytes into a GC value |
| *export result* | guest-allocated through `cabi_realloc` for a guest export | the host | the export's synthesized `post-return` |

A `borrow<T>` is a call-scoped non-owning handle and is released when the call
returns; an `own<T>` handle carries a drop obligation discharged either when the
owning value is consumed or in the owning export's `post-return`
([canonical ABI and WIT](canonical-abi-and-wit.md)).

### Who allocates and who frees

Only guest code can allocate in or free guest memory, so `cabi_realloc` is
always the allocator; the canonical ABI fixes *which side triggers* each
operation. For a function whose parameters or result contain a list or string:

| Direction | Allocates | Frees | Trigger |
| --- | --- | --- | --- |
| guest calls an import, list/string argument | the guest lowering | the guest lowering | emitted after the call returns |
| an import returns a list/string | the host (it calls the guest `cabi_realloc`) | the guest lowering | emitted after the bytes are copied into a GC value |
| the host calls a guest export, list/string argument | the host (it calls the guest `cabi_realloc`) | the host | after the export returns |
| a guest export returns a list/string | the guest lowering | the guest's `cabi_post_<name>` | the host calls `post-return` after lifting |

The guest never trusts the host to free a guest-owned buffer: the compiler
inserts each free at the point the canonical ABI assigns ownership, on the same
control-flow path as the call. A trap aborts the instance, so an interrupted
call does not leave a reclaimable allocation behind.

### Statically known ABI regions

The ABI memory is partitioned at compile time:

```text
[0, SCRATCH_END)          scratch return areas; reserved, allocator-owned
[HEAP_STATE, HEAP_START)  allocator bookkeeping: free-list head and heap break
[HEAP_START, memory end)  allocator blocks, grown on demand with memory.grow
```

`PRINT_SCRATCH = 0` is the return pointer passed to indirect calls;
`SCRATCH_END = 16` is the first free offset. `HEAP_STATE = SCRATCH_END`, and
`HEAP_START` is the first address the allocator may hand out, rounded up from
the end of the heap-state words. Passive literal segments are not
MIR-addressable: they are read by `array.new_data`, never by a load or store,
so they occupy no address interval in this model.

### The allocator interface

`cabi_realloc` is exported with the canonical signature and the following
contract, where `align` is a nonzero power of two:

```text
realloc(old_ptr, old_len, align, new_len) -> pointer
    new_len == 0            free(old_ptr); return 0
    old_ptr == 0            allocate(new_len, align)
    otherwise               resize the block at old_ptr to new_len
```

Block layout (all words are pointer-width; wasm32 shown):

```text
H + 0   block_size     total bytes of this block from H, a multiple of MIN_BLOCK
H + 4   next_free      free-list link when the block is free, 0 otherwise
H + 4   payload_len    live payload byte length when the block is allocated
P = H + HEADER         returned payload pointer
```

`P - 4` is the 4-byte length prefix that byte buffers use; because
`HEADER = 8`, it aliases the `payload_len` word. A fresh block's payload is
aligned to the requested `align`, and block sizes are multiples of `MIN_BLOCK`,
so blocks tile the allocator region exactly and adjacent blocks never overlap.
`MIN_BLOCK = HEADER = 8`, which matches the maximum canonical ABI field
alignment (`i64`/`f64`). The public contract requires the returned pointer to be
`align`-aligned; the allocator aligns the payload of every fresh or reused block
to `align`.

### Invariants

- `cabi_realloc` returns a pointer aligned to `align`, or traps; it never
  returns a pointer into the scratch region, the heap-state region, or a static
  segment.
- A live block's `payload_len` word equals the `old_len` passed to a later
  resize or free of that block, and `P - 4` is that word.
- A free block appears at most once on the free list; the list is address
  ordered, and no two free-list blocks are adjacent (adjacency is coalesced on
  free).
- `allocator region + free blocks + live blocks` partition `[HEAP_START,
  heap_break)` exactly; `heap_break` never moves backward.
- Every dynamic MIR store is proven to target a pointer returned by
  `cabi_realloc`; other dynamic stores are rejected.
- Every canonical buffer is freed by its owner: call-local buffers at call
  return, import results after their bytes are copied into a GC value, and
  export results in the export's `post-return`.
- The allocator body makes no GC operation and holds no GC reference; it is
  ordinary core Wasm.

## Design

### Header, alignment, and sizing

A block is `HEADER` bytes of metadata followed by payload. `block_size` counts
the whole block and is stored in its first word so the allocator can step to the
next block and coalesce without a separate footer. The second word is
overloaded: it is the payload length while the block is live and the free-list
link while it is free. The 4-byte length prefix that byte buffers require
sits immediately before the payload, so `P - 4` and the `payload_len` word are
the same location; the string codec writes the true byte length there after
allocation, and a later free or resize checks it against `old_len`.

Sizing is expressed in `MIN_BLOCK` units:

```text
required(n, align) = align_up(HEADER + align_up(n, MIN_BLOCK), MIN_BLOCK)
```

Aligning the payload rather than the header keeps `P` correct for every
canonical field alignment while keeping block sizes granular enough to tile the
region. Because `align` is at most the maximum canonical field alignment for
the ABI, `HEADER = MIN_BLOCK = 8` is sufficient; a profile with wider canonical
fields raises `HEADER`/`MIN_BLOCK` together and needs no other change.

### Free lists, coalescing, and reuse

The allocator keeps one address-ordered free list. Its head lives in the first
heap-state word. `allocate` does a first-fit search: it walks the list and takes
the first block whose `block_size` can hold `required`. If the remainder is at
least `MIN_BLOCK`, the block is split — the front part becomes the allocation
and the tail stays free; otherwise the whole block is used. This reuses freed
storage and bounds fragmentation.

`free` returns a block to the list and coalesces eagerly:

- walk the address-ordered list to the position where the freed block belongs;
- merge with the following free block when `H + block_size == successor`;
- merge with the preceding free block when `predecessor + predecessor.block_size == H`.

Coalescing on both sides requires an address-ordered list; it is what keeps
large free ranges available after many small frees. The final block also carries
a boundary at `heap_break`, so the "next block" terminator is `heap_break`, not a
null pointer; address `0` remains a safe null link because it is inside the
scratch region.

### Growth and overflow trapping

`allocate` first consults the free list. When no block fits, it takes storage
from the bump end: it reads `heap_break` from the second heap-state word, checks
that a `required`-sized block fits, and otherwise grows memory. Growth rounds
the new end up to a 64 KiB page, computes the pages to add without adding
`65535` (which could itself overflow), calls `memory.grow`, and traps when it
returns `-1`. Before any pointer is returned or any allocator state is
committed, every address computation is checked for wasm32 wraparound:
`P - HEADER`, `P + payload`, the block end, and the page-rounded end must not
wrap below their operands. An overflow traps rather than producing a
low or aliased pointer. After growth, the bump end advances to
`align_up(block end, MIN_BLOCK)` and the new `heap_break` is stored.

### `post-return` synthesis

A guest export that returns a non-scalar is lifted through the canonical ABI in
reverse: the guest writes the value into linear memory through `cabi_realloc`,
returns the return-area pointer, and the host reads and copies the value. The
compiler then synthesizes `cabi_post_<name>` for that export. It frees the
export's return area and every buffer the export allocated for the result, in
the reverse order they were created. `wasi:cli/run` returns no aggregate, so the
current only export needs no `post-return`; the design requires one wherever the
export result shape does. A `post-return` also releases any `own<T>` handles the
result owns.

### Dynamic-store provenance

The static extent pass rejects a MIR store whose address is not statically
known unless the address is proven to come from `cabi_realloc`
([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)). The
proof is a value fact: a call to the reserved `cabi_realloc` symbol with a
statically known `new_len` produces an `Allocated(size)` fact, and a memory
access through that value is admitted when its fixed `offset + width` fits
`size`. Indirect parameter records are the one current producer: they are
allocated through `cabi_realloc` and stored into before the call. A future
dynamic store that is not allocator-derived is rejected rather than relying on
the WebAssembly runtime bounds check, which cannot protect allocator metadata or
static regions.

### Rejected alternatives

- **Keep the bump allocator and never free.** Rejected: growth would be bounded
  by allocation history, not call frequency, and a long-running program that
  calls a list-returning import would leak linear memory without bound
  ([DEC-10](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md)).
- **A hand-written GC collector for linear memory.** Rejected: it duplicates
  the engine's collector and reintroduces root and stack maps
  ([DEC-09](../../../decision/DEC-09-gc-only-language-heap.md)).
- **One free list with no coalescing.** Rejected: alternating allocations and
  frees fragment the region until no single block fits, even when the total free
  bytes suffice; coalescing on free is required for bounded reuse.
- **Boundary-tagged blocks with a footer and constant-time coalescing.**
  Deferred: a footer costs a word per block and is an optimization over the
  address-ordered-list design, which is correct and simple to synthesize.
- **Rely on the runtime bounds check for dynamic stores.** Rejected: it proves
  only that an access is inside memory, not that it is inside a writable
  allocation, so it cannot protect allocator metadata or static regions.

## Algorithms

### `cabi_realloc`

```text
realloc(old_ptr, old_len, align, new_len):
    require align is a nonzero power of two
    if new_len == 0:
        if old_ptr != 0: free(old_ptr)
        return 0
    require old_ptr == 0 implies old_len == 0
    if old_ptr == 0:
        return allocate(new_len, align)
    require old_ptr >= HEADER and the block at old_ptr is complete
    if old_len != payload_len(old_ptr): trap
    if payload_aligned(old_ptr, align) and capacity(old_ptr) >= new_len:
        # grow or shrink in place, splitting off a large tail when shrinking
        set payload_len(old_ptr, new_len)
        return old_ptr
    block = allocate(new_len, align)
    copy min(old_len, new_len) bytes from old_ptr to block
    free(old_ptr)
    set payload_len(block, new_len)
    return block
```

### `allocate`

```text
allocate(n, align):
    need = align_up(HEADER + align_up(n, MIN_BLOCK), MIN_BLOCK)
    walk the free list:
        if block.capacity >= need:
            if block.capacity - need >= MIN_BLOCK:
                split: front becomes the allocation, tail stays free
            remove the block from the list
            store block_size and payload_len
            return payload(block)
    # bump end
    start = heap_break
    end = checked_add(start, need); trap on overflow
    pages = page_round_up(end); trap on overflow
    if pages > memory.size:
        if memory.grow(pages - memory.size) == -1: trap
    store block_size = need and payload_len = n
    heap_break = end
    return payload(start)
```

### `free` and coalescing

```text
free(ptr):
    h = ptr - HEADER
    size = block_size(h)
    # find the address-ordered insertion point
    prev = 0; node = free_head
    while node != 0 and node < h:
        prev = node; node = next(node)
    # merge with the following free block
    if node != 0 and h + size == node:
        size += block_size(node)
        node = next(node)
    # merge with the preceding free block, or insert h before node
    if prev != 0 and prev + block_size(prev) == h:
        block_size(prev) += size
        next(prev) = node
    else:
        block_size(h) = size
        next(h) = node
        if prev == 0: free_head = h
        else: next(prev) = h
```

### Freeing the four classes

```text
lower_string_argument(string):
    buffer = string_to_bytes(string)          # cabi_realloc + UTF-8 transcode
    (pointer, length) = (buffer + 4, load(buffer))
    push (pointer, length)
    # call-local: free(buffer + 4, length, 1, 0) after the call returns

read_returned_string(retptr):
    pointer = Load [retptr + 0]; length = Load [retptr + 4]
    value = bytes_to_string(pointer, length)  # fresh GC string
    cabi_realloc(pointer, length, 1, 0)       # import result freed
    return value

free_parameter_record(address):
    cabi_realloc(address, record_size, record_align, 0)
```

## Code map

```text
crates/psrs-backend/src/
  abi.rs / abi/mod.rs        SCRATCH, HEAP_STATE/HEAP_START constants,
                             REALLOC_SYMBOL, ownership helpers
  mir/
    wit/mod.rs               result recovery: free the import buffer
    wit/parameters/mod.rs    string argument: free the transcode buffer
    wit/parameters/indirect.rs  parameter record: allocate and free around the call
  wasm/
    lower/mod.rs             heap-state data segment, memory minimum,
                             cabi_realloc export, cabi_post_<name> synthesis
    lower/realloc/mod.rs     synthesize_realloc: the aligned allocator
    lower/extent/mod.rs      scratch + heap-state regions
    lower/extent/access.rs   dynamic-store allocator provenance
    lower/extent/address.rs  Allocated(size) fact from a cabi_realloc call
    lower/runtime.rs         passive literal segments (static class)
```

Responsibilities and required entry points:

- `abi.rs` owns the allocator's address constants. It must define
  `PRINT_SCRATCH`, `SCRATCH_SIZE`, `SCRATCH_END`, `HEAP_STATE`, `HEAP_START`,
  `HEADER_SIZE`, and `MIN_BLOCK`, and the reserved `REALLOC_SYMBOL`; no other
  module may redefine them.
- `wasm/lower/realloc.rs` (a module directory may replace it) must synthesize
  the general `cabi_realloc` with the standard signature and export it exactly
  when the module crosses the canonical ABI. Required entry point:
  `fn synthesize_realloc(layout) -> Function`.
- `mir/wit/mod.rs` and `mir/wit/parameters.rs` must free each call-local and
  import-result buffer at the documented point; they must not free a static
  buffer or one owned by a different class.
- `wasm/lower/mod.rs` must place the heap-state words, set the memory minimum
  to cover the initial heap, and synthesize `cabi_post_<name>` for every export
  whose lift needs linear memory.
- `wasm/lower/extent.rs` must resolve the scratch and heap-state regions and
  require `cabi_realloc` provenance for a dynamic store; a store through an
  unproven dynamic pointer is rejected.

## Invariants and verification

The allocator's contract is checked three ways. First, allocator unit tests run
the generated `cabi_realloc` under Wasmtime and exercise allocation, free,
address-ordered reuse, coalescing, alignment (including a misaligned-request
sequence), in-place resize with copied old bytes, growth, and every trap path
(bad alignment, null pointer with a length, mismatched old length, address
overflow, and `memory.grow` failure). Second, an execution test repeatedly
crosses the boundary (for example returns a string from an import in a loop)
and asserts that linear memory stops growing once the live set stabilizes,
which is the observable consequence of reclamation. Third, the thin-IR verifier
and WebAssembly validator check the emitted allocator body and memory.

The static extent pass checks that a statically known address lies inside the
scratch or heap-state region and that its whole effective interval stays there,
and that a dynamic store's address is allocator-proven. The MIR memory verifier
checks value types and memory identity. No language value is stored in linear
memory between calls, so no linear buffer outlives its owner.

## Worked example

An import returns a 5-byte string; `cabi_realloc(0, 0, 1, 5)` finds no free
block, takes a bump-end block of `align_up(8 + 8, 8) = 16` bytes at `HEAP_START`,
stores `block_size = 16` at `H` and `payload_len = 5` at `H + 4`, and returns
`P = H + 8`, say `32`. The return area holds `(32, 5)`; the host writes the five
bytes. The lowering copies them into a fresh GC string and calls
`cabi_realloc(32, 5, 1, 0)`: the block is marked free and, when the free list is
empty, becomes its head. A later `cabi_realloc(0, 0, 1, 5)` takes that same
block without growing memory, which is exactly the reuse the bump allocator
could not perform. If the next request needs only part of a larger freed block,
the block is split when at least `MIN_BLOCK` bytes remain after the request;
otherwise the whole block is handed out.

## Boundaries and interfaces

- **Input:** the canonical ABI adaptation's allocation calls, the result-recovery
  path's host buffers, and export-result buffers.
- **Output:** the exported `cabi_realloc`, the heap-state data segment, the
  memory minimum, and any `cabi_post_<name>` exports.
- **To the byte-operations layer:** the block and length-prefix conventions this
  document fixes; the byte ops themselves stay in
  [linear memory and the canonical ABI boundary](linear-memory-and-canonical-abi-boundary.md).
- **To canonical ABI and WIT lowering:** which buffer class each allocation is,
  and the point at which it is freed; handle representation and classification
  stay there, but handle drop/borrow-release timing follows this document's
  ownership classes.
- **To the encoder:** the allocator and `post-return` functions are ordinary
  core functions in the thin Wasm IR.

## Open questions and future work

- **Footer-based coalescing.** A boundary tag would make coalescing constant
  time at the cost of a word per block; the address-ordered list is the chosen
  simple, correct baseline.
- **Caching a linear view.** A repeated boundary call may cache a linear view
  of a GC string to avoid re-transcoding; the cache would carry its own lifetime
  rules ([DEC-10](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md)).
- **Alignments above the canonical maximum.** A profile with wider canonical
  fields raises `HEADER`/`MIN_BLOCK`; the model already parameterizes on them.
- **GC canonical ABI.** The Component Model has an in-flight pre-proposal to
  lower component types directly to core Wasm GC types (a `gc` canonical option
  plus a `core-type`), which would pass strings and lists as typed references
  and remove linear memory, `cabi_realloc`, and `post-return` from the string
  path. That direction is compatible with this project's GC string
  representation; see [canonical ABI and WIT](canonical-abi-and-wit.md#gc-canonical-abi).
- **Threaded allocators.** The design is single-threaded; a shared-memory
  profile would need synchronization, which the capability matrix keeps out of
  scope ([D-04](../../D-04-suite-roadmap.md)).

## Implementation notes

The allocator and buffer-free path match this design. The remaining deviations
are coverage, not choices:

- `cabi_post_<name>` is not synthesized for a list or other non-scalar buffer
  result, because `wasi:cli/run` returns no aggregate and no list-returning
  export exists. An export whose result is `own<T>` does get `cabi_post_<name>`,
  and that function calls `resource.drop` on the returned handle.
- Handle drop and borrow release are lowered by the canonical ABI adapter.
  An owned handle returned as `Int` from a non-export function is not tracked
  in the caller.

The synthesized allocator body lives in
`wasm/lower/realloc/` and shares the `wasm/lower/asm.rs` structured-instruction
builder with the string codec. Coverage is tracked by ALC-01..ALC-07 in the
[implementation checklist](../../implementation/backend/canonical-buffer-allocation.md).

## References

- [WebAssembly Component Model Canonical ABI](https://github.com/WebAssembly/component-model/blob/main/design/mvp/Explainer.md#canonical-abi):
  `realloc`, the return pointer, and `post-return`.
- WebAssembly 3.0 specification: linear memory, `memory.size`, `memory.grow`,
  load and store instructions.
- [DEC-09 — GC-Only Language Heap](../../../decision/DEC-09-gc-only-language-heap.md),
  [DEC-10 — Canonical ABI Buffer Ownership and Lifetime](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md).
- [linear memory and the canonical ABI boundary](linear-memory-and-canonical-abi-boundary.md),
  [canonical ABI and WIT](canonical-abi-and-wit.md),
  [capability profile](capability-profile.md).
