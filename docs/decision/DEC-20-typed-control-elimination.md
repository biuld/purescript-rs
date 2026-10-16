# DEC-20 — Typed Source-to-Target Control Elimination

**Status:** Accepted  
**Date:** 2026-10-11  
**Clarifies:** [DEC-19](DEC-19-compiler-generated-resumable-frames.md)  
**Governed by:** [F-02](../feature/F-02-portable-programs.md) and
[control elimination and handler scopes](../design/backend/fp/control-elimination-and-handler-scopes.md).

## Context and constraints

The draft five-argument `handle` is an open state fold: its clause returns a
source computation, which the driver interprets under the same handler. That
cannot implement fresh clause requests outside the handler while preserving its
source and target types. The retained ClauseScope program exposes the mismatch.
The design also promises library-selectable deep/shallow handlers, multi-shot
replay, delimiter expiry and a flat guest machine. A dispatch patch or mode flag
cannot repair the missing target-answer contract.

## Decision

Generalize the existing `Prim.Control.handle` to take a return clause, a rank-2
operation clause, and a source computation. Its raw continuation returns the
original source computation; both clauses return a distinct target computation.
No new intrinsic is added. State and union projection are ordinary library
closures; deep code reinstalls `handle`, shallow code uses the raw remainder.
Shallow output retains the source effect collection unless library code explicitly
transforms it. The existing `run` remains a separate closed state fold.

Source dispatch ends before clause execution. Its lifetime extent remains live
until the target answer finishes. Explicit heap contexts preserve this distinction,
caller capture boundaries, and replay activation identities. Tail finalization
coalescing may preserve common completion boundaries but cannot erase independently
observable expiry. The software-frame architecture and target profile of DEC-19
remain unchanged; the selected source surface is monadic, not direct style.

## Consequences

This revises an unpublished draft API: `ControlHandle` keeps identity 96, but its
checked scheme and arity change. Compiler registry, verification, layouts, lowering,
independent package and acceptance cases require a coherent migration and explicit
artifact/package compatibility invalidation. Existing v1 execution evidence is
retained as historical evidence and cannot certify v2 scope semantics.

Library expressibility is supported by official ordinary-language type checking
and a rewrite model; lifetime, GC, native stack and foreign cleanup still require
real target execution. The full contract is in the linked design; the
[checklist](../implementation/backend/algebraic-effects.md) records migration and
remaining verification. This decision does not add frontend effect typing or
claim acceptance of the upstream Run library.
