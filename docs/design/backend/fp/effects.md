# Effects

**Feature:** F-02  
**Status:** Draft (design)  
**Prerequisites:** [functional core](../../frontend/semantics/functional-core.md), [CC IR](cc-ir.md), and
[type classes and dictionaries](type-classes-and-dictionaries.md); monads,
closures, and the distinction between a value and a computation. Read
[IR boundaries](../00-ir-boundaries.md) first.  
**Summary:** `Effect a` stays an abstract library type through type checking and
Typed Core. One representation lowering then replaces each effect value with a
closure that takes the runtime token and returns the lowered result. That
closure is not a source arrow, so a function or another effect inside the
result stays a separate call. `pure`, `bind`, and `runEffect` are ordinary
source functions; only this lowering threads the token.

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
when the runtime supplies a token. This design keeps that token out of the
source type. Type checking treats `Effect` as an abstract library constructor.
One later translation is the only place that recognizes it, and the closure it
emits is what CC and MIR call. Source programs cannot forge the token or treat
an ordinary function as an effect.

## Model

### Surface and semantics

The effect library owns the source vocabulary. Its visible signatures include:

```purescript
foreign import data Effect :: Type -> Type

pure :: forall a. a -> Effect a
bind :: forall a b. Effect a -> (a -> Effect b) -> Effect b
runEffect :: forall a. Effect a -> a
```

`runEffect` is provided only to the selected command entry as specified by
[F-02](../../../feature/F-02-portable-programs.md). It has an ordinary function
type; entry authorization is a driver rule, not an inference rule.

The equations below are the representation translation, not source equalities.
`Token` does not occur in a source type, and `Effect a` does not unify with
`Token -> a`.

```text
⟦Effect τ⟧ = RepClosure([Token], ⟦τ⟧)
pure v      = RepClosure(token) { v }
bind m k    = RepClosure(token) { call (call k (call m token)) token }
runEffect m = call m runtimeToken
```

- **Construction is inert.** Building an `Effect` value allocates a closure and
  performs no operation; merely storing or passing it does not run it.
- **Running is explicit.** The elaborated `runEffect` supplies the token. Only
  the selected command entry can reference `runEffect`; an effect invoked twice
  there runs twice.
- **Sequencing is left to right.** Elaborated `bind` runs the first computation,
  then feeds its result to the continuation with the same token, so operations
  keep source order.
- **The token is not a source value.** No source program names it, applies an
  effect to it, or passes an ordinary function where an effect is required.

### Two callable forms

A source function and an effect closure are different callable values.

```text
SourceArrow a b     = the curried Function spine a -> b
RepClosure params r = a closure created with a fixed parameter list and a
                      result value
```

Curried-arrow flattening collects the domains of one `SourceArrow` group and
stops when the result is not an arrow. A `forall` in that result is already a
separate closure. A `RepClosure` is the same kind of boundary: its parameter
list is fixed when it is created, and its result is returned as a value.

```text
⟦Effect (a -> b)⟧       = RepClosure([Token], SourceArrow a b)
⟦Effect (Effect a)⟧     = RepClosure([Token], RepClosure([Token], ⟦a⟧))
⟦String -> Effect Unit⟧ = SourceArrow String (RepClosure([Token], Unit))
```

Running `Effect (a -> b)` passes `Token` and receives a function. Applying that
function is a second call. The effect's arity is one. The function's arity is
the arity of `a -> b`. `Effect (Effect a)` likewise takes one token and returns
another effect closure; the inner effect takes its own token when it is run.

`log :: String -> Effect Unit` therefore has source arity one. `log "message"`
is a saturated call. It returns a `RepClosure`, and the token is that closure's
parameter, not a second parameter of `log`.

### Elaboration and invariants

The frontend resolves `Effect` as an imported abstract type constructor and
checks it by ordinary kind, application, and subsumption rules. It adds no
effect flag, private constructor, or representation mode.

- `Effect a` is `Application(Constructor(User(effect_id)), a)` on the uniform
  application spine ([DEC-15](../../../decision/DEC-15-unified-type-representation.md)).
  THIR and Typed Core keep that type. Unification, subsumption, and arity
  flattening do not treat it as a function.
- No stage stores an "effect" flag on an expression. Effects compose through
  `pure`, `bind`, and `runEffect`.
