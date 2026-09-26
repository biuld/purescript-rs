# Linear Memory and the Canonical ABI Boundary

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Stable (design)  
**Prerequisites:** the Canonical ABI exchange format (flattened values, the return pointer, `realloc`), WebAssembly linear memory and the wasm32 address model, and Wasm GC as the language heap. Read [canonical ABI and WIT](canonical-abi-and-wit.md), [MIR](../fp/mir.md), and [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md) first.  
**Summary:** Linear memory is retained only as the byte-oriented Canonical ABI and WASI boundary: GC strings and byte lists transiently linearized, the return area of canonical calls, and passive data segments. It is not a general object heap. Language strings, aggregates, closures, variants, arrays, and erased values use Wasm GC. The allocator that backs transient buffers and their lifetime are owned by [canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md).

## Scope

This document owns the address model, the string and byte-list representation,
the scratch return area, data-segment use, and the MIR byte-operation contract at
the boundary. It does not own canonical ABI adaptation itself
([canonical ABI and WIT](canonical-abi-and-wit.md)), the `cabi_realloc`
allocator and buffer ownership/lifetime
([canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md)),
the structured encoding of memory instructions
([Wasm encoding](encoding-and-structuring.md)), the choice of GC as the heap
([capability profile](capability-profile.md)), or the exact GC layouts
([data representation](../fp/data-representation.md)).

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

The canonical ABI adapter selects the ABI memory (or memories) from the target
profile. A wasm32 profile uses `i32` addresses and one ABI memory,
`MemoryId(0)` at `MemoryIndex(0)`; a memory64 profile uses `i64` addresses; and
a profile that enables `multi_memory` may name more than one ABI memory. CC
references stay opaque, so the pointer width and memory count are MIR and
encoder parameters, not CC concepts. The boundary instructions carry the
memory and offset they address:

```text
Load   { destination: ValueId, address: ValueId, memory: MemoryId,
         offset, span }
Load8U { destination: ValueId, address: ValueId, memory: MemoryId,
         offset, span }
Store  { address: ValueId, value: ValueId, memory: MemoryId,
         offset, span }
WrapI64 { destination: ValueId, value: ValueId, span }
WidenI64 { destination: ValueId, value: ValueId, signed: bool, span }
TrapIf { condition: ValueId, span }
```

`address` and `value` take the profile's pointer/value types (`i32`/`i64`);
`offset` is a constant of the pointer width.

A source `String` is a **GC byte-sequence value**, not a linear pointer, and
byte lists and every other source value are GC-managed
([DEC-10](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md)). Linear
memory exists only to carry canonical ABI bytes:

- passive read-only data segments for static literal bytes, materialized into
  GC strings with `array.new_data`;
- transient call buffers (indirect parameter records and import return areas);
  and
- export return areas.

Every transient buffer has exactly one owner and is freed when its lifetime
ends; no language value is stored in linear memory between calls. The reserved
scratch region is `[0, SCRATCH_END)` and holds canonical return areas;
`PRINT_SCRATCH = 0` is the return pointer passed to indirect calls and
`SCRATCH_END = 16` is the first free offset. The allocator's free lists, free
pointer, and buffer ownership rules live in
[canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md).

MIR's `Load`/`Load8U`/`Store` read and write pointer-width values; `offset` is a
statically known constant added by the leaf `MemArg`, and `Load8U` reads one
byte. The `i64` conversions exist only for canonical ABI scalars, not for source
values.

### Invariants

- Every boundary access names a memory selected by the target profile; an
  address is the profile pointer type (`i32` wasm32 / `i64` memory64), and a
  loaded or stored value has the width of its ABI field.
- `Load`/`Store` access the field width and `Load8U` accesses one byte. Static
  access intervals use checked, unsigned pointer-width arithmetic and never wrap
  into low memory.
- The scratch region `[0, SCRATCH_END)` and the heap-state region are owned by
  the canonical allocator; no language value is stored there.
- Passive data segments hold static literal bytes only; MIR never addresses
  them. A distinct literal becomes a GC string once with `array.new_data` and is
  interned in a lazily initialized mutable global shared by every use.
- An access whose address is statically known must fit wholly inside the
  scratch region, the heap-state region, or a proven allocation. Accesses with
  dynamic addresses rely on WebAssembly's runtime bounds check and trap when out
  of bounds.
