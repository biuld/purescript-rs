# D-13 — Effect-Based Platform Library

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md), issue #8
**Status:** Bootstrap implementation

## Purpose

Platform services must describe effects in their source-level types. A service
such as console output must not run merely because a value was constructed, and
the platform-independent Prelude must not expose WASI-specific bindings.

## Surface boundary

`Prelude` owns the platform-independent effect vocabulary:

```purescript
type Effect a = Boolean -> a
pure :: forall a. a -> Effect a
bind :: forall a b. Effect a -> (a -> Effect b) -> Effect b
runEffect :: forall a. Effect a -> a
```

The compiler treats `Effect` as a built-in type constructor and elaborates
`Effect a` to the bootstrap function representation. This keeps imported
polymorphic signatures instantiable while avoiding a platform field on CST,
AST, HIR, or Core nodes.

Platform modules own their WIT imports and expose effectful operations:

```purescript
module WASI.Console where
log :: String -> Effect Unit
error :: String -> Effect Unit

module WASI.Clock where
now :: Effect Int
```

Programs opt into these capabilities with explicit imports. Prelude remains
portable and does not import WASI interfaces.

## Bootstrap semantics

An effect constructor returns a closure. The closure does not call its WIT
imports until it is passed to `runEffect`. `bind` calls the first closure with
the same execution token, then passes its result to the next closure, preserving
left-to-right sequencing. Reusing a stored effect and calling `runEffect` twice
executes it twice; merely storing or passing it does not execute it.

The Boolean token is only the current bootstrap representation. It is not a
public runtime contract and does not provide scheduling, cancellation,
resource management, or asynchronous execution. Those concerns require a
separate effect runtime design.

## Lowering requirement

The backend supports partial application of top-level functions by generating a
closure that captures the supplied arguments and calls the original function
when the remaining effect arguments arrive. This permits the source-level form
`runEffect (log "message")` while preserving the ordinary top-level function
calling convention.

## Validation

The driver tests verify that constructing an effect has no output, sequential
`runEffect` calls preserve source order, and a stored effect runs once per
explicit execution. Wasmtime execution remains optional locally and is required
in CI by the repository's runtime validation workflow.