- Exactly one pass recognizes `effect_id`: the representation lowering below.
  After it, CC and MIR see ordinary closures and calls. They do not consult a
  callable-constructor table and they do not match `Effect`.
- The token's physical type is chosen by that pass. The synchronous runtime
  uses an `i32` placeholder. Changing it changes the pass and the runtime, not
  the source API or the Core type of `pure` and `bind`.
- Reusing a value never reorders or merges observably distinct `runEffect`
  calls; each elaborated call receives a token.
- The driver identifies the selected command entry and checks the scope of
  `runEffect` references before ordinary type inference.
- Calls, including calls reached through closures, may perform effects. Core,
  CC, and MIR optimizations preserve their order and multiplicity unless a
  separate purity proof establishes that a particular call is inert.

## Design

### Chosen representation: one closure translation

Representation lowering runs after Typed Core and before curried-arrow
flattening. It is an ordinary pass: it consumes Core, where `Effect` is still
abstract, and produces CC closures with explicit parameter lists.

- A value of type `Effect τ` becomes `RepClosure([Token], ⟦τ⟧)`.
- `pure`, `bind`, and `runEffect` are foreign imports of those abstract
  signatures. The pass replaces them with the closures in the model. It does
  not emit a host call for them.
- A foreign import whose source type is `Effect τ` becomes a closure that
  performs the host call when the token is supplied. In a strict language the
  call must not happen when the effect value is built. `pure foreignCall` would
  evaluate the call too early, so the suspension is this closure, not `pure`.
- A source function such as `log :: String -> Effect Unit` keeps one source
  parameter. Its body is ordinary `bind` over effect-typed operations. The
  returned value is the `RepClosure` produced for that `Effect`.

There is no special `Effect` node in CC or MIR. The values are closures
([CC IR](cc-ir.md)) and the applications are ordinary direct or closure calls.
If the effect language grows beyond a single synchronous computation, the design
moves to **dictionary passing** in the sense of
[type classes and dictionaries](type-classes-and-dictionaries.md): an effect
becomes a record/closure over its operation implementations and
`runEffect` interprets it. That is a change of the operation set, not of the
lowering mechanism, and it still introduces no dedicated CC/MIR node.

The synchronous token may be the constant `i32` value `0`. The calls themselves
are observable and cannot be merged or removed; distinct token bits are not a
substitute for that optimizer rule. A later runtime may pass state or resource
handles without exposing the token to source programs.

### Partial application

Partial application is under-application of a source arrow. `map f` for
`map :: (a -> b) -> Effect a -> Effect b` captures `f` and waits for the effect.
`log "message"` is not that situation: `log` has one source parameter, the call
is saturated, and the result is an effect closure waiting for a token. The
token is never counted as a remaining parameter of `log`.

### Rejected alternatives

- **A dedicated effect node in CC or MIR.** Rejected: an effect is a closure and
  a bind is a call; a dedicated node would couple the backend to the effect
  library, add a verifier path, and prevent effects from composing as ordinary
  values.
- **Performing an operation when it is constructed (eager effects).** Rejected:
  it breaks the user-visible contract that constructing an effect has no
  observable result and that an effect runs once per `runEffect`.
- **A source-visible `Effect a = Token -> a` alias, or unifying the two.**
  Rejected: callers could construct and run effects as unrestricted functions.
  Preserving effectful calls is an optimizer invariant, not a consequence of
  hiding the token's bits.
- **A callable side table keyed by the `Effect` type identity.** Rejected: every
  arity, application, and verifier pass would stop when it sees that
  constructor, so the library type leaks into the calling convention. The
  representation lowering is the one conversion that knows `effect_id`; its
  output no longer contains an effect to recognize.
- **Folding the token into the source parameter list.** Rejected:
  `String -> Effect Unit` would become a two-parameter function, and
  `Effect (a -> b)` would become a call of arity two. The function inside the
  effect does not exist until the effect runs.
- **A type-checker mode that accepts a token lambda at type `Effect a`.**
  Rejected: the lambda's type would be an arrow, or the checker would record
  an arrow's term under the abstract type. Either choice makes representation
  part of type equality.
- **Full CBPV representation now.** Rejected for the current synchronous
  `Effect`: it would add value/computation distinctions the source does not need
  yet. The CBPV view is retained as the growth path if the effect language gains
  multiple computation forms ([functional core](../../frontend/semantics/functional-core.md)).
