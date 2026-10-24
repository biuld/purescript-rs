# DEC-25: Monomorphic static storage bindings

**Status:** Accepted, 2026-10-24\
**Feature:** [F-02](../feature/F-02-portable-programs.md)

## Context and constraints

A target library needs instance-local persistent state for console timers.
Ordinary non-function source globals currently lower to zero-argument functions
and are evaluated at each reference. A top-level allocation cannot promise
shared identity. A process-global runtime registry with an arbitrary key would
also allow unrelated source types to alias after erasure.

## Decision

Provide `psrs:runtime-static#cell` as a small checked storage primitive. Each
resolved foreign declaration selects one mutable, single-element array per
component instance. Its signature is a closed monomorphic `T -> Array T`.
Validate every declaration, including unused declarations, against that exact
relation before erasure. Reject forall binders and type variables anywhere in
the signature; do not instantiate a polymorphic persistent cell.

The ordinary CC payload protocol erases the initializer and retains the shared
array identity. P9 discharges the checked call to a MIR `StaticCell` operation,
retaining the binding identity, array layout, strict initializer and span.
The MIR verifier checks the common mutable erased-element array schema,
initializer/result compatibility, consistent layouts for repeated uses of a
binding, and the target's mutable-global capability. It trusts the producer's
pre-erasure monomorphism check, as it cannot recover source type equality.

P10 allocates one nullable GC global per used binding, sorted by resolved
identity. On a null global it allocates an array containing the initializer
and publishes the result; later calls return that same array. Arguments are
always evaluated strictly, including after initialization. No initializer
callback is invoked by the primitive, so it introduces no reentrant callback
or partially initialized logical element. Failed allocation publishes nothing.
Unused bindings create no state. Fresh component instances receive fresh cells.
The independent stateless GC storage provider remains stateless.

## Consequences

Timer lookup, lifecycle, elapsed arithmetic and output remain ordinary library
PureScript over checked storage and WIT leaves. The compiler has no timer-name
recognizer or timer registry algorithm. Static storage requires WebAssembly GC
and mutable globals and has ordinary allocation/trap effects; optimizers must
not treat its calls as pure or discard an unused state-changing operation.

This primitive does not establish general memoization or initialization-order
semantics for source globals. Such a language-wide change needs its own design
and evidence. There is no dynamic key API or shared cross-instance state.
