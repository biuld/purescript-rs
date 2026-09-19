# DEC-03 — PureScript-Faithful Type System and Effect Encoding

**Status:** Accepted  
**Date:** 2026-09-19

## Context

[F-02](../feature/F-02-portable-programs.md) commits the compiler to PureScript
language semantics, and [D-01](../design/D-01-frontend-and-ir-boundaries.md)
and [D-02](../design/D-02-wasm-lowering.md) plan the type system in stages:
monomorphic types, Hindley–Milner inference, algebraic data types, kinds and
row-polymorphic records, then type classes and explicit dictionaries.

Host services are currently modeled with Rust enums: `Intrinsic` and
`RuntimeFunction` in `psrs-hir`, hardcoded types in the type checker, and
hardcoded semantics in Core and the backend. `log` is typed `String -> Unit`,
which erases its effect.

Two directions were considered for the standard library and effects:

1. A compiler-native algebraic effect system: built-in effect rows and handlers,
   with the compiler shipping a minimal effect-based standard library.
2. Following PureScript's type system and encoding effects as a library.

PureScript's type system has no native algebraic effects, no effect handlers,
and no first-class effect rows. Its `Effect a` is an FFI/IO monad, not
algebraic effects. Algebraic effects are encodable as a library — a free/freer
monad over row-typed effects, as in `purescript-run` — but that encoding
requires higher-kinded types, type classes with instances, row polymorphism,
rank-N types, algebraic data types, and closures. Those are exactly the staged
PureScript features already planned.

## Decision

Follow PureScript's type system as the compiler's type-system direction, and
encode effects the PureScript way as a library. Do not add a compiler-native
algebraic effect system to the core IRs.

Specifically:

- Grow the type system in PureScript's order: rank-1 polymorphism, then
  higher-kinded types and kinds, algebraic data types and pattern matching,
  type classes with explicit dictionaries, row polymorphism, and rank-N types.
- Encode effects later as a library (free/freer monads over row-typed effects,
  `purescript-run` style). The compiler does not add effect rows, handlers, or
  delimited continuations to CST, AST, HIR, THIR, or Typed Core.
- Host functions are registry-backed data (`psrs_hir::runtime`: name, symbol,
  and type) plus library declarations, not a `RuntimeFunction` enum, and the
  backend lowers them to WASI by name. The `Intrinsic` set stays for compiler
  primitives such as integer operators. This keeps the two-layer split of
  [D-02](../design/D-02-wasm-lowering.md): PureScript-facing standard library →
  WASI interfaces ([DEC-06](DEC-06-runtime-interface-via-wit.md)).
- The compiler-native algebraic-effect standard library proposed earlier is
  rejected.

See [D-03](../design/D-03-type-system.md) for the staged type-system plan.

## Alternatives considered

- **Compiler-native algebraic effects (rejected).** It deviates from
  PureScript, conflicts with the planned class/row elaboration, and still needs
  a data-driven host ABI registry, so it would add a new effect mechanism
  without removing the registry work. The intended library encoding needs most
  of the PureScript type system anyway.
- **PureScript's `Effect` monad alone (rejected as incomplete).** It models
  host IO but not user-definable effects or handlers; the algebraic-effect
  library sits on top of it.
- **Keep the enum runtime functions (rejected).** It spreads each service
  across HIR, resolution, type checking, Core, and the backend, and cannot
  express effectfulness without another special case.

## Consequences

- Effect-level compatibility with official PureScript remains the goal; the
  compiler does not invent a competing effect system.
- A large type-system program precedes any algebraic-effect library: rank-1
  polymorphism, kinds, ADTs, classes, and rows.
- Until the backend gains type erasure and dictionary passing, it rejects
  programs whose checked types it cannot lower, with source-oriented
  diagnostics. Rejecting at the backend is a bootstrap limitation, not a
  type-system rule.
- Host services become registry entries and library declarations rather than
  compiler-name special cases; the Wasm emitter consumes the registry only.
- Trade-offs: algebraic effects arrive later than a native shortcut would
  allow, and the standard library must respect PureScript's type-class and row
  semantics.
- Follow-up: record the type-system design in
  [D-03](../design/D-03-type-system.md), and update
  [D-01](../design/D-01-frontend-and-ir-boundaries.md) and
  [D-02](../design/D-02-wasm-lowering.md) when the type representations change.
