# D-10 — Linear-Memory Representation

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Purpose

Define the concrete runtime representation produced by the P9 linear-memory
planner, so the same target-neutral CC can lower to an MVP core module as well
as to Wasm GC. This document complements
[D-06](D-06-low-level-ir-and-wasm-types.md) (planner contract and IR
boundaries) and [D-09](D-09-scalar-and-numeric-lowering.md) (scalar semantics).
It supersedes the earlier statement in
[D-02](D-02-wasm-lowering.md) that linear memory is reserved for the WASI
boundary: the linear planner is a real language-heap strategy, and WASI remains
its byte-oriented boundary.

## Address model

- The linear target is **wasm32**: every pointer is an `i32` byte offset into a
  single memory. `memory64` is disabled in the capability profile
  ([D-05](D-05-backend-capability.md)), so the address type and the `ValueType`
  of every handle are `i32`.
- The planner assigns a `MemoryId`; the current profile has exactly one memory
  (`MemoryId(0)`). `multi_memory` is disabled. A `LinearLoad`/`LinearStore`
  carries the `MemoryId` it addresses so a future profile can add memories
  without changing CC.

The address type is a planner parameter, not a CC concept: CC references are
opaque handles, and only the planner's `value_type` chooses `i32`. A future
memory64 profile would make that choice `i64` and widen the linear access
instructions, but it stays disabled because the pinned component toolchain
cannot lift 64-bit memories and the WASI host path is incomplete
([D-05](D-05-backend-capability.md)).
- A reference `ValueShape` lowers to an `i32` handle. Nullability is a runtime
  concern of the source layer; the planner emits `0` for a null handle.

## Object layouts

All offsets are byte offsets. Fields are aligned to their natural alignment
(`4` for `i32`/handles, `8` for `f64`); the planner rounds each field offset up
to its alignment. Sizes are rounded to the object's alignment.

| CC requirement | Linear layout | Size |
| --- | --- | --- |
| `Box { value }` | `value` at offset `0` | aligned size of `value` |
| `Product { fields }` | each field at its aligned offset, from `0` | last field end, aligned |
| `Variant { cases }` | `tag: i32` at `0`; the selected case's fields at aligned offsets after the tag | `4` plus the largest case size, aligned |
| `Array { element }` | `length: i32` at `0`; elements at `4 + i * stride` | `4 + length * stride` |
| closure environment | function-table slot `i32` at `0`; captures at aligned offsets after it | `4` plus captures, aligned |

A `Variant` case's field offsets are computed independently per case, starting
after the shared tag. `VariantGet` is only reached on the matching case, so
overlapping case payloads are safe and a sum costs its largest case.

Scalars are stored by value with the type from
[D-09](D-09-scalar-and-numeric-lowering.md): `i32`/`Boolean` as `i32`, `Number`
as `f64`, `Char` as `i32`, and references as `i32` handles.

## Strings

A `String` is an `i32` pointer to a length-prefixed UTF-8 buffer: a 4-byte
little-endian length followed by the bytes. This is the same value the WASI
canonical ABI consumes, so the boundary needs no conversion. String literals
live in active data segments.

## Variant dispatch

Following [DEC-08](../decision/DEC-08-target-neutral-variant-representation.md),
constructor dispatch is tag-based and never uses a GC type test:

- `VariantNew { representation, case, fields }` allocates the object, stores the
  case tag at offset `0`, and stores the case fields at their offsets.
- `VariantTag { representation, value }` loads the tag at offset `0`.
- `VariantGet { representation, case, field, value }` loads the field at the
  case's offset.

A sum type whose constructors are all nullary uses an immediate `i32` tag and
integer comparison; it allocates nothing and creates no `Variant` requirement.

## Erased values

The erased representation used by polymorphic values
([D-08](D-08-generic-wasm-representation.md)) is an `i32` handle on the linear
target:

- A concrete scalar is boxed by allocating its `Box` payload and yielding the
  handle; unboxing loads it.
- A concrete reference is already a handle and needs no allocation.
- `RepresentationTest`/`RepresentationCast` are **not** used for constructor
  dispatch (DEC-08). When they are used for erased adaptation, the linear
  lowering makes a cast between compatible erased and concrete references a
  no-op and boxes or unboxes only across a scalar boundary.

## Allocator

- Allocation is a bump allocator: a free pointer lives in linear memory and
  advances by the aligned object size. There is no reclamation yet; a program
  that allocates without bound will exhaust memory and trap.
- The module exports `cabi_realloc` for the canonical ABI. Allocations are
  length-prefixed so a returned `(pointer, length)` pair is a valid `String`
  value, as described in [D-07](D-07-wit-imports-and-std.md).
- A reserved scratch region at the start of memory holds the return-pointer
  area of canonical ABI calls; string and object data begin after it.
- The allocator grows memory on demand with `memory.grow` (`bulk_memory` is not
  required).

## Instruction contract

P9 lowers CC to these MIR instructions, which the MIR verifier checks:

- `LinearAlloc { bytes }` produces an `i32` pointer; `bytes` is nonzero and is
  the aligned size of the planned object.
- `LinearLoad`/`LinearStore` carry `MemoryId`, a byte `offset`, and the
  `ValueType` of the access. The offset must be within the planned object, the
  access type must match the stored shape, and the alignment implied by the
  type must not exceed the field's planned alignment.
- `LinearClosureNew`/`LinearClosureCall`/`LinearClosureGetCapture` use the
  function-table slot and environment layout above.

`i64` accesses are permitted only for canonical ABI adaptation
([D-07](D-07-wit-imports-and-std.md)); source-level values do not use `i64` yet.

## Capability gating

The linear planner is selected by `TargetCapabilities` and requires only core
MVP. It must reject any requirement it cannot represent with a source-associated
P9 diagnostic rather than silently switching strategies. With the variant and
erased lowering above, the remaining GC-only operations are the GC-specific
aggregate, array, closure, and reference instructions, which the planner never
emits.

## Delivery order

1. Add the `MemoryId` and alignment fields to the linear load/store instructions
   and verify them.
2. Lower `VariantNew`/`VariantTag`/`VariantGet` and add execution tests for a
   data type with fields on the MVP profile.
3. Lower erased boxing/unboxing and `RepresentationTest`/`RepresentationCast`
   adaptation.
4. Make the allocator alignment- and scratch-aware and cover repeated
   allocations with an execution test.
5. Reuse the canonical ABI adapter on the linear profile where its result forms
   are supported.
