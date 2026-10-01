# DEC-12 — Resolved WIT Bindings and Descriptor-Based Lowering

**Status:** Proposed
**Date:** 2026-09-27

## Context and constraints

[DEC-06](DEC-06-runtime-interface-via-wit.md) made WASI 0.2 the host interface
and the canonical ABI lowerer the glue. [DEC-11](DEC-11-primitive-ffi-stdlib-wrappers.md)
kept the source side to primitive types and made `SourceType` the primitive
identity that CC and MIR cannot recover from layout alone.

The current ABI path keeps four views of one declaration type:

- the resolved HIR type on the external (`psrs_hir::ExternalSymbol::signature`);
- `SourceType`, projected from that type by `abi::classification`;
- the WIT classification (`WasiParamKind` / `WasiResultKind` / `FlatSlot`);
- CC's `ValueShape` and `Representation`.

`SourceType` is a compiler-embedded, hand-maintained binding table. Because it
drops the resolved type identity, `cc::source_abi` has to recover the
declaration's layout by structurally searching the Core type table
(`core_type_matches_source`). The mirror also breaks its own stated rationale:
`SourceType::Resource` embeds a `HirTypeId`.

The canonical ABI needs facts the PureScript type cannot express: numeric width
(`s8`..`u64` all map to `Int`), `string` versus `list<u8>` byte semantics, and
`own`/`borrow` ownership. Those facts must stay in a WIT descriptor. Only the
source-side mirror is redundant.

Industry implementations do not put a WIT type system in the compiler. A
binding generator emits typed guest declarations and the canonical-ABI glue,
and the compiler or runtime sees only the canonical ABI. Where a host language
lacks a needed type, the wrapper types live in a library (for example Go's
`cm` package), not in the compiler.

## Decision

WIT imports are resolved once, early, in a target-aware linking stage between
Typed Core and CC. The stage produces a `ResolvedExternal` that carries both
halves:

```text
ResolvedExternal = {
    symbol,
    type_id: TypeId,   // resolved source type, interned in the module type table
    wit: WIT descriptor // canonical kind, flat slots, retptr, ownership
}
```

- Each foreign import's declared signature is interned into the module type
  table and referenced by `TypeId`. The backend does not recover it by
  structural search. In the current implementation the interning runs at the
  Core linking boundary (`abi/link.rs`) against the module type table; the
  frontend may intern it earlier without changing the contract.
- CC and MIR lower calls from `(type_id, WIT descriptor)`. `SourceType` is
  removed. The WIT descriptor (`WasiParamKind` / `WasiResultKind` / `FlatSlot`)
  is retained; it carries the ABI facts the source type cannot.
- Because MIR sees CC, not Core, the source structure MIR needs (record field
  labels, enum case order) is recovered from CC's representation metadata, not
  from a source-type mirror. Record labels are already carried by
  `RepresentationTable::product_labels`; enum order is the validated constructor
  tag order. Signature validation therefore runs in the linking stage, where the
  Core type is available, and never in MIR.
- Core stays target-neutral. The WIT descriptor and the WIT names live in the
  linking side table, not in Core.
- The WIT-to-source mapping is a binding-generation concern. The compiler
  consumes generated declarations; it does not carry a WIT type system.

## Consequences

- One source of truth for the source side (Core layout) and one for the ABI
  (the WIT descriptor). Removes `core_type_matches_source` and the lossy
  HIR-to-`SourceType`-to-Core round trip.
- A foreign import's declared type must be representable in the Core type table
  so it can be interned. In the current slice this is done at the Core linking
  boundary and touches `psrs-core` and `psrs-backend`, not the frontend.
- The change is behavior-preserving. The source language does not change and
  DEC-11's primitive rule stands.
- The WIT descriptor stays out of Core because Core is target-neutral, so the
  linking result travels beside the module as a side table, as
  `ExternalBindings` does today.

Rejected alternatives:

- Making WIT types canonical compiler types. Width, byte semantics, and
  ownership are ABI metadata, not value types; newtype erasure would drop the
  nominal distinction before the boundary, and PureScript has no linear types
  to enforce ownership.
- Keeping `SourceType` and only replacing the structural search. That leaves
  the mirror and the four-way duplication.
- Putting the WIT descriptor in Core. Core is target-neutral; a component
  descriptor would couple it to one backend.
