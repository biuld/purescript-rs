# DEC-22 — Labelled Storage for Open and Erased Records

**Status:** Proposed
**Date:** 2026-10-17

## Context and constraints

Closed typed products cannot represent an unknown row tail. The old canonical
product with erased fields also loses the field metadata needed when an erased
record later appears at an open-row use, including values extracted from an
Array or parameterized ADT. Dynamic `Record.Unsafe` keys make this an actual
library requirement rather than an optimization opportunity.

The compiler must preserve the official record dictionaries and pure library
definitions, keep one producer/consumer representation contract, and support
generic, recursive, imported and higher-order uses. Choosing an index for one
closed call, recognizing reflected literal names, or requiring complete
monomorphization does not meet those constraints.

## Decision

Adopt one immutable labelled record-storage protocol for open rows and bare
erased record values. It consists of parallel arrays of canonical String labels
and recursively erased payloads, held by one header. Labels are unique and
sorted; lengths agree. Closed typed records retain specialized products and
cross this boundary through explicit pack/unpack operations. Complete closed
recovery validates the actual labels and lengths, rather than casting between
nominal products.

The record owner publishes a checked schema containing the exact representation
handles. Structural reads consume it; they do not search for similar physical
products. Type checking owns row relations and field types, the payload owner
owns recursive erasure/recovery, and P9 owns physical GC types. Canonical ABI
views retain both the logical closed fields and their actual labelled storage.
MIR checks these views against their owner and physical definitions; recursive
list copies retain field descriptors until their actual accesses are emitted.

Schema checks establish physical structure. They do not prove arbitrary runtime
array contents. Source construction and checked dynamic constructors own sorted,
unique, equal-length contents; runtime reads and closed recovery enforce their
required bounds and shape checks. Normal source typing remains a producer
obligation at the Core-to-CC boundary. Labels are runtime field names, not source
type IDs or dynamic type tags.

The full design is [open record rows](../design/backend/fp/open-records.md);
the [execution checklist](../implementation/backend/open-records.md) controls
acceptance. This proposal is not a claim that every slice is implemented.

## Consequences

Generic records retain their complete tails through arrays, ADTs, closures and
ABI adaptation. Source record dictionaries can use ordinary typed storage
primitives without a second implementation of Eq, Ord, Show or row solving.
Immutable record conversion may allocate; nested array and nominal-reference
identity still follows the existing owners' policies.

The representation costs a header, two arrays, field erasure, and label checks.
Closed operations keep direct products, and later optimizations may remove
proven-redundant conversions or share metadata. Correctness cannot depend on
those optimizations. JavaScript `undefined` and prototype behavior have no
implicit target representation: unsafe foreign adapters require explicit
differences and behavior evidence, without successful-looking placeholders.