- **Executing effects inside the compiler's constant evaluator or driver.**
  Rejected: only the runtime performs effects; the compiler preserves the
  program as a value.

## Algorithms

### Lowering `pure`, `bind`, and `runEffect`

The source declarations are abstract. Representation lowering replaces their
bodies; the type checker does not see a token.

```text
lower_effect(Effect τ) = RepClosure([Token], lower(τ))

lower_pure = \value -> RepClosure(token) { value }
lower_bind = \first -> \next -> RepClosure(token) {
    result = call (call first token)
    rest   = call (call next result)
    call rest token
}
lower_run  = \action -> call action runtimeToken
```

`call` of a `RepClosure` passes exactly that closure's parameter list. It does
not continue into a function or effect stored in the result. Sequencing is the
order of these calls: `call first token` completes before `call next result`.

### Partial application of a source arrow

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

- **Stored effect run twice.** Each `runEffect` calls the closure once, so both
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
- **A future richer token.** Changing the token to a stateful value changes the
  representation lowering and the runtime, not the source API or the CC/MIR
  operation families.

## Code map

The effect vocabulary is frontend source and the lowering path is the ordinary
closure and partial-application path. The implementation must conform to this
organization.

```text
crates/psrs-typecheck/src/          ordinary checking of User(effect_id)
crates/psrs-core/src/effect/        lower_effects(module) -> EffectLowering
crates/psrs-driver/src/prelude.rs   entry-only runEffect
crates/psrs-backend/src/cc/         closures and calls, with no Effect match
crates/psrs-backend/src/mir/
```

Responsibilities and required types:

- The effect library supplies `foreign import data Effect :: Type -> Type`,
  `pure`, `bind`, and the trusted `runEffect` binding, all at the abstract
  signatures in the model. The driver enforces entry-only `runEffect`.
- `psrs-hir` retains the resolved `Effect` type identity. No CST, AST, HIR, or
  Core node carries an effect flag or a token type.
- `psrs-typecheck` checks `Effect a` as an ordinary abstract application.
  It has no `Effect` constructor, no mode that opens an effect into an arrow,
  and no unification of `Effect a` with a function.
- `lower_effects(module)` is the only function that matches `effect_id`. It
  replaces `Effect τ` with `RepClosure([Token], lower(τ))`, replaces `pure`,
  `bind`, and `runEffect`, and suspends a foreign import of type `Effect τ`
  inside that closure. It records every closure it wrote and checks that record
  before returning. `EffectLowering::verify` rejects a recorded node whose
  parameter list is not `[Token]` or whose result is not `lower(τ)`. It does
  not add a Core or CC effect node.
- CC and MIR lower the resulting closures through `FunctionRef` and direct or
  indirect calls. Curried-arrow flattening reads source `Function` spines only.
  Partial application (`lower_partial_global_application`) applies to
  under-applied source arrows, not to the token of an effect.
- The representation may later grow into dictionary passing
  ([type classes and dictionaries](type-classes-and-dictionaries.md)). The token
  stays inside `lower_effects`; its type is chosen there.
- WASI operations are owned by the
  [WASI platform library](../wasm/wasi-platform-library.md) and the
  [canonical ABI and WIT](../wasm/canonical-abi-and-wit.md). An operation whose
  source type is `Effect` is suspended by `lower_effects` and performs its host
  call only when that closure runs.

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

`log :: String -> Effect Unit` has one source parameter. Lowering proceeds as:

```text
// runEffect (log "first")
action1 = DirectCall(log, ["first"])
          // result: RepClosure([Token], Unit)
result1 = call action1 runtimeToken
```

`log`'s body is `bind` over effect-typed writes. Each `bind` has already become
a `RepClosure`; running `action1` is what performs the writes. Building
`action1` allocates that closure and prints nothing.

The second `runEffect` lowers identically into the following assignment, so the
ANF order runs `"first\n"` before `"second\n"`. Constructing `action1` alone
would allocate a closure and print nothing, which is exactly the test
`constructing_an_effect_does_not_execute_it`.

## Boundaries and interfaces

- **From the frontend.** `Effect` is an imported abstract type constructor.
  `do`/`ado` desugars to library `bind`. No effect flag is added to CST, AST,
  HIR, or Core nodes, and P5 does not open the representation.
