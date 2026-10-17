# Open Record Rows and Dynamic Fields

**Feature:** F-02
**Status:** Draft
**Prerequisites:** [representation and evidence](representation-and-evidence.md),
[polymorphism and erasure](polymorphism-and-erasure.md), and
[CC IR](cc-ir.md).
**Summary:** A record whose row is abstract needs a layout that retains fields
outside its known prefix. The record owner supplies one immutable labelled
storage protocol for open rows and erased record values. Closed typed products
enter and leave that protocol through explicit recursive conversions.

## Scope

This contract covers record construction, projection, patterns, immutable
updates, row instantiation, bare polymorphic storage, nested aggregates,
callables, canonical ABI adaptation and checked record-storage primitives.
P5 owns row equality, `Lacks`, `Cons`, `RowToList` and dictionary evidence.
Runtime field selection does not solve those relations again.

## Background

An open row's known fields are only a prefix of its semantic contents. Treating
that prefix as a complete product loses the unknown tail. A product index
selected for one closed instantiation is not an index for every instantiation.
The original prelude's record dictionaries also use dynamic string keys through
`Record.Unsafe`; recognizing one reflected constant cannot implement that API.

## Model

The record owner registers one `RecordStorage` schema per representation table:

```text
labels : Array String
values : Array Erased
record : Product [labels, values]
```

These are internal arrays, distinct from source-level array element typing.
The schema is immutable checked metadata. It names its three representations;
consumers do not search the table for physically similar products. Labels and
values have equal lengths, labels are unique and sorted by the target's
Unicode-scalar string ordering, and each value uses the existing recursive
erased-payload protocol. Runtime labels are field names, not source type IDs or
runtime type tags. Unknown tails retain their complete label/value contents.

## Design

Closed typed records retain their specialized product layouts. Open records
use `RecordStorage`. The canonical owner protocol for a record entering a bare
erased slot is also `RecordStorage`, so passing through `forall a. a -> a`
does not discard the metadata a later open-row use needs.

Packing projects a closed product in canonical label order and erases each
field through the common payload planner. Unpacking recovers each required
field through that same planner. Recovery to a closed row validates its complete
label set; recovery to an open row retains the complete storage. Nested record,
array and callable fields use these plans recursively, including nominal ADT
payloads and generated ABI adapters. No nominal cast between a product and the
record store substitutes for packing or unpacking.

Known-prefix field access and record patterns select by label and recover at
the checked field type. Immutable updates evaluate the base and all update
expressions exactly once in source order, then replace those fields in fresh
storage, retaining the unknown tail. No record operation mutates its input.

## Algorithms

Storage primitives expose bounded structural operations: field count, label at
an index, field read at its checked use type, and checked construction from
private labelled slots. The library owns searches, insert/delete algorithms and
the official `Record.Unsafe` wrappers. Heterogeneous private slots must retain
the producer's recursive erased policy; an arbitrary nominal `Any` cast cannot
stand in for the slot contract. Bounds, mismatched lengths and duplicate labels
fail explicitly. Missing keys cannot fabricate a value of arbitrary result
type. Invalid unsafe JS inputs involving `undefined` or object prototypes need
an explicit target contract and separate oracle observations.

The runtime contract describes source operands, result types and the projection
to the shared `RecordOperation` owner. Before ABI erasure, a slot source type
must resolve through its checked type applications to a positively identified
ordinary data declaration with no constructors. Preserve the declaration's
identity and applied arguments; do not require a bare type node or infer the
carrier from its spelling. Absent type metadata, newtypes, opaque declarations
and constructor-bearing declarations do not qualify. P8 checks the source
contract and closed-record conversions, while P9 retains the provider export
identity and verifies its exact CC operation and physical signature. P9 trusts
P8's checked Core source scheme and cannot reconstruct a `Slot` declaration
identity from erased CC arrays.
The provider boundary reuses the application record schema and checked kernels;
it does not supply a second record codec. `build` copies both input arrays,
checks equal lengths, non-null labels and slots, and strict Unicode-scalar label
ordering, then returns fresh storage. Its `Array String` input still has the
source array's erased element representation, so labels are recovered as
strings before entering the owner's concrete `Array String` field.

The generated provider namespace is `psrs:record-storage`, with exports and
checked source signatures `length :: forall r. Record r -> Int`,
`key_at :: forall r. Int -> Record r -> String`,
`payload_at :: forall r. Int -> Record r -> Slot`, and
`build :: forall r. Array String -> Array Slot -> Record r`. These declarations
are checked source bindings, not raw Wasm imports. P9 discharges each binding's
identity only after it matches the projected record operation, then lowers
through deduplicated application-local helpers over the registered
`RecordStorage` schema and codec. This keeps record construction and access
under the one application owner instead of introducing a second schema or
codec; it is an ownership choice, not a restriction on cross-module GC
references.

## Code map

`cc/record/` owns the checked schema and record plans. `cc/layout/` assigns
open-row shapes and registers the owner; `cc/payload` uses the owner at erased
boundaries. `cc/lower/record/` consumes source row evidence and emits explicit
operations. CC verification checks schema and plan endpoints, complete field
coverage, conversion direction and operand use shapes. MIR lowering realizes
these operations and verifies their physical layouts. `psrs-runtime` owns the
checked primitive contracts, while `psrs-stdlib` owns wrappers and algorithms.

## Invariants and verification

- An open row never becomes its known prefix's closed product.
- A record has one authoritative label/value sequence; no independent copied
  typed payload can disagree with it after an update.
- Schema metadata validates referenced representations, nullability and array
  elements. This proves physical schema, not arbitrary runtime array contents.
- Packing and checked dynamic construction establish contents invariants;
  reads and recovery enforce required lengths, labels and bounds.
- Optimizations may remove conversions only when the same owner contract proves
  them unnecessary. Finite local-row specialization is optional for correctness.
- WIT adapters retain logical closed field requirements while accessing the
  producer's actual storage, including erased record fields in ADTs.

## Worked example

`read :: forall r. { value :: Int | r } -> Int` receives labelled storage.
Calls with `{ value: 42, label: "x" }` and `{ enabled: true, value: 42 }`
pack different closed products into that same storage shape. Selection uses
`"value"`, not a shared assumed index. Updating `value` preserves `label` or
`enabled`; returning the result through an erased slot retains those fields.

## Boundaries and interfaces

CST/HIR gain no backend metadata. Core preserves complete rows and checked
instantiations. CC owns storage and conversion requirements; MIR owns physical
GC types and operations. Foreign storage use types are checked before ABI
erasure. Missing schema or conversion evidence is an error.

## Open questions and future work

Implementation acceptance is tracked in the [execution checklist](../../../implementation/backend/open-records.md).
The existing closed-product-only implementation remains incomplete until every
required vertical slice, malformed-IR rejection and runtime comparison passes.

## References

- [DEC-17](../../../decision/DEC-17-representation-and-evidence.md)
- Original prelude v6.0.1 `src/Record/Unsafe.{purs,js}`, pinned at
  `f4cad0ae8106185c9ab407f43cf9abf05c256af4`.
