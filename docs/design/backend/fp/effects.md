# Effects

**Feature:** F-02  
**Status:** Draft (design)  
**Prerequisites:** [functional core](functional-core.md), [CC IR](cc-ir.md), and
[type classes and dictionaries](type-classes-and-dictionaries.md); monads,
closures, and the distinction between a value and a computation. Read
[IR boundaries](../00-ir-boundaries.md) first.  
**Summary:** `Effect a` is a first-class computation value represented as an
ordinary closure; `pure`, `bind`, and `runEffect` are ordinary functions and
the execution token is an internal argument threaded through a bind. Effects
therefore need no special node in CC or MIR and lower entirely through the
closure and product representation already used for closures, records, and
dictionaries.

## Scope

This document owns the representation of `Effect a` and the meaning of `pure`,
`bind`, and `runEffect`, including the execution token and effect sequencing. It
does not own the WASI services an effect may call
([WASI platform library](../wasm/wasi-platform-library.md)), the do/ado
desugaring that produces `bind` (frontend; `FE-05` in
[DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md)), or the
general closure and product representations ([CC IR](cc-ir.md),
[data representation](data-representation.md)).

## Background

An **effectful** operation such as printing must be *described* before it is
*performed*. The standard way to separate description from performance is the
**monadic translation** (Wadler, *Monads for Functional Programming*, 1992/1995;
Moggi, *Notions of Computation and Monads*, 1991): an effectful computation has
a type `M a`, `pure` embeds a value, `bind` sequences a computation with a
continuation, and a distinguished runner performs the computation. PureScript's
`Effect` is the special case where the only observer is the runtime: `Effect a`
is a synchronous computation that yields an `a` when run.

Wadler's translation gives semantics; **call-by-push-value** (Levy, *Call-by-Push-Value: A Subsuming Paradigm*, 1999/2004)
gives the type-theoretic picture that this design reserves for growth. CBPV
separates **values** (thunks, functions) from **computations** and makes the
mapping `U`/`F` between them explicit. The current synchronous `Effect` is a
single computation former, so the monadic presentation suffices; if the effect
language later distinguishes more computation forms, the CBPV view is the
intended generalization, and it should remain contained in the functional core
and the platform layer.

PureScript's official `Effect a` is close to a thunk: a value that performs work
when supplied with a token. In this repository the embedded `Prelude` defines
`type Effect a = Boolean -> a`, and the type checker elaborates `Effect a` to
that function type. That spelling is a placeholder, not the final
representation; the semantics it encodes (a computation value that runs only
when applied to a token) is the contract this document fixes.

## Model

### Surface and semantics

The platform-independent `Prelude` owns the effect vocabulary:

```purescript
type Effect a = Boolean -> a      -- current placeholder spelling; see Design

pure :: forall a. a -> Effect a
bind :: forall a b. Effect a -> (a -> Effect b) -> Effect b
runEffect :: forall a. Effect a -> a
```

Semantically, an `Effect a` is a computation value and the token is its
execution context:

```text
Effect a         ≈ Token -> a            (a computation value)
pure v            = \_ -> v
bind m k          = \t -> k (m t) t
runEffect m       = m token               (token from the runtime)
```

- **Construction is inert.** Building an `Effect` value allocates a closure and
  performs no operation; merely storing or passing it does not run it.
- **Running is explicit.** Only `runEffect` supplies the token; each explicit
  `runEffect` performs the computation. A stored effect run twice runs twice.
- **Sequencing is left to right.** `bind` runs the first computation, then feeds
  its result to the continuation with the same token, so operations keep source
  order.
- **The token is opaque.** It is internal to the runtime and the effect
  machinery; it is not part of the source-visible API, even though the current
  spelling names a `Boolean`.

### Elaboration and invariants

The frontend treats `Effect` as a built-in type constructor and does not add a
platform field to CST, AST, HIR, or Core nodes
(`psrs_hir::BuiltinType::Effect`). The checker elaborates `Effect a` to a
function type so imported polymorphic signatures stay instantiable.

- `Effect a` is a first-class value of an ordinary type.
- No stage stores an "effect" flag on an expression; effects compose only
  through `pure`, `bind`, and `runEffect`.