- **Through `lower_effects`.** This is the conversion from the abstract Core
  type to a CC closure. Downstream stages receive closures, not `Effect`.
- **To CC.** An effect is a closure whose parameter list is `[Token]`. Source
  partial application remains available for under-applied source arrows.
- **To MIR/Wasm.** Ordinary closure creation and calls. The token's MIR type is
  chosen by `lower_effects`.
- **To the platform layer.** WASI calls are made only when the effect is run;
  the platform library exposes the operations, and the backend lowers them
  through the canonical ABI ([canonical ABI and WIT](../wasm/canonical-abi-and-wit.md),
  [DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md)).
- **Not owned.** Effect operation vocabulary (platform library), `do`
  desugaring (frontend), and the runtime's scheduler or resource model.

## Open questions and future work

- **Stateful token.** The runtime and platform layer must agree on a token
  representation if scheduling or resource state is later threaded through it.
- **Effect dictionary.** If the operation set grows and becomes extensible,
  effect operations should be dictionary-passing records, with `runEffect` an
  interpreter; this is defined by
  [type classes and dictionaries](type-classes-and-dictionaries.md), not here.
- **Asynchronous effects.** WASI 0.3 async streams/futures are a separate
  platform track (`BE-25`); CBPV is the intended semantic home if and when
  async computation forms arrive.
- **Optimization.** Inlining `pure`/`bind` belongs to
  [Core optimization](../opt/core.md); token elimination after representation
  lowering belongs to [MIR optimization](../opt/mir.md). Both must preserve the
  run-once-per-`runEffect` behavior.
- **Diagnostics.** A source program that refers to `runEffect` outside the
  selected command entry receives a frontend diagnostic before Core lowering.

## Implementation notes

`lower_effects` in `crates/psrs-core/src/effect/` is the representation
conversion. Prelude declares `foreign import data Effect :: Type -> Type`.
`pure`, `bind`, and `runEffect` are abstract `psrs:effect` imports. The pass
returns `EffectLowering`, whose `synthesized` symbols are those three
declarations and whose `closures` are the `EffectClosure` entries it wrote. It
matches the opaque `Prelude.Effect` type id, replaces each application with
`Type::Closure` whose parameter list is Core `Int`, and suspends a foreign import
whose type ends in that application so the host call runs inside the closure.
`crates/psrs-backend/src/effects.rs` drops the abstract imports and performs
that suspension. The entry check lives in
`crates/psrs-driver/src/program/effects.rs` and looks up `runEffect` on
declarations and externals. `runEffect`'s synthesized body applies the integer
`0`.

`callable_types` remains a field on the typed module and is always empty.
Closure conversion and MIR do not read it. Calling convention uses the closure's
parameter list: `log` has one parameter, `log "message"` is a saturated call,
and `Effect (Int -> Int)` is not a two-parameter function
(`an_effect_of_a_function_is_not_arity_two_and_log_is_saturated`). A partial
application of `Boolean -> String -> Effect Unit` captures the Boolean once and
does not run the write (`a_partial_source_application_captures_once_and_defers_the_effect`).

Typed Core still shows `Effect Int` as a user-type application. A source
function is rejected by ordinary unification, and a user-declared `data Effect`
is not opaque, so the pass leaves it nominal. Order, inertness, and the
entry-only runner are recorded in
[the effects checklist](../../../implementation/backend/effects.md).
`EffectLowering::verify` rejects a recorded closure whose parameter list is not
`[Token]` or whose result is not the lowered effect result. The executed
fixtures rewrite the node after `lower_effects` returns and call `verify` again
(`a_lowered_effect_closure_flattened_to_arity_two_is_rejected`,
`a_lowered_effect_closure_with_the_wrong_result_is_rejected`). They fail with a
Core `VerifyError` and do not enter the backend. `lower_effects` also calls
`verify` before it returns, and
`crates/psrs-backend/src/effects.rs` maps that returned error to
`InvalidCompilerIr` under `P8 effect lowering`. The closures the pass writes
already match the record, so the fixtures do not take that mapping. A type
table changed after the pass returns is not checked again on the compile path.
`Type::Closure` remains a general representation, and only the recorded nodes
are constrained. The optimizer's effectful-call preservation rules remain in
the Core and MIR optimization documents.

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
