# DEC-08 — Target-Neutral Variant Representation

**Status:** Accepted  
**Date:** 2026-09-21

## Context

Sum types (PureScript data types) must lower to both target strategies the
backend supports:

- the Wasm GC target, which has nominal subtyping and the runtime type tests
  `ref.test`, `ref.cast`, and `br_on_cast`; and
- the linear-memory/MVP target, which has none of those and can only compare a
  stored tag.

The bootstrap represented each constructor as a separate CC `Product`
requirement carrying a leading tag field, and dispatched with
`RepresentationTest`/`RepresentationCast`. Those operations name a GC type test,
so the linear planner rejects them and a data type with fields cannot lower to
MVP. A target-neutral CC cannot decide between a GC type test and a tag
comparison; that choice belongs to P9.

A unified `Variant` requirement had been present in the CC model but was never
constructed, and its GC planning was too weak to be usable: it required every
case to have an identical field shape. It therefore represented neither the GC
nor the linear industry strategy.

## Decision

Represent each source sum type as one target-neutral `Variant` requirement: an
ordered set of cases, each with a stable tag and a field-shape list. Add three
target-neutral CC operations:

- `VariantNew { representation, case, fields }`
- `VariantTag { representation, value } -> Integer`
- `VariantGet { representation, case, field, value } -> ValueShape`

P9 realizes them per target:

- **GC planner.** Emit one abstract, non-final `struct` supertype that carries
  the tag, plus one final `struct` subtype per case that adds that case's
  fields. `VariantNew` is `struct.new` of the case type; `VariantTag` is
  `struct.get` of the shared tag; `VariantGet` is `ref.cast` to the case type
  followed by `struct.get`. The runtime object layout is one object per
  constructor, as before.
- **Linear-memory planner.** Emit `{ tag: i32, payload }`; each case lays its
  fields out after the tag at its own offsets. `VariantNew` allocates and
  stores; `VariantTag` loads the tag; `VariantGet` loads the field at the case's
  offset.
- A sum type whose constructors are all nullary keeps the immediate `i32` tag
  representation and integer comparisons; it creates no `Variant` requirement.

`RepresentationTest`/`RepresentationCast` remain in CC only for erased
representation adaptation
([D-08](../design/D-08-generic-wasm-representation.md)), not for constructor
dispatch.

## Consequences

- The same CC module lowers to the GC and linear planners, so pattern matching
  works on the MVP profile.
- The abstraction moves from one `Product` per constructor to one `Variant` per
  sum type; the concrete GC object layout is unchanged, so existing execution
  behavior and tests remain valid after the lowering is rewritten.
- CC, the CC verifier, both planners, and the case lowering gain variant
  operations and must be updated together.
- Type arguments are still not runtime tags: constructor identity is the case
  tag, following
  [DEC-07](DEC-07-runtime-representation-for-parameterized-adts.md).
- A future planner may choose a different case encoding (for example `i31` for
  nullary cases of a mixed sum) without changing CC.