- The current token is representation-only: replacing `Boolean` with a richer
  token type must not change the source API or the Core shape of `pure`/`bind`.
- Reusing a value never reorders or merges observably distinct `runEffect`
  calls; each call receives a token.

## Design

### Chosen representation: closures and dictionary passing

`Effect a` lowers as an ordinary closure:

- `pure v` becomes a closure that ignores its token and returns `v`.
- `bind m k` becomes a closure that applies `m` to the token, passes the result
  to `k`, and applies the resulting effect to the same token.
- `runEffect m` applies `m` to the runtime token.

There is no special `Effect` node in CC or MIR. The values are closures
([CC IR](cc-ir.md)) and the applications are ordinary direct or closure calls.
If the effect language grows beyond a single synchronous computation, the design
moves to **dictionary passing** in the sense of
[type classes and dictionaries](type-classes-and-dictionaries.md): an effect
becomes a record/closure over its operation implementations and
`runEffect` interprets it. That is a change of the operation set, not of the
lowering mechanism, and it still introduces no dedicated CC/MIR node.

The current token is a placeholder. The final token type is chosen by the
runtime and carries whatever state effects need — scheduling, resource handles,
or an interpreter state — and remains invisible to source and to Core.

### Partial application

The surface form `runEffect (log "message")` requires `log "message"` to be an
`Effect Unit` when `log :: String -> Effect Unit`. In the current placeholder spelling
`log` is a two-argument function (`message`, then `token`), so `log "message"`
is a **partial application**. The backend implements partial application of a
top-level function by generating a closure that captures the supplied arguments
and, when invoked, calls the original function with the captures followed by the
remaining parameters (`cc/lower/call.rs::lower_partial_global_application`). No
eta-expansion or arity special case is needed at the source of an effect.

### Rejected alternatives

- **A dedicated effect node in CC or MIR.** Rejected: an effect is a closure and
  a bind is a call; a dedicated node would couple the backend to the effect
  library, add a verifier path, and prevent effects from composing as ordinary
  values.
- **Performing an operation when it is constructed (eager effects).** Rejected:
  it breaks the user-visible contract that constructing an effect has no
  observable result and that an effect runs once per `runEffect`.
- **`Effect a = Unit -> a` with no token.** Rejected as the final model: a
  unique token keeps distinct runs distinct, so an engine cannot merge or
  eliminate two otherwise identical `runEffect` calls, and the token is the
  natural place to thread runtime state.
- **Full CBPV representation now.** Rejected for the current synchronous
  `Effect`: it would add value/computation distinctions the source does not need
  yet. The CBPV view is retained as the growth path if the effect language gains
  multiple computation forms ([functional core](functional-core.md)).
- **Executing effects inside the compiler's constant evaluator or driver.**
  Rejected: only the runtime performs effects; the compiler preserves the
  program as a value.

## Algorithms

### Lowering `pure`, `bind`, and `runEffect`

These are ordinary definitions in the embedded `Prelude`, so no compiler pass is
effect-specific; they lower through lambda and application lowering:

```text
pure  = \value -> \token -> value
bind  = \first -> \next -> \token -> next (first token) token
runEffect = \action -> action true
```

A lambda becomes a closure capturing its free variables (`cc/lower/lambda.rs`),
and an application becomes a direct or closure call (`cc/lower/call.rs`,
`mir/lower/assignments.rs`). The sequencing in `bind` is the sequencing of
argument evaluation in CC: the argument `first token` is evaluated before the
continuation call, so the first effect completes before the second begins.

### Partial application of an effect operation

```text
lower_partial_global_application(f, supplied_arguments, expression):
    captured = [lower(arg) for arg in supplied_arguments]   // in order
    generated = fresh function with parameters:
        (closure, remaining_parameters...)
    body:
        captured_i = ClosureGetCapture(closure, i)
        result     = DirectCall(f, captured ++ remaining_parameters)
    closure = FunctionRef(generated, target_signature, captures = captured)
    return closure
```

The generated function is verified like any other; its closure type is the
target signature of the partially applied expression.

### Sequencing across `let` and `do`

