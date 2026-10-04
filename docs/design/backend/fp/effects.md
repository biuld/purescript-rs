# Effects

**Feature:** F-02  
**Status:** Draft (design)  
**Prerequisites:** [functional core](../../frontend/semantics/functional-core.md), [CC IR](cc-ir.md), and
[type classes and dictionaries](type-classes-and-dictionaries.md); monads,
closures, and the distinction between a value and a computation. Read
[IR boundaries](../00-ir-boundaries.md) first.  
**Summary:** `Effect a` stays abstract through type checking and Typed Core, with
its trusted constructor and operation identities resolved once and carried to
the representation boundary. Lowering uses those identities to create generic
closures and to plan suspended foreign imports before erasing the abstract type.
The command entry accepts `Int` or `Effect Unit`; an internal wrapper runs an
effectful entry once and returns zero after normal completion.

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
trap :: Effect Unit
```

`runEffect` is a trusted library operation with an ordinary source function
type. A direct reference is permitted only in the selected command entry
declaration. This is a lexical reference check, not a capability or
non-escape guarantee: the entry may pass the function value to a helper, which
may then call it. For `main :: Effect Unit`, the compiler-generated command
wrapper is the primary route for running the selected action.

`trap` is the effect whose result never exists: an uncaught failure on this
target is a guest trap, and this is the one operation that says so. It takes no
argument because the failure has already been reported through an ordinary
service such as `Effect.Console.error` before it is used; a library that has a
message to carry writes it first and then escapes. It is
`Effect Unit` because that is the assertion and platform API the current
library needs. This API choice is not forced by the lowering: a non-returning
trap produces no result value, so the backend does not need to invent a value
of the use-site type.

The equations below are the representation translation, not source equalities.
`Token` does not occur in a source type, and `Effect a` does not unify with
`Token -> a`.

```text
⟦Effect τ⟧ = RepClosure([Token], ⟦τ⟧)
pure v      = RepClosure(token) { v }
bind m k    = RepClosure(token) { call (call k (call m token)) token }
runEffect m = call m runtimeToken
trap        = RepClosure(token) { unreachable }

commandEntry(main : Effect Unit) =
    let action = call main []
    runEffect action
    0
```

- **Construction is inert.** Building an `Effect` value does not perform the
  operation deferred in its closure; storing or passing the value does not run
  it. Source arguments are evaluated strictly as usual and may themselves have
  observable behavior.
- **Running is explicit.** The selected entry may call `runEffect` explicitly
  where its source type permits. For an `Effect Unit` entry, the generated
  command wrapper runs its returned action exactly once and yields zero after
  normal completion. A trap propagates.
- **Sequencing is left to right.** Elaborated `bind` runs the first computation,
  then feeds its result to the continuation with the same token. Ordering comes
  from the calls and strict evaluation order, not from the token's bits.
- **The token is not a source value.** No source program names it, applies an
  effect to it, or passes an ordinary function where an effect is required.
- **Escaping is not returning.** Applying `trap` to the token reaches an
  unreachable path instead of producing a `Unit`, so everything sequenced after
  it in the same effect runs not at all. That is what makes an assertion failure
  observable to a runner that only sees the guest terminate.

### Two callable forms

A source function and a representation closure are different callable
values. `RepClosure` is notation for the existing generic `Closure`
representation with fixed parameters. It is not an Effect-specific closure
kind or runtime object.

```text
SourceArrow a b     = the curried Function spine a -> b
RepClosure params r = a generic closure with a fixed parameter list and a
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

The frontend resolves `Effect` through the trusted library binding and checks
it by ordinary kind, application, and subsumption rules. It adds no effect
flag, private constructor, or representation mode.

- `Effect a` is `Application(Constructor(User(effect_id)), a)` on the uniform
  application spine ([DEC-15](../../../decision/DEC-15-unified-type-representation.md)).
  THIR and Typed Core keep that type. Unification, subsumption, and arity
  flattening do not treat it as a function.