- `cabi_realloc` returns a pointer aligned to the requested alignment and never
  overlaps a static region. Its alloc/free/resize contract and block layout are
  owned by [canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md).
- Every canonical buffer is freed by its owner: call buffers when the canonical
  call returns, import results after the bytes are copied into a GC value, and
  export results in the export's `post-return`
  ([canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md)).
- Language strings, arrays, and aggregates never use linear memory between
  calls; their operations are GC `array.*`/`struct.*`.

## Design

### Memory and pointer width

The target profile chooses the pointer width and the ABI memory. The stable
profile uses wasm32 (`i32` addresses) and one memory with `multi_memory`
disabled, so a memory instruction cannot address the wrong memory. Carrying the
`MemoryId` keeps the meaning explicit and lets a memory64 or multi-memory
profile change only MIR and the encoder: a memory64 profile widens addresses and
the allocator's words to `i64`
([canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md)).
CC references are opaque handles, so CC is unaffected
([IR boundaries](../00-ir-boundaries.md)).

### Strings as GC values and static literals

A source `String` is a GC byte-sequence value. String literals are encoded once
as passive read-only data segments, deduplicated by content, and each distinct
literal is materialized into a GC string exactly once with `array.new_data` and
interned in a lazily initialized mutable module global that every use reads; a
literal is never addressed by MIR and needs no linear buffer. Passing a string
to an import copies its bytes into a transient linear buffer allocated by
`cabi_realloc`, passes `(pointer, length)`, and frees the buffer when the call
returns. Reading a returned string copies the host-written bytes into a fresh GC
string and then frees the linear buffer.

### The canonical allocator and buffer lifetime

`cabi_realloc` is a general aligned allocator, not a bump allocator, and every
canonical buffer has exactly one owner and is freed when its lifetime ends:
*static* buffers live for the program, *call-local* buffers are owned by the
calling function, *import results* are owned by the guest and freed after their
bytes are copied into a GC value, and *export results* are freed by the export's
synthesized `post-return`. The allocator's full contract, its block layout and
free lists, the heap-state region, growth and overflow trapping, `post-return`
synthesis, and the four ownership classes are owned by
[canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md).
P9 emits the allocator calls; P10 mechanically synthesizes and exports
`cabi_realloc`.

### Byte operations at the boundary

P9 lowers canonical ABI adaptation to the boundary instructions above. The MIR
verifier checks their value types and memory identity. Byte-level copies for
return areas and byte lists use MVP loads, stores, and structured branches where
the Canonical ABI requires a copy. Language array updates and clones use GC
`array.set`/`array.copy`, not linear instructions. `WrapI64`/`WidenI64` narrow or
widen a 64-bit WASI scalar, and `TrapIf` rejects a nonzero canonical status
instead of silently succeeding.

### Static access extents

The access width is 4 bytes for `Load` and `Store`, and 1 byte for `Load8U`.
For an address value with unsigned wasm32 value `a`, the instruction's `u32`
immediate offset `o`, and access width `w`, its half-open effective interval is
`[a + o, a + o + w)`. The verifier calculates this in a wider checked integer;
it must not apply `i32` wrapping to the effective address. A statically known
interval must end at or before `2^32`, the end of the wasm32 address space.
The separate WebAssembly runtime check verifies that the interval also fits in
the memory's current byte length, which may be less than `2^32`.

For an address the verifier cannot resolve statically, `o + w` must still be
at most `2^32`. A larger value makes the access trap for every possible
wasm32 base and is rejected as an invalid MIR access. When that fixed part fits,
an unknown base is permitted for extent checking: the WebAssembly instruction
performs the current-memory bounds check and traps if `a + o + w` exceeds
memory. An unknown-base `Store` additionally requires a `cabi_realloc`
provenance proof (the complete design's rule; the current lowering emits no such
MIR store). This is a deliberate boundary between compile-time checks and
runtime checks; the compiler does not reject a dynamic canonical pointer just
because it cannot prove its runtime value.

The verifier resolves static addresses while the MIR instructions and their
source spans are available. It recognizes the scratch interval
`[0, SCRATCH_END)` and the allocator's heap-state region, both allocator-owned
and read/write. String literals are GC values and are not MIR-addressable, so
they define no region. A resolved constant address must belong to a known
region, and its entire effective interval must remain within that same region;
an access into a gap, across a region boundary, or into allocator metadata is
rejected. This checks the bytes the MIR operation can touch without adding
memory provenance or object-layout fields to MIR.