A `let`-bound effect is lowered in ANF order, so `let a = runEffect e1 in
runEffect e2` runs `e1` before `e2`. `do`/`ado` desugaring (frontend, `FE-05`)
rewrites to `bind` before Core, so the backend sees only `pure`, `bind`,
`runEffect`, and ordinary calls.

### Edge cases

- **Stored effect run twice.** Each `runEffect` applies a fresh token, so both
  runs execute; the closure itself is unchanged.
- **Effect captured by a closure.** The closure capture path handles an effect
  value like any other reference; no special case.
- **Effect returned by a function.** It is a closure produced by `bind` or an
  operation; the caller decides whether to run it.
- **`bind` with a continuation that ignores its argument.** Still sequenced; the
  first computation runs before the continuation.
- **Polymorphic effect.** `Effect a` with an erased `a` uses the erased
  protocol; `runEffect`'s consumer knows the concrete type
  ([polymorphism and erasure](polymorphism-and-erasure.md)).
- **A future richer token.** Changing the current `Boolean` to a stateful
  token changes only `Prelude` and the runtime, not the Core shape or the CC/MIR
  operations.

## Code map

The effect vocabulary is frontend source and the lowering path is the ordinary
closure and partial-application path. The implementation must conform to this
organization.

```text
crates/psrs-hir/src/ty.rs
crates/psrs-typecheck/src/
  builtins.rs
  signature.rs
crates/psrs-core/src/effect.rs
crates/psrs-driver/src/prelude.rs
crates/psrs-backend/src/
  cc/lower/lambda.rs
  cc/lower/call.rs
  mir/lower/assignments.rs
```

Responsibilities and required types:

- The embedded `Prelude` (`psrs-driver/src/prelude.rs`) must define the required
  surface names `Effect`, `pure`, `bind`, and `runEffect` as ordinary source
  declarations. `Effect` must remain the single built-in computation former;
  `pure`, `bind`, and `runEffect` must lower as ordinary lambdas and
  applications with no effect-specific compiler pass.
- `psrs-hir/src/ty.rs` must expose `BuiltinType::Effect` as a built-in type
  constructor. No CST, AST, HIR, or Core node may carry an effect flag or
  platform-specific effect field.
- `psrs-typecheck` must elaborate `Effect a` to a function arrow so imported
  polymorphic signatures stay instantiable, and must reject running an effect
  in a pure position. Required entry point in `signature.rs`:
  `fn elaborate_effect(ty, env) -> Result<Type, Diagnostic>`; `builtins.rs` must
  register `Effect` as a built-in type constructor.
- `psrs-core/src/effect.rs` must ensure the elaborated `pure`, `bind`, and
  `runEffect` shapes are ordinary Core values (closures and calls); it must not
  add an effect-specific Core node.
- The backend must lower effects through the closure path. `cc/lower/lambda.rs`
  must create the closures for `pure` and `bind`; `cc/lower/call.rs` must
  provide the partial-application entry point
  `lower_partial_global_application(f, supplied_arguments, expression)` used when
  an effect operation is applied to fewer than its runtime arguments;
  `mir/lower/assignments.rs` must create closures (`FunctionRef`) and perform
  closure calls (`IndirectCall`). No CC or MIR node may be effect-specific.
- The representation target is closure passing now, growing to dictionary
  passing if the operation set becomes extensible
  ([type classes and dictionaries](type-classes-and-dictionaries.md)). The token
  is internal: it must not appear in the surface API, and its type is chosen by
  the runtime, not by the effect library.
- The WASI operations effects call are owned by the
  [WASI platform library](../wasm/wasi-platform-library.md) and lowered through
  the [canonical ABI and WIT](../wasm/canonical-abi-and-wit.md); this tree owns
  only the effect vocabulary and its lowering.

## Invariants and verification

- Constructing an effect performs no call into a WASI import; this is tested by
  observing no output when an effect is built and never run.
- Running effects preserves source order; the ordering derives from ANF
  assignment order, so the CC verifier's ordering checks cover it.
- A stored effect runs once per explicit `runEffect`; two runs produce two
  observable effects.
- Dictionaries or closures carrying effects have ordinary shapes; the CC and MIR
  verifiers check them as products and closures, not as effects.