- Trusted-library binding resolves the semantic identity of `Effect` and the
  `pure`, `bind`, `runEffect`, and `trap` operations once. The linked Core input
  carries those resolved identities to the lowering that consumes them. No
  later stage reconstructs trust from a qualified source name or opacity flag.
- Before replacing abstract applications, the lowering classifies each
  effectful foreign import from that identity and its checked, synonym-expanded
  external scheme in Core. `Effect` remains abstract at this point. The pass
  records an immutable suspension plan for that exact import.
  After erasure, only imports in those plans receive a delayed wrapper; a
  generic one-parameter closure shape is not evidence of an effect.
- No stage stores an "effect" flag on an expression or adds a dedicated Effect
  IR node, closure kind, or runtime object. After representation lowering, CC
  and MIR see generic closures and calls.
- The synchronous token may be the integer zero. It carries no scheduling or
  ordering guarantee. Call order and multiplicity are preserved by the
  evaluation and optimizer contracts.
- Entry selection produces one resolved command-entry `SymbolId`: use
  `Main.main` when present, otherwise require one unique top-level `main`.
  The lexical `runEffect` reference check and generated entry wrapper use that
  same identity.
- Calls, including calls reached through closures, may perform effects. Core,
  CC, and MIR optimizations preserve their order and multiplicity unless a
  separate purity proof establishes that a particular call is inert.

## Design

### Chosen representation: one closure translation

Representation lowering runs after Typed Core and before curried-arrow
flattening. It is an ordinary pass: it consumes Core, where `Effect` is still
abstract, and produces CC closures with explicit parameter lists.

- A value of type `Effect τ` becomes `RepClosure([Token], ⟦τ⟧)`.
- `pure`, `bind`, `runEffect`, and `trap` are foreign imports of those abstract
  signatures. The pass replaces them with the closures in the model. It does
  not emit a host call for them.
- `trap`'s closure body is an unreachable path, so applying it ends the guest.
  It is a Core expression type, not a new CC or MIR instruction: the existing
  trap assignment already ends a path for an unmatched pattern.
- Before effect applications are rewritten, each imported operation is
  classified from the trusted `Effect` identity in its checked, synonym-expanded
  external scheme. `Effect` remains abstract until this lowering.
  The plan records the checked source scheme, source parameters, and effect
  payload. When applying it, lowering derives the host function type from those
  parameters and the payload. The wrapper performs the host call when the
  returned effect closure is
  run; an unrelated import returning an ordinary closure is left unchanged.
  In a strict language, the host operation itself must not happen when the
  effect value is built. `pure foreignCall` would evaluate that call too early,
  so the import wrapper provides the suspension.
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

### Lowering `pure`, `bind`, `runEffect`, and `trap`

The source declarations are abstract. Representation lowering replaces their
bodies; the type checker does not see a token.

```text
lower_effect(Effect τ) = RepClosure([Token], lower(τ))

lower_pure = \value -> RepClosure(token) { value }
lower_bind = \first -> \next -> RepClosure(token) {
    result = call first token
    rest   = call next result
    call rest token
}
lower_run  = \action -> call action runtimeToken
lower_trap = RepClosure(token) { unreachable }
```

`call` of a `RepClosure` passes exactly that closure's parameter list. It does
not continue into a function or effect stored in the result. Sequencing is the
order of these calls: `call first token` completes before `call next result`.

### Planning suspended imports

Before lowering replaces `Effect` in an import signature, the backend reads the
external's checked scheme from `Module.external_types` and records an immutable
plan for that resolved external. The scheme has aliases already expanded and
retains its `ForAll` quantifiers. A plan retains the original checked source
`TypeId`, its quantified variables and stripped body, the source parameters,
the exact Effect application and payload/result types, and the source symbol
and span. When applying the plan, lowering derives the host function type from
the recorded source parameters and payload, retargets the external binding to
that host symbol and type, and builds a wrapper under the original source
symbol used by Core calls. The applied record retains the derived host symbol
and type; verification checks the host binding and generated wrapper against
that type and the original plan. The host operation executes only when the
closure body is called; an ordinary one-parameter closure in another import
has no such plan and is not suspended.
Structural verification compares each plan with its generated wrapper and the
complete transformed Core. Focused execution tests separately establish
inertness and ordering.