Static address analysis follows `Constant` and `Copy` values,
the `i32.add` and `i32.sub` operations when their operands are statically
known, and block parameters whose incoming values all resolve to the same
address. Other operations, function parameters, imported results, and loaded
values are unknown. A known value whose arithmetic wraps as an `i32` remains a
known numeric address, so the verifier applies the region and effective-range
checks to the resulting address. Conflicting block inputs become unknown. If a
known symbolic literal base is combined with an unknown operand, the result is
unknown; this checker does not infer dynamic object bounds or add runtime
instrumentation.

This scope is conservative for the current ABI. Scratch and heap-state offsets
are owned by the compiler and can be checked exactly. Returned pointers,
function parameters, and other host-provided values are dynamic pointers;
proving their allocation provenance requires MIR metadata or ABI checks, while
rejecting them would reject valid canonical calls. They therefore use the
WebAssembly runtime's linear-memory bounds trap. That runtime check proves only
that an access is inside the current memory; it does not prove that a dynamic
pointer stays within a particular allocation or string object. It also does not
prove write permission. The complete design accepts a dynamic `Store` only when
the pointer is proven to come from `cabi_realloc`; the current ABI lowering
emits no `Store` through an unknown dynamic address. A runtime bounds check
alone cannot protect allocator metadata or static regions, so a future dynamic
store must establish writable-buffer provenance in ABI lowering or be rejected.

### Rejected alternative

- **A linear-memory language heap.** Rejected by
  [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md): a
  garbage-collected source language run on linear memory would need a
  hand-written collector (root and stack maps, tracing, compaction) that
  duplicates the engine's GC. Reclaiming call-scoped ABI buffers
  ([DEC-10](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md)) does not
  make linear memory a language heap; the former `LinearMemoryPlanner`, its
  object layouts, its erased boxing path, and the language-object
  pointer-bounds verifier are removed.

## Algorithms

### `cabi_realloc`

The allocator's `realloc`, `allocate`, `free`, and coalescing algorithms are
owned by [canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md).
This document relies on its contract: an aligned payload pointer with a 4-byte
length prefix immediately before it, reclamation of freed storage, and a trap on
growth failure or address overflow.

### Passing a string argument

```text
lower_string_argument(string):
    length = StringLength(string)          # GC byte-array length
    buffer = cabi_realloc(0, 0, 1, length) # transient linear buffer
    copy StringBytes(string) -> buffer
    push (buffer, length)                  # canonical (pointer, length)
    # the caller frees `buffer` after the canonical call returns
```

### Reading a returned string

```text
read_returned_string(retptr):
    pointer = Load [0] retptr         # returned payload pointer
    length  = Load [4] retptr
    if length == 0: return empty_gc_string
    bytes = LoadBytes [pointer, pointer + length)
    value = StringFromBytes(bytes, length)   # fresh GC string
    cabi_realloc(pointer, length, 1, 0)      # free the host buffer
    return value
```

A returned list whose element type is not a byte is copied element-wise into a
GC array instead; the byte path above is the `String`/`list<u8>` specialization.

### Verifying static access extents

```text
verify_static_access_extents(module, layout):
    regions = [{scratch interval, permissions: read/write},
               {heap-state interval, permissions: read/write}]
    require regions are disjoint and each lies within the pointer-width address space

    for each function:
        facts = solve_address_facts(function, layout)
        for each Load, Load8U, or Store instruction:
            width = 1 if instruction is Load8U else 4
            require u64(offset) + width <= 2^32
            if facts[address] is Known(base):
                start = checked_add(u64(base), u64(offset))
                end = checked_add(start, width)
                require end <= 2^32
                region = the unique MIR-addressable region containing base
                require region exists and end <= region.end
                access = read for Load/Load8U, write for Store
                require access in region.permissions
            else:
                # Wasm checks the actual address against current memory.
                accept dynamic reads; require separate writable-buffer proof
                        for dynamic stores

solve_address_facts(function, layout):
    initialize function parameters and unsupported results as Unknown
    initialize block parameters as Pending
    repeat until facts stop changing:
        Constant(v)       => Known(u32_bit_pattern(v))
        Copy(v)            => facts[v]
        I32Add(a, b)       => Known(wrapping_add(a, b)) if both are Known
        I32Sub(a, b)       => Known(wrapping_sub(a, b)) if both are Known
        I32Add/I32Sub      => Unknown if any operand is Unknown
                              Pending if no operand is Unknown and one is Pending
                              Unknown otherwise
        block parameter    => Known(v) if every incoming argument is Known(v)
                              Unknown if there are no incoming arguments, facts
                              conflict, or any fact is Unknown
                              Pending otherwise
        all other results  => Unknown
    treat any remaining Pending fact as Unknown
```

