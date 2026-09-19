# DEC-07 — Runtime Representation for Parameterized ADTs

**Status:** Accepted  
**Date:** 2026-09-19

## Context

The type checker already preserves parameterized algebraic data types, but the
first backend slice only lays out concrete, non-parameterized constructors.
Parameterized values must work with records, arrays, closures, and higher-order
functions without putting source type variables into MIR or Wasm layouts.

Two implementation strategies were considered:

1. Whole-program monomorphization: discover reachable type applications and
   emit a specialized layout and function for each one.
2. Runtime erasure: keep one layout per source constructor and represent values
   whose layout depends on a type parameter through a uniform runtime value.

Monomorphization is attractive for unboxed scalar performance, but it requires
whole-program specialization, duplicate handling for recursive and higher-order
uses, and a fallback for values whose instantiation is not statically visible.
That would make closures and separate compilation depend on specialization.

## Decision

Use runtime erasure for parameterized ADTs. Type parameters remain explicit in
HIR, THIR, and Typed Core for checking and specialization-independent
diagnostics, then disappear at representation lowering. A parameterized
constructor has one GC layout per constructor, independent of its type
arguments.

- A field whose runtime representation depends on a type parameter is stored
  as a boxed `eqref`-compatible value. Scalars are boxed at the boundary and
  unboxed when a typed consumer requires a scalar; GC references can be passed
  without an additional allocation when they already satisfy the erased
  representation.
- Fields with a representation proven independent of the parameters may stay
  unboxed. The layout verifier, not the frontend, decides this optimization.
- Constructor identity and pattern tests use the concrete GC constructor type;
  source type arguments are not runtime tags.
- The same erased value protocol is reused by generic records, arrays, and
  closure captures. Newtypes remain a separate rule: a valid single-field
  newtype is represented by its field and allocates no wrapper.
- No whole-program specialization is required for correctness. A later
  optimization may monomorphize selected hot paths, but it must preserve the
  erased representation as the semantic fallback.

## Consequences

- Generic functions and values can cross module and closure boundaries without
  requiring a complete list of call-site instantiations.
- The backend needs boxing/unboxing operations and a verifier for erased field
  layouts before parameterized ADTs can execute.
- Some generic scalar operations pay a boxing cost, while concrete
  non-parameterized fields retain the existing unboxed layouts.
- Type identity remains compile-time information; runtime checks distinguish
  constructor layouts rather than comparing source type arguments.
- The implementation can proceed incrementally: first add erased GC fields and
  boxed scalar primitives, then reuse them for arrays and closure captures.