### Normalizing the command entry

Entry selection is shared with the lexical `runEffect` check and produces one
resolved `SymbolId`. The selected source declaration is `Main.main` when it is
present; otherwise the program must have exactly one top-level declaration
named `main`. The selected declaration must take no arguments and return either
`Int` or the trusted `Effect Unit` type. Before erasure, the backend checks
that an Effect command-entry record names exactly the selected Core entry;
missing or stale metadata is an invalid compiler contract.

An `Int` entry retains the existing zero-argument integer command
convention, and its result remains the process exit code. For `Effect Unit`,
the compiler generates an ordinary Core adapter that evaluates the selected
declaration to an action, runs it exactly once through the trusted effect
runner, then returns zero. A guest trap escapes before normal completion and
is not translated into zero. The adapter is placed in the selected source
module, retains source origin for diagnostics, and becomes the Core module
entry before CC. The selected source `SymbolId` remains available for lexical
runner checks. The adapter lowers to generic closure and call forms.

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

Trusted source bindings and ordinary closure lowering jointly own the
representation. The implementation must conform to this organization.

```text
crates/psrs-driver/src/program/effects.rs
    resolve TrustedEffect; select EffectCommandEntry { symbol };
    enforce the lexical runEffect reference rule
crates/psrs-thir/src/lib.rs and crates/psrs-core/src/lib.rs
    carry WIT ExternalType { symbol, source_module, ty } checked schemes through P6/P7;
    preserve those IDs while linking and optimizing Core
crates/psrs-backend/src/bindings/mod.rs
    build ExternalBindings from checked Core external schemes
crates/psrs-core/src/effect/
    lower abstract Effect applications using trusted identities
crates/psrs-backend/src/effects.rs
    classify abstract imports and retain their suspension plans;
    build planned wrappers and the Effect Unit command adapter;
    verify the complete transformed Core
crates/psrs-backend/src/cc/ and crates/psrs-backend/src/mir/
    lower generic closures, calls, and command ABI
```
Responsibilities and required types:

- Trusted library binding resolves a `TrustedEffect` record with the resolved
  `Effect` constructor identity and the `pure`, `bind`, `runEffect`, and
  `trap` operation identities. The driver passes this record alongside linked
  Core.
- Each WIT external import has an `ExternalType { symbol, source_module, ty }` in THIR and Core.
  `ty` indexes the checked module type table; type synonyms are expanded during
  checking and `ForAll` quantifiers remain in the scheme. Linking and Core
  optimization remap the type ID with that table. The raw HIR external
  annotation remains source metadata, not the authority for WIT semantics.
  The recorded `source_module` identifies the declaring module independently
  of the foreign symbol namespace and remains authoritative for diagnostics.
- `ExternalBindings::from_core` consumes the checked Core external scheme table.
  Effect classification, WIT conformance, and trusted WIT operation-signature
  validation use these checked type IDs; backend stages do not re-elaborate HIR
  annotations or reconstruct aliases. Compiler intrinsics keep their explicit
  `Intrinsic` descriptor contracts and do not use `ExternalType`.
  When trusted operation imports become synthesized declarations, their
  external scheme entries are removed with those imports.
- `psrs-typecheck` checks `Effect a` as an ordinary abstract application.
  It has no `Effect` constructor, no mode that opens an effect into an arrow,
  and no unification of `Effect a` with a function.
- Before rewriting abstract effect applications, the lowering pipeline records
  an `EffectImportPlan` for each semantically effectful WIT import, using its
  checked external scheme and trusted Effect identity. The plan retains the
  original checked `TypeId`, quantified variables, decomposed source arguments
  and payload/result types, and the source symbol/span. Applying the plan derives
  the host type from the source parameters and payload; the applied record keeps
  the derived host symbol/type for verification. Lowering replaces `Effect τ`
  with generic
  `Closure([Token], lower(τ))` types and synthesizes operation bodies and
  wrappers only from the explicit identities and plans.