`Known` values are interpreted as unsigned pointer-width addresses
(`i32` wasm32 / `i64` memory64) when used as a memory base. `Pending` only means
that a loop or unresolved predecessor has not provided a fixed-point fact yet;
treating it as `Unknown` keeps the analysis conservative. The fixed
`offset + width` check also applies to `Unknown` bases; it rejects only an
instruction that is out of range for every possible base. Static diagnostics use
the memory instruction's source span. String literals are GC values and never
appear as an address fact; only scratch, heap-state, and proven allocator
pointers are MIR-addressable.

### Edge cases

- A GC string is copied into a transient buffer to pass to an import and back
  into a GC string on return; the buffer is freed in both directions.
- A zero-size realloc frees and returns `0`; an imported empty string becomes an
  empty GC string with no linear buffer.
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
    lower/extent.rs
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
- `wasm/lower/runtime.rs` must collect and deduplicate string literals, encode
  them as passive data segments, and allocate one mutable `(ref null $string)`
  global per distinct used literal for lazy interning. The structurer emits
  `array.new_data` once per literal behind that global's `ref.is_null` guard.
  Required entry point: `fn plan_data_segments(strings, layout) -> DataSegments`.
- `wasm/lower/realloc.rs` must synthesize the general `cabi_realloc` allocator
  and export it only when the module crosses the canonical ABI (a returned string
  or byte list, an indirect parameter record, or an export return area). Its
  contract, block layout, and free lists are owned by
  [canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md).
  Required entry point: `fn synthesize_realloc(layout) -> Function`.
- `wasm/lower/mod.rs` must assemble the memory (minimum pages), the data
  segments, and the allocator export into the thin Wasm module. After planning
  those segments and before structuring instructions, it must run
  `lower/extent.rs` against the MIR module and resulting ABI memory layout.
- `wasm/lower/extent.rs` must resolve statically known MIR addresses, compute
  checked access intervals, and reject known accesses outside the scratch and
  heap-state regions, reject known stores into read-only regions, and reject
  accesses beyond the wasm32 address space. Unknown dynamic addresses pass this
  static extent check when their fixed `offset + width` fits the wasm32 space;
  dynamic stores also require a `cabi_realloc` provenance proof. The emitted
  WebAssembly instruction supplies the runtime current-memory bounds trap.
  Required entry point:
  `fn verify_static_access_extents(module, layout) -> Result<(), Vec<BackendError>>`.
- `wasm/lower/structure/instructions.rs` must lower the MIR boundary
  instructions to leaf Wasm load/store/convert instructions carrying a `MemArg`.
- `mir/wit/mod.rs` and `mir/wit/parameters.rs` must adapt strings and byte lists
  to and from the `(pointer, length)` exchange format and free each transient
  buffer at the point fixed by
  [canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md);
  a non-byte list must be rejected before this layer. They must not emit a
  dynamic `Store` without an ABI-level writable-buffer guarantee; current
  lowering emits no dynamic MIR store.

**No language objects in linear memory.** The module tree must not allocate
language aggregates, closures, variants, arrays, or erased values in linear
memory. GC objects are referenced only by opaque GC handles; the only values
that cross this boundary are `i32` addresses, lengths, and canonical scalars.
Any module needing a language-heap operation must depend on the GC lowering path
([IR boundaries](../00-ir-boundaries.md)), not on these modules.

## Invariants and verification

The MIR memory verifier checks that each `Load`/`Load8U`/`Store` names a memory
selected by the target profile, that the address has the profile pointer type,
and that the loaded, stored, or converted value has the required width and
`i32`/`i64` type; `WrapI64` and `WidenI64` are checked for the matching
`i64`/`i32` operand and result. The static extent pass runs after the data layout
is known and checks each resolvable address against the pointer-width address
space and its declared ABI region. Dynamic addresses remain valid and rely on
the WebAssembly instruction's runtime bounds check; a dynamic store additionally
requires a `cabi_realloc` provenance proof. The thin-IR verifier and WebAssembly
validator then check the emitted leaf instructions and allocator body
([Wasm encoding](encoding-and-structuring.md)).

