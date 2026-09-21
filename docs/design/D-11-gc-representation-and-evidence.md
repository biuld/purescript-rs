# D-11 — Wasm GC Representation and Execution Evidence

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Purpose

Record the concrete Wasm GC representation the P9 GC planner produces and the
execution evidence each backend capability needs. This complements
[D-06](D-06-low-level-ir-and-wasm-types.md) (planner contract and verification),
[D-08](D-08-generic-wasm-representation.md) (erased values),
[DEC-07](../decision/DEC-07-runtime-representation-for-parameterized-adts.md)
(parameterized ADTs), and
[DEC-08](../decision/DEC-08-target-neutral-variant-representation.md) (sum
types). It is the reference for what a GC-mode MIR module looks like and how a
capability is promoted from `Partial` to `Implemented`.

## Value representation

| CC `ValueShape` | MIR `ValueType` |
| --- | --- |
| `Integer` | `i32` |
| `Boolean` | `i32` (`0`/`1`) |
| `Number` | `f64` |
| `Reference(Repr(_))` | `(ref null? $repr)` |
| `Reference(Aggregate)` | `(ref null? struct)` |
| `Reference(Closure(_))` | `(ref null? $closure)` |
| `Reference(Erased)` | `(ref null? eq)` |

`String` is not a GC object: it stays an `i32` linear-memory pointer to a
length-prefixed UTF-8 buffer so it is directly usable by the canonical ABI
([D-10](D-10-linear-memory-representation.md)).

## Object layouts

All GC objects are immutable unless stated otherwise.

| CC requirement | MIR type | Fields |
| --- | --- | --- |
| `Box { Integer }` | `struct` | one `i32` |
| `Box { Number }` | `struct` | one `f64` |
| `Product { fields }` | `struct` | each field's storage, in order |
| `Variant { cases }` | abstract `struct { tag: i32 }` plus one final case `struct { tag: i32, fields... }` | see below |
| `Array { element }` | `array (mut storage)` | one mutable element type |
| closure | `struct` | `(ref $code)` then `(ref $capture_array)` |
| capture array | `array (mut (ref null eq))` | one mutable nullable `eqref` per capture |

A `Variant` is realized per
[DEC-08](../decision/DEC-08-target-neutral-variant-representation.md): an
abstract, non-final supertype carries the tag, and each case is a final subtype
that adds its fields. `VariantNew` is `struct.new` of the case type,
`VariantTag` is `struct.get` of the shared tag, and `VariantGet` is `ref.cast`
to the case type followed by `struct.get`. A sum type whose constructors are
all nullary is an immediate `i32` tag and allocates nothing.

Captures are stored in one uniform `eqref` array so a closure type is
independent of its capture types:

- a `Boolean` capture is boxed as an `i31` value;
- an `Int` capture is boxed in the `Box { Integer }` struct so all signed
  32-bit values round-trip without truncation;
- an `f64` capture is boxed in the `Box { Number }` struct;
- a reference capture is stored as-is; and
- an erased capture is already `eqref`.

The GC planner emits these types as one recursion group. `Box` layouts are
created only when a value is actually boxed, so an unreachable box does not
become a type.

## Operations

| CC operation | GC lowering |
| --- | --- |
| `ProductNew` / `ProductGet` | `struct.new` / `struct.get` |
| `VariantNew` / `VariantTag` / `VariantGet` | `struct.new` / `struct.get` / `ref.cast` + `struct.get` |
| `ArrayNew` / `ArrayGet` / `ArraySet` / `ArrayLen` | `array.new_fixed` / `array.get` / `array.set` / `array.len` |
| `FunctionRef` | `ref.func` plus `struct.new` of the closure with its capture array |
| `ClosureGetCapture` | `struct.get` of the capture array, `array.get`, then unbox if needed |
| `IndirectCall` | extract the code reference and `call_ref` |
| `RepresentationTest` / `RepresentationCast` | `ref.test` / `ref.cast` |
| nullary constructor / tag test | `i32` constant / `i32.eq` |
| erased adaptation | box, unbox, or a no-op cast (D-08) |

The MIR verifier checks each of these against the concrete type table, as
specified in [D-06](D-06-low-level-ir-and-wasm-types.md).

## Capabilities

The GC path requires `gc`, `reference_types`, and `function_references`, and
uses `multi_value` only if a generated signature has more than one result.
SIMD, tail calls, exceptions, threads, and memory64 are not used. The GC
planner rejects an MVP-only profile before MIR verification with a
source-associated diagnostic.

## Execution evidence

Every capability row is promoted to `Implemented` only with the four pieces
[D-05](D-05-backend-capability.md) requires: a capability flag, lowering and
validation coverage, a binary or WAT regression test, and a Wasmtime execution
test where the behavior is observable. A local regression alone is not enough
for a row that the official suite covers.

The matrix below names the fixture class and the profile it must run under.
`wasmtime` execution tests skip when the runtime is unavailable, so they never
block `cargo test --workspace`.

| Capability | Fixture class | GC | MVP linear |
| --- | --- | --- | --- |
| Scalars and direct calls | integer/boolean/number arithmetic and comparisons | required | required |
| `if` and `case` | value-producing branches and constructor matches | required | required |
| Nullary data types | tag construction and tag comparison | required | required |
| Field data types | construction, tag test, and field projection (DEC-08) | required | required |
| Newtypes | erased single-field construction and match | required | required |
| Records | literal, field read, and update | required | required |
| Arrays | literal, length, index, and update | required | required |
| Closures | captured scalar/reference closures and higher-order calls | required | required |
| Parameterized ADTs | erased field construction and recovery | required | not yet |
| Strings and `log` | data segment, length prefix, stdout write | required | not yet |
| WASI clock and random | monotonic clock and random bytes | required | not yet |
| Variants (unified) | mixed nullary/field sum construction and match | required | required |

"Not yet" marks a real gap that the corresponding design document closes
([D-10](D-10-linear-memory-representation.md), [D-07](D-07-wit-imports-and-std.md)),
not a permanent limitation.

## Delivery order

1. Land the unified variant representation (M6) with GC and MVP execution
   tests, and add the row to the matrix.
2. Fill the GC gaps: parameterized erased fields, records, arrays, and
   closures across the full operation set.
3. Add the missing linear rows as [D-10](D-10-linear-memory-representation.md)
   lands.
4. Keep the matrix and the D-05 audit in sync; a new CC operation adds its
   verifier and its execution test before it counts as evidence.
