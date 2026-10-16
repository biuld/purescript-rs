# DEC-19 — Compiler-Generated Resumable Frames for Generic Handlers

**Status:** Accepted  
**Date:** 2026-10-11  
**Amends:** [DEC-03](DEC-03-purescript-faithful-type-system.md), the clause that
keeps delimited continuations out of every IR.  
**Governed by:** [F-02](../feature/F-02-portable-programs.md),
[delimited control and resumable frames](../design/backend/fp/delimited-control-and-resumable-frames.md),
and [library-owned algebraic effects](../design/backend/fp/algebraic-effects-and-handlers.md).

## Context and constraints

A portable program needs a way for library code to define effectful operations
and the handlers that interpret them. [DEC-03](DEC-03-purescript-faithful-type-system.md)
chose to encode effects as a library (`purescript-run` style) and explicitly kept
"effect rows, handlers, or delimited continuations" out of CST, AST, HIR, THIR,
and Typed Core. The compiler-owned `Effect`/`State` mechanism was removed on
2026-10-11, leaving no replacement representation.

A library-only encoding over a free/freer monad is portable and needs no compiler
change, but it forces every effectful computation through an explicit monadic
value and depends on library trampolining for stack safety. A generic mechanism
that can suspend a computation and capture the remainder — delimited control —
provides compiler-owned continuation sequencing while keeping every concrete
operation, row, and handler in library code. The selected `Control` surface is
monadic; it does not provide direct-style syntax or capture arbitrary pure calls.
[DEC-20](DEC-20-typed-control-elimination.md) clarifies its typed elimination and
clause-scope contract.

Delimited control has exactly two viable implementations on this target:

- **Compiler-generated resumable frames.** The compiler lowers the regions that
  can suspend into an explicit heap-allocated frame chain (a defunctionalized CPS
  / abstract machine) driven by a loop. This uses only features the pinned profile
  already enables: Wasm GC, typed function references, and tail calls.
- **Target-native stack switching.** Lower suspension to the WebAssembly
  stack-switching proposal (`cont`/`suspend`/`resume`) and let the runtime capture
  the guest execution stack.

A plain host import cannot implement either. A Rust/Wasm function called by the
guest cannot capture the caller's Wasm execution stack; Rust's linear-memory stack
is a different stack, so "runtime support" is not a normal binding.

## Decision

Adopt **compiler-generated resumable frames** as the generic, effect-agnostic
handler and interruption mechanism.

- The compiler owns a small, abstract control surface: a suspension primitive, a
  delimiter/handler boundary, and an abstract typed resumption. No pass recognizes
  a concrete operation name (`State`, `Console`, `Run`, `Effect`), and the type
  system gains no effect rows or handlers.
- The lowering is a **selective, defunctionalized CPS**: only computations that
  can reach a suspension are converted; the continuation is an explicit chain of
  GC frame objects resumed by a trampoline, so execution is stack-safe
  independently of the native stack.
- Concrete operations, rows, and handlers remain in the independent standard
  library and user libraries, built on the control surface. The compiler does not
  own their meaning.
- **Do not adopt WebAssembly stack switching now.** It is a Phase 3 proposal and
  not merged into the core specification. Wasmtime disables it by default
  (`Config::wasm_stack_switching = false`), it depends on the `exceptions`
  feature, and the pinned profile disables `exceptions`. Revisit it as a
  target-profile revision only when it reaches Phase 4/5 and the pinned runtime
  enables it by default; until then the artifact contract must not depend on it.

### Amendment to DEC-03

DEC-03's prohibition of delimited continuations in the IRs is narrowed: a
*generic, effect-agnostic* control mechanism may be lowered below Typed Core (in
the backend MIR), and a small abstract control surface may appear at the boundary
that the library builds on. DEC-03's type-system decision is retained: there are
no frontend effect rows, no compiler-known handlers, and no compiler-native effect
system; effects remain a library encoding. No effect tracking or effect reasoning
is added to the type system: the compiler does not infer, propagate, or solve
effect requirements, and does not reason about which effects a function may
perform.

## Alternatives considered

- **WebAssembly stack switching (rejected for now).** It is a Phase 3 proposal,
  disabled by default in the pinned runtime, depends on a second disabled feature
  (`exceptions`), and would tie the artifact contract to a moving proposal and a
  non-default runtime configuration. It remains a possible future accelerator
  behind an explicit profile revision.
- **Library-only free monad (retained as the portable baseline, not sufficient
  alone).** It needs no compiler change and remains valid for pure handlers, but
  its stack safety is a library trampoline, not a compiler guarantee.
  The selected compiler surface also uses explicit monadic computations; arbitrary
  non-monadic interruption is not established by this decision.
- **Host-imported stack capture (rejected as impossible).** A Wasm import cannot
  capture its caller's execution stack.
- **A compiler-native effect system (rejected, unchanged from DEC-03).** Adding
  effect rows and handlers to the type system is a language change DEC-03
  rejected; this record does not revive it.

## Consequences

- The backend gains a new lowering obligation: selective CPS/defunctionalization,
  an explicit frame representation, a trampoline, and verification that frame
  identity, capture boundaries, and resumption consumption are preserved. Frames
  are GC values, so captured roots are collected by the engine.
- The target profile is unchanged: GC, typed function references, and tail calls
  already cover the mechanism; no new capability flag is required.
- The native mechanism and the portable library encoding are distinct and must not
  be silently interchanged; runtime one-shot and library multi-shot behavior stay
  distinguishable.
- Delimited control is powerful and dangerous. Suspension across a non-suspendable
  host/foreign frame, multi-shot replay of mutable state, and continuation
  lifetime are explicit contracts, not automatic guarantees.
- The design document
  [`delimited-control-and-resumable-frames.md`](../design/backend/fp/delimited-control-and-resumable-frames.md)
  is the source of truth for the mechanism, its lowering, and the comparison with
  stack switching. Implementation follows it.