## Worked example

An imported function returns a 5-byte string. The lowering calls
`cabi_realloc(0, 0, 1, 5)`; the allocator places the 5-byte payload at an aligned
address, records its size header and length prefix, and returns the payload
pointer, say `48`. The return area at address `0` holds `(48, 5)`. The host
writes the 5 bytes at `[48, 53)`. The lowering then copies those bytes into a
fresh GC string, frees the buffer with `cabi_realloc(48, 5, 1, 0)`, and makes
the GC string the source result; no linear buffer remains.

For a static extent example the scratch region is `[0, 16)`, followed by the
heap-state region. A 4-byte `Store` at scratch address `0` is allowed. A
dynamic `Load8U` with offset `0xffff_ffff` passes the fixed-part check because
its maximum interval ends at `2^32`; it traps at runtime unless the memory has
all `2^32` bytes and the base is zero. A dynamic `Store` is accepted only when
its address is proven to come from `cabi_realloc`.

## Boundaries and interfaces

- **Input:** MIR byte operations and canonical ABI adaptation produced by P9;
  GC string values from MIR.
- **Output:** the memory, its passive data segments, and the `cabi_realloc`
  export, all carried in the thin Wasm module.
- **To the Canonical ABI layer:** the transient buffer convention and the
  `(pointer, length)` exchange; string/byte-list copy in and out.
- **To GC lowering:** strings and every other language value are GC-managed;
  linear memory only carries transient canonical bytes
  ([DEC-10](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md)).

## Open questions and future work

- **Memory layout.** Canonical memory layout for indirect records, tuples, and
  variants beside the scalar/byte shapes.
- **Memory64 and multi-memory.** Widen addresses and the allocator to `i64`, and
  select among ABI memories, when a profile and the toolchain/host support it
  ([capability profile](capability-profile.md)).
- **Dynamic pointer provenance.** The static extent pass does not yet prove that
  a dynamic allocator pointer stays within its allocation. The complete design
  requires a `cabi_realloc` provenance proof for dynamic stores; stronger
  object-bound guarantees are added only if the canonical ABI requires them
  beyond WebAssembly's current-memory bounds trap.

## Implementation notes

The allocator and buffer-lifetime gaps are owned and tracked by
[canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md)
and its [implementation checklist](../../implementation/backend/canonical-buffer-allocation.md):
`cabi_realloc` is now a reclaiming allocator and transient buffers are freed at
the boundary; the general `cabi_post_<name>` synthesis (return-area and buffer
free) is implemented and fixture-verified, while no production export has a
non-scalar result to trigger it and resource handles are lowered. The ABI memory
here is fixed to `MemoryId(0)` with `i32` addresses; the profile does not yet
select a pointer width or additional memories.

Strings and literals match the complete design: a source `String` is the GC
`(array (mut i16))` type, literals are passive data segments materialized once
with `array.new_data` and interned in a lazily initialized mutable global, and
the ABI adapter transcodes UTF-16 to and from the
component's UTF-8 (invalid sequences and unpaired surrogates become U+FFFD). The
static MIR access-extent pass covers the scratch and heap-state regions; GC
string literals are not MIR-addressable and define no region. The thin-IR
verifier, the Wasm validator, and the allocator regression suite remain
implemented against the current representation.

## References

- WebAssembly 3.0 specification: memories, `memory.size`, `memory.grow`, load
  and store instructions, data segments.
- [WebAssembly Component Model Canonical ABI](https://github.com/WebAssembly/component-model/blob/main/design/mvp/Explainer.md#canonical-abi):
  `realloc`, return pointer, list and string passing.
- [DEC-09 — GC-Only Language Heap](../../../decision/DEC-09-gc-only-language-heap.md),
  [DEC-10 — Canonical ABI Buffer Ownership and Lifetime](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md),
  [DEC-05 — Target wasmtime's WebAssembly Feature Set](../../../decision/DEC-05-wasmtime-feature-set.md).
- [canonical ABI and WIT](canonical-abi-and-wit.md),
  [canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md),
  [capability profile](capability-profile.md),
  [data representation](../fp/data-representation.md).