- The wrapper and rewritten Core are structurally verified before CC. This
  checks binding and type-shape contracts; it does not add a Core or CC Effect
  node or prove runtime behavior.
- CC and MIR lower the resulting generic closures through `FunctionRef` and
  direct or indirect calls. Curried-arrow flattening reads source `Function`
  spines only. Partial application
  (`lower_partial_global_application`) applies to under-applied source arrows,
  not to the token of an effect.
- The representation may later grow into dictionary passing
  ([type classes and dictionaries](type-classes-and-dictionaries.md)). The token
  stays inside effect lowering; its type is chosen there.
- WASI operations are owned by the
  [WASI platform library](../wasm/wasi-platform-library.md) and the
  [canonical ABI and WIT](../wasm/canonical-abi-and-wit.md). A host call is
  suspended only when its checked scheme returns an application of the trusted
  Effect constructor and import classification records a suspension plan;
  generic closure shape does not classify an import.

## Invariants and verification

The structural verifier checks that the trusted constructor and operation
identities match their binding metadata; each planned external still matches
its checked Core scheme, and each applied plan matches its derived host type and
generated wrapper; each rewritten effect application has one token parameter
and the lowered result;
and the transformed Core module, including wrappers, is valid. These checks
establish structure and identity only.

Behavioral guarantees require source-level execution. Tests must show that
building the deferred action does not call its external, that strict argument
evaluation remains in source order, that `bind` runs each continuation action
once and left to right, and that the `Effect Unit` command wrapper returns zero
only after normal completion. A trap must escape the wrapper without a normal
exit result. CC/MIR shape verification does not establish these behaviors.

Execution evidence requires `wasmtime`. A scoreboard's runtime-completion
classification is not a golden stdout comparison and cannot distinguish every
host CLI failure from a guest exit. Focused tests must assert expected output,
process status, or an explicit trap marker with `PSRS_REQUIRE_WASMTIME=1`
([DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md)).

## Worked example

```purescript
main :: Effect Unit
main = do
  log "first"
  log "second"
```

`log :: String -> Effect Unit` has one source parameter. The selected
declaration returns an effect closure. The compiler-generated command wrapper
calls the selected declaration once, applies the returned closure once, and
returns zero after the action finishes:

```text
action = DirectCall(Main.main, [])
        // result: Closure([Token], Unit)
ignored = call action runtimeToken
result = 0
```

The writes occur inside the action closure, so the wrapper prints
`first\nsecond\n` once and exits with code zero. If the action traps, the
wrapper does not reach its normal zero result. A wrapper verifier checks the
generated call types and selected symbol; Wasmtime execution tests establish
that the writes happen once and that a trap escapes.
## Boundaries and interfaces

- **From the frontend and driver.** The driver passes a `TrustedEffect`
  binding and an `EffectCommandEntry` with the selected source `SymbolId`
  alongside linked Core. `do`/`ado` desugars to library `bind`. No effect flag
  is added to CST, AST, HIR, or Core nodes, and P5 does not open the
  representation.
- **Through effect lowering.** This converts abstract Core types to generic
  closures. It verifies an `Effect Unit` entry and inserts its ordinary adapter
  before CC. Import plans survive long enough to generate and verify wrappers;
  downstream stages receive closures, not `Effect`.
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
- **Diagnostics.** A direct source reference to `runEffect` outside the
  selected command-entry declaration receives a source diagnostic. Passing the
  runner from within that declaration to a helper is permitted; this rule does
  not prevent escape or invocation through a passed value.

## Implementation notes

Implementation and test status, including stage-specific evidence, are
maintained in the [effects acceptance record](../../../implementation/backend/effects.md).
That record retains historical results and marks the expanded identity,
import-plan, transformed-Core, and command-entry requirements pending until
their focused tests and required runtime execution are recorded. This design
specifies the intended contracts; closure-shaped IR alone does not establish
the behavioral guarantees above.

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