- Execution evidence requires `wasmtime`; the tests skip when the runtime is
  absent and run in CI under `PSRS_REQUIRE_WASMTIME=1`, as required by
  [DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md).

## Worked example

```purescript
main =
  let first  = runEffect (log "first")
  in  let second = runEffect (log "second")
  in  0
```

`log :: String -> Effect Unit` is a two-parameter function after lowering
(`message`, then `token`). Lowering proceeds as:

```text
// runEffect (log "first")
captured   = Constant "first"
partial_fn = partial_<span>(closure, token) {
    msg    = ClosureGetCapture(closure, 0)
    result = DirectCall(log, [msg, token])       // the original two-parameter `log`
}
action1    = FunctionRef(partial_fn, signature = Boolean -> Unit, captures = [captured])
result1    = DirectCall(runEffect, [action1])       // runEffect applies action1 to true
```

The second `runEffect` lowers identically into the following assignment, so the
ANF order runs `"first\n"` before `"second\n"`. Constructing `action1` alone
would allocate a closure and print nothing, which is exactly the test
`constructing_an_effect_does_not_execute_it`.

## Boundaries and interfaces

- **From the frontend.** `Effect` is a built-in type constructor; the checker
  elaborates it and `do`/`ado` desugars to `bind`. No effect flag is added to
  CST, AST, HIR, or Core nodes.
- **To CC.** Effects are closures and applications; the only backend feature an
  effect needs is partial application of a top-level function.
- **To MIR/Wasm.** Ordinary closure creation and calls; the token type is a MIR
  value type chosen by the runtime, not by the effect library.
- **To the platform layer.** WASI calls are made only when the effect is run;
  the platform library exposes the operations, and the backend lowers them
  through the canonical ABI ([canonical ABI and WIT](../wasm/canonical-abi-and-wit.md),
  [DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md)).
- **Not owned.** Effect operation vocabulary (platform library), `do`
  desugaring (frontend), and the runtime's scheduler or resource model.

## Open questions and future work

- **Final token type.** The current placeholder uses `Boolean`; the runtime and the
  platform layer must agree on a token that carries scheduling and resource
  state once effects need it.
- **Effect dictionary.** If the operation set grows and becomes extensible,
  effect operations should be dictionary-passing records, with `runEffect` an
  interpreter; this is defined by
  [type classes and dictionaries](type-classes-and-dictionaries.md), not here.
- **Asynchronous effects.** WASI 0.3 async streams/futures are a separate
  platform track (`BE-25`); CBPV is the intended semantic home if and when
  async computation forms arrive.
- **Optimization.** Inlining `pure`/`bind` and eliminating the token on
  statically known computations belongs to the MIR optimization track
  (`BE-12`) and must preserve the run-once-per-`runEffect` behavior.
- **Diagnostics.** A source program that runs an effect in a pure position is a
  type error produced by the checker, not by the backend.

## Implementation notes

In progress. The embedded `Prelude` defines
`type Effect a = Boolean -> a` with `pure`, `bind`, and `runEffect` as ordinary
source functions, and the checker expands `Effect a` to a `Boolean -> a`
function type; there is no abstract effect type. Partial application of a
top-level function is implemented in `cc/lower/call.rs`; the generated function
is verified like any other. Construct/run/order behavior is covered by the
`psrs-driver` effect tests under `wasmtime`. The Boolean token is explicitly a
placeholder; nothing in this document depends on it.

## References

- Moggi, E., *Notions of Computation and Monads* (1991).
- Wadler, P., *Monads for Functional Programming* (1992/1995).
- Levy, P. B., *Call-by-Push-Value: A Subsuming Paradigm* (1999) and
  *Call-by-Push-Value* (2004).
- [CC IR](cc-ir.md): closures, captures, and partial application.
- [type classes and dictionaries](type-classes-and-dictionaries.md): the
  dictionary form the effect representation may grow into.
- [WASI platform library](../wasm/wasi-platform-library.md),
  [canonical ABI and WIT](../wasm/canonical-abi-and-wit.md), and
  [DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md): the
  platform boundary effect operations use.
- [DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md): runtime
  execution evidence.
