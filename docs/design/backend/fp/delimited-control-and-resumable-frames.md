# Delimited Control and Resumable Frames

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Draft design. The mechanism is selected by
[DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md); the
lowering is not implemented.  
**Prerequisites:** [DEC-03](../../../decision/DEC-03-purescript-faithful-type-system.md),
[DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md),
[functional Core](../../frontend/semantics/functional-core.md),
[CC IR](cc-ir.md), [MIR](mir.md),
[control flow and tail calls](control-flow-and-tail-calls.md),
[representation and evidence](representation-and-evidence.md),
[capability profile](../wasm/capability-profile.md), and the
[library-owned effects](algebraic-effects-and-handlers.md) this mechanism runs.  
**Summary:** The compiler owns an effect-agnostic delimited-control surface —
`Control f a` with `return`/`bind`/`perform`/`run` — lowered by a selective,
defunctionalized continuation-passing transformation to explicit GC frame objects
driven by a trampoline. It needs no new target capability and is distinct from
WebAssembly stack switching.

## Scope

This document owns the generic control surface, its identity, the selective
CPS/defunctionalization lowering, the frame representation and trampoline,
deep/shallow encoding, one-shot status, continuation lifetime and GC roots,
foreign-call capture boundaries, and the comparison with native stack switching.

It does not own operation signatures, handlers, or rows — those are the
[library-owned effects](algebraic-effects-and-handlers.md) this mechanism runs — nor
the official Effect/ST facades ([native topic](native-effect-and-st-handlers.md)),
nor target capability decisions ([capability profile](../wasm/capability-profile.md)).

It adds no source syntax, no frontend effect rows, and no effect tracking or effect
reasoning in the type system: the surface is an ordinary monadic type over an opaque
request functor, and the compiler never infers, propagates, or solves effects and
never reasons about which effects a function may perform.

The mechanism follows DEC-19. DEC-19 narrowly amends DEC-03's blanket exclusion of
delimited continuations below Typed Core; the type-system decision (effects are a
library encoding) is unchanged.

## Background

### Delimited control

A **continuation** is the rest of a computation from a given point. **Delimited
control** captures that rest only up to a boundary — a **delimiter** — rather than
capturing the entire remaining program. A computation may **suspend** at an
operation; the delimiter's **handler** receives the operation together with the
captured continuation (a **resumption**) and decides how to proceed: it may resume
zero times (abort), once, or many times (replay). Capturing stops at the delimiter,
so code outside it is not part of the continuation.

This mechanism is what makes library-defined handlers executable. The operations,
signatures, and handlers themselves are library values
([library-owned effects](algebraic-effects-and-handlers.md)); the compiler owns only
the suspension/delimiter/resumption mechanism and its representation, and never
learns what a particular operation means.

### Continuation-passing style and defunctionalization

In direct style, a function `f :: A -> B` returns its result to its caller, and the
caller's remaining work lives in the engine's execution stack. In
**continuation-passing style (CPS)**, the same function becomes
`A -> (B -> r) -> r`: it does not return; it calls a **continuation** `k` that names
the rest of the computation. Every call can then be a tail call, so the "rest" is a
value (`k`) rather than a stack of return addresses. Conceptually, capturing a
continuation is keeping that `k`; resuming is calling it.

The concept alone is not enough for this target, for two reasons:

1. **Capture needs reification.** If the "rest" is spread across the live Wasm
   stack, it cannot be stored, passed to a handler, inspected, or resumed twice. The
   continuation must be **data**.
2. **Stack safety must not depend on the engine.** The pinned profile enables tail
   calls, so pure closure-CPS can be stack-safe, but a captured continuation still
   needs a representation the compiler can reason about and verify.

The lowering therefore **defunctionalizes** CPS: instead of closures, a continuation
is an explicit chain of **frame objects** in the GC heap, and a loop runs the
machine one frame at a time.

### Why a host import cannot capture a continuation

A Rust/Wasm function called by the guest cannot capture the caller's Wasm execution
stack; Rust's private linear-memory stack is a different stack, and moving its stack
pointer or saving registers is not a continuation implementation. "Runtime support"
for delimited control therefore cannot be a normal imported binding. The two viable
implementations are compiler-generated frames (this design) and target-level stack
switching (rejected for now, below).

### Stack switching is not a target feature

The WebAssembly stack-switching proposal is at Phase 3 (implementation), not merged
into the core specification. The pinned runtime disables it by default
(`Config::wasm_stack_switching = false`), it depends on the `exceptions` feature,
and the pinned profile disables `exceptions`. Adopting it would be a deliberate
target-profile revision that ties the artifact to a moving proposal and a
non-default runtime configuration. DEC-19 therefore selects compiler-generated
frames as the baseline and keeps stack switching as a future accelerator behind a
separate decision.

## Model

### The computation carrier and primitives

The compiler-owned surface is an **over-functor computation**: a freer computation
over an opaque request functor `f`, with four primitives.

```text
Control f a                  -- computation over request functor f, yielding an a

return  : a -> Control f a
bind    : Control f a -> (a -> Control f b) -> Control f b
perform : f a -> Control f a
run     : (forall x. f x -> (x -> Control f a) -> Control f a)
        -> Control f a -> a
```

- `Control f a` is opaque; no pass inspects a request.
- `perform` suspends: it captures the continuation up to the nearest `run`,
  delivers the request value `f a` to that delimiter, and resumes with an `a`.
- `run` installs the delimiter. Its handler is rank-2 in the request's answer `x`,
  so an operation's payload/continuation relation is checked by ordinary rank-N
  typing; no native GADT or existential is needed.
- `bind` composes continuations directly (freer style), so no `Functor f`
  constraint is required and a library functor needs no mapping instance.

The continuation a handler receives is the function `x -> Control f a`, delimited by
its `run`; it captures only frames inside that delimiter. `run` is **closed** to its
answer `a`; a residual computation is a library concern, handled by re-entering
`run`.

**Why this surface.** It is exactly the shape `purescript-run` builds on, so that
ecosystem's operation rows, `VariantF` encodings, and deep handlers map onto it
directly; it keeps every concrete operation in library code; and it is expressible
with ordinary rank-N polymorphism. The alternatives are rejected. A separate
`suspend`/`handle`/`resume` triple with an abstract `Resumption` still has to carry
the payload/result relation in the request value, so it collapses into this surface
unless it adds an existential. `shift`/`reset` threads an answer type through the
monad and is not expressible without a frontend effect-row system, which DEC-03
excludes.

The surface is **monadic, not direct style**. Direct-style effectful code needs
`shift`/`reset` or effect syntax and is future work
([Open questions](#open-questions-and-future-work)). The compiler's contribution
over a pure library free monad is the representation: it lowers `bind`/`perform` to
an explicit continuation chain, so binding is stack-safe without materializing a
tree and a handler receives a first-class resumption.

The surface is independent of how the library encodes operations: `f` is opaque, so
`VariantF` over an open row, a closed operation functor, `Data.Exists`, or a rank-N
eliminator are all library choices.

### Effect-agnostic contracts

These are the guarantees the compiler provides and the ones that make concrete
effects safe:

- A request is delivered unchanged to its delimiter; the compiler never inspects or
  reinterprets it.
- A continuation is resumed at exactly the result type at which it suspended. The
  compiler preserves this relationship through erasure, boxing, and call adaptation;
  it cannot be exchanged for an unrelated result type.
- A continuation captures only up to its delimiter. Frames outside the delimiter are
  not part of it.
- A continuation's lifetime is bounded by its delimiter, and the values it captured
  stay valid for as long as it is live (they are GC roots).
- Multiplicity is an API property: the base surface is multi-shot, one-shot is a
  separate API, and the compiler provides the representation and check for both.

### Continuation multiplicity

Deep and shallow are handler semantics, owned by the
[library-owned effects](algebraic-effects-and-handlers.md#handler-semantics)
document; the lowering supports both (see
[Deep and shallow handlers](#deep-and-shallow-handlers)).

The surface is **multi-shot**. A handler receives a persistent continuation function
and may call it zero, one, or many times; replay re-runs the interpreted remainder
and does not roll back mutations or external IO already performed by another branch.
This matches the portable library encoding and needs no extra representation.

**One-shot is a separate library API.** It exposes an opaque `Resumption a` value
with a consumed status instead of a bare function; the first resume atomically
consumes it and a second attempt is rejected rather than silently reinterpreted. The
compiler provides the frame representation and the atomic check; no optimization may
turn a multi-shot continuation into a one-shot one. Existing PureScript arrows are
unrestricted, so hiding a constructor or adding a nominal parameter does not enforce
one-shot use; the guarantee lives at the representation boundary, not in the type.

## Design

### Identity and exposure

The compiler owns the carrier and the four primitives. They are declared as a
compiler-reserved interface module, `Prim.Control`, the way `Prim.Coerce` exposes
`coerce`: `Prim.Control` declares the abstract type `Control` and the
`return`/`bind`/`perform`/`run` values, whose schemes the compiler checks against the
[model above](#the-computation-carrier-and-primitives). The module carries no
operation registry; no pass recognizes a concrete operation or adds an
operation-specific solver.

The surface is exposed to source only as this abstract typed API; the independent
standard library wraps it into the operation and handler API users see. Because the
surface is abstract, a raw capture cannot inhabit an unrestricted pure function type
while changing its control behavior: the caller must use `Control` and the
primitives, whose types state the suspension.

`Prim.Control` is not a host import and not a WIT binding: it has no runtime
implementation of its own, because the lowering produces ordinary guest code.

### Lowering: selective CPS to resumable frames

#### Selective conversion

Converting a whole program to CPS would be slow and would change the representation
of code that never suspends. The compilation is **selective**:

- A function is *convertible* when it can reach a suspension in its dynamic extent —
  directly, through a call, or through a value it invokes.
- Only convertible functions are lowered to resumable form. Pure, non-convertible
  functions keep the ordinary direct representation and calling convention.
- A suspension converts exactly the region from the suspension point out to its
  delimiter; frames below the delimiter are ordinary calls and are not captured.

Selectivity keeps the lowering proportional to the effectful part of the program and
keeps pure code fast. It is also where optimization interacts with the mechanism: an
inliner or specializer must not merge a convertible function into a non-convertible
one without preserving the conversion of the merged region, nor erase a suspension
whose result is unused, because the suspension is observable control, not a pure
computation.

#### Frame construction

Each step of a converted computation is represented by a frame. A frame stores:

- the free variables the step needs (its captured environment),
- a small tag identifying which continuation step it is ("after this `perform`,
  apply this function to the answer"),
- the pointer to the next frame (the previously captured remainder).

A `perform` builds a frame chain from the current point out to its `run` delimiter,
hands the request and a handle to that chain (the continuation) to the handler, and
stops running the current computation. Continuing re-installs the chain and supplies
a value as the performed step's result. Frames are ordinary GC objects, so captured
references are roots the engine scans automatically; this is a decisive advantage
over capturing a raw execution stack, where the runtime must establish stack maps to
find roots.

#### The trampoline

A **trampoline** is the loop that runs the machine:

```text
run(state):
    loop:
        step = state.current
        match step:
            Return(v)          -> return v
            Eval(frame)        -> state.current = frame.step(state)
            Suspend(payload, k) -> state = call_handler(payload, k)
            Resume(k, v)       -> state.current = k.install(v)
```

Because the loop drives every step, the native Wasm stack does not grow with the
depth of binds or with long request sequences. Deep recursion in direct style
becomes bounded frame churn, the same way a self tail call becomes a loop. Using
`return_call`/`return_call_ref` for the tail positions of the emitted steps keeps
the constant-stack property even where the machine calls into ordinary code.

#### Deep and shallow handlers

The delineated region is a marker in the frame chain. A **deep** handler is one
whose continuation re-installs the handler before continuing, so subsequent requests
at the same delimiter are handled again; the lowering wraps the resumed chain with
the handler frame. A **shallow** handler resumes the chain without re-wrapping, so
the handler runs once and later requests escape to an outer delimiter. The library
selects the semantics; the lowering must implement both and must not silently deepen
or shallow a handler.

#### One-shot and multi-shot

The frame chain is the continuation. Two representations are possible:

- **Persistent frames** (the natural multi-shot representation): a resumed frame is
  not mutated, so the same continuation may be resumed again. Replay re-runs the
  captured remainder against the handler's supplied values. Mutable references and
  external IO already performed by another branch are shared, exactly as the pure
  model requires; multi-shot execution does not roll back IO.
- **Owned frames** (the one-shot representation): a `Resumption` holds a status of
  `Pending`, `Running`, `Consumed`, or `Cancelled`. The first `resume` atomically
  transitions out of `Pending`; a second attempt is rejected rather than silently
  reinterpreted. Transfer must be atomic with respect to re-entrancy, so a re-entrant
  resume of a running resumption fails rather than corrupting the chain.

The two are distinct APIs. An optimization may not turn a multi-shot continuation
into a one-shot one.

#### Lifetime, GC roots, and abandonment

A continuation keeps its captured frames reachable, so anything those frames
captured stays alive until the continuation is dropped, resumed to completion,
cancelled, or abandoned. When a handler returns without resuming, its continuation
becomes unreachable and the GC reclaims the frames. The compiler must not drop a
live continuation to an optimization, and must not let a dead one keep roots alive
beyond its contract.

#### Foreign-call boundaries

Suspension cannot cross a non-suspendable host or foreign frame: a captured
continuation that spans a canonical-ABI call or an in-progress WIT import cannot be
resumed without that boundary's cooperation. The lowering rejects a conversion that
would capture across such a boundary unless the boundary declares an explicit
suspension protocol. A continuation also cannot retain a borrowed canonical-ABI
buffer or a host-resource reference past the boundary's lifetime.

### Compiler CPS versus native stack switching

DEC-19 selects compiler-generated frames. The alternative, WebAssembly stack
switching, is a real implementation with different trade-offs. This section is the
comparison the mechanism decision rests on.

The two implementations differ in where the "remainder" lives. **Native stack
switching** extends the instruction set with a continuation reference type and
`suspend`/`resume` instructions: a delimiter becomes a continuation object, and
suspending captures the current execution stack into it, which the runtime relocates
off the main Wasm stack; resuming reinstalls the captured segment. The runtime must
then scan captured stack slots for GC roots. **Compiler CPS** never touches the
execution stack: the compiler rewrites the functions that can suspend into a state
machine whose states are explicit GC frame objects, and a work-loop (trampoline)
drives them, so the "remainder" is ordinary heap data the GC already understands.

| Dimension | Compiler resumable frames (CPS/defunctionalized) | Native stack switching |
| --- | --- | --- |
| Standardization | Uses standardized Wasm 3.0 features (GC, function references, tail calls) already in the profile | A Phase 3 proposal, not merged into the core specification; semantics may still change |
| Runtime enablement | Runs on the default pinned runtime: those features are on by default | Wasmtime disables it by default; it additionally depends on the `exceptions` feature, which this profile disables |
| Portability | Any engine implementing the pinned profile | Only engines implementing the proposal with the matching configuration |
| What is captured | An explicit heap chain of frame objects | The live Wasm execution stack (a continuation object) |
| Capture cost | Allocates frame records proportional to the captured region; selective conversion limits it to the effectful part | The runtime captures or switches a stack segment; cost scales with the stack being captured |
| Cost on non-suspending code | Pure code stays direct; only convertible regions pay per-step frame cost | Direct code is unchanged; the runtime's switching machinery is present regardless |
| Stack safety | A trampoline gives constant native stack independent of bind depth and of the engine | Depends on the runtime moving captured stacks off the main stack; not a compiler guarantee |
| GC/roots | Frames are GC values; captured roots are scanned automatically | Captured stack slots need runtime stack maps; GC/continuation integration is the runtime's responsibility |
| One-shot/multi-shot | Explicit: persistent frames are multi-shot, owned frames are one-shot and checkable | The runtime captures a stack; multi-shot requires cloning it; one-shot is the natural case |
| Foreign boundary | The compiler statically rejects capturing across a non-suspendable host frame | `suspend` also cannot cross a host frame; the proposal defines which frames are capturable |
| Compiler complexity | Large: selective conversion, frame identity, lifetime, and optimization interaction are new obligations in MIR and its verifier | Smaller in the compiler, but the artifact depends on a target feature and the runtime must be configured for it |
| Optimizability | Frames are explicit and inspectable; every pass must preserve frame identity and suspension observability | Capture points are opaque to the compiler, so it cannot reorder or eliminate them, but it also cannot reason about them |
| Verification | Fits the existing MIR/Wasm pipeline; verified and executed under the pinned runtime | Needs a validator built with the proposal enabled and a runtime so configured |
| Principal risk | High implementation cost and subtle frame/lifetime bugs | Dependence on a moving proposal and a non-default runtime configuration |

In short: stack switching moves the difficult part (capturing and rooting a live
stack) into the runtime and the standard, at the cost of an unstable, off-by-default
target feature. Compiler frames move it into the compiler, at the cost of a new
lowering, but keep the artifact inside the stable profile and make continuation
identity and lifetime explicit and verifiable. Given the project's rule that runtime
support is not a contract and that a proposal is adopted only by deliberate profile
revision, the compiler frames are the correct baseline. Stack switching remains a
future accelerator behind a separate decision once the proposal stabilizes.

### Rejected shortcuts

- Capturing the Wasm stack from a generic host import; a normal imported function
  cannot capture its caller's execution stack.
- Adopting Wasm stack switching while it is a Phase 3, off-by-default proposal that a
  second disabled feature gates.
- Letting a raw capture primitive inhabit an unrestricted pure function type while
  changing its control behavior.
- Offering only whole-program CPS: purity and stack cost of unconverted code must be
  preserved by a selective analysis.
- Recovering a hidden operation protocol by inspecting handler bodies.

## Algorithms

### Selective conversion

```text
convertible(f):
    if f performs a suspension:            return true
    for each call, value applied, or capture in f:
        if convertible(callee) or callee is unknown: return true
    return false

lower(program):
    mark every function convertible by a fixed point over the call graph
    for each convertible function:
        rewrite its body in CPS form (Section "Frame construction") and
        register its frames in the module frame table
    leave non-convertible functions direct
```

An unknown or indirect callee that might suspend makes its caller convertible, so
the analysis is conservative at exactly the boundaries it cannot see.

### Perform and continuation

```text
perform(request):
    k = capture_frames(current, up_to = nearest run delimiter)
    transfer(request, k) to the delimiter's handler

handler(request, k):              -- library code
    ...                           -- resume k, abort, or replay k

continue(k, value):
    reinstall(k.frames, value)    -- the performed step's result is value
```

`capture_frames` reads the current chain up to the delimiter without copying outer
frames. For the multi-shot surface it does not mutate the chain; for the one-shot
API the status transition (`Pending` → `Running` → `Consumed`) is atomic, so a
re-entrant resume is rejected.

### Frame construction and trampoline loop

```text
run(initial):
    state = initial
    loop:
        match state:
            Return(v)           -> return v
            Eval(frame)         -> state = frame.step()
            Suspend(payload, k) -> state = deliver(payload, k)
            Resume(k, v)        -> state = install(k, v)
            Abort               -> trap
```

`install(k, v)` continues the captured chain with `v` as the suspended step's
result. `deliver` calls the delimiter's handler; a handler that returns without
resuming produces a normal answer, and its continuation becomes unreachable.

### Deep-handler reinstatement

```text
install_deep(k, v, handler):
    // Deep: the same delimiter must handle the continuation's later requests.
    install(k, v) with handler re-wrapped around the resumed chain

install_shallow(k, v):
    install(k, v) without re-wrapping the handler
```

The frame chain records which wrapping applies; the lowering must not conflate the
two.

### One-shot consumption

```text
transfer(k):
    atomically: if k.status != Pending: reject
                k.status = Running
    ... normal or trapping completion sets k.status = Consumed ...
    ... cancellation sets k.status = Cancelled ...
```

The atomic transition is what makes one-shot reuse rejection a runtime guarantee
rather than a documented expectation.

## Code map

This is the design target, not a claim that these APIs exist yet:

| Location | Intended responsibility |
| --- | --- |
| `Prim.Control` owner | The abstract `Control f a` carrier and the `return`/`bind`/`perform`/`run` primitives, with their checked schemes |
| MIR control-lowering owner | Selective CPS/defunctionalization, the frame table, the trampoline, deep/shallow wrapping, and one-shot status |
| MIR verifier | Frame identity, capture boundary, continuation result-type, and suspension-observability checks |
| Existing Core/CC/MIR lowering and optimizers | Compile the converted frames as ordinary higher-order code; preserve generic observable-call semantics |
| Runtime control owner, only if separately adopted | Stack-switching ABI, captured-root storage, lifecycle checks, and capability requirements |

No compiler-side operation registry, handler IR, or effect-specific solver is
required. Runtime control module names and entry-point signatures remain unselected
until a target option is explicitly adopted.

## Invariants and verification

- A suspension is observable control: it is never erased, reordered across, or
  duplicated by an optimizer, and a convertible function is never merged into a
  direct one without preserving its conversion.
- A continuation is resumed only at its captured result type, and a one-shot
  continuation is transferred at most once, atomically with respect to re-entrancy.
- Deep and shallow handlers behave as declared; the lowering does not silently
  change the semantics.
- Captured frames keep their values valid until the continuation is dropped, resumed
  to completion, or cancelled; a continuation is never retained across a
  non-suspendable foreign boundary.
- The trampoline drives every step, so execution is stack-safe independently of the
  native stack; emitted tail positions use `return_call*`.
- Closure conversion, adapters, and optimizations preserve evaluation behavior under
  ordinary IR contracts; they do not verify a private protocol.

Evidence must cover frame identity, deep/shallow behavior, one-shot/multi-shot
rejection, stack safety, and any native control capability, separately from the
library-level interpretation evidence. The
[acceptance checklist](../../../implementation/backend/algebraic-effects.md) records
the requirements.

## Worked example

The [library-owned effects document](algebraic-effects-and-handlers.md#worked-example)
defines the program `runPure (handleState 0 program)`. Under this mechanism it is a
small frame chain. Tracing the trampoline:

```text
run(handleState 0 program)
  Put 41     -> handler state 0 becomes 41; continue
  Get k      -> handler supplies 41 to k; k is the delimited continuation yielding Int
  Return(42) -> the state handler returns { state: 41, value: 42 }
```

Each `put` and `get` performs an operation, builds a frame from the current point to
the `handleState` delimiter, delivers it to the handler, and resumes through the
trampoline. Depth of `do` binding does not grow the native stack. The library-level
meaning of this program is in the
[library-owned effects document](algebraic-effects-and-handlers.md#worked-example).

## Boundaries and interfaces

The control surface is abstract and library-facing; it is not new source syntax, and
the compiler adds no ambient effect inference. It is the only compiler-owned part of
the effects story; operations, rows, handlers, and the one-shot/multi-shot API are
library code ([library-owned effects](algebraic-effects-and-handlers.md#ownership)).

The baseline target profile does not gain stack switching through this design. A
native resumption option requires a pinned target capability, an MIR/lowering
contract, validator coverage, and actual execution evidence. An unsupported
capability is an error, never a silent change to continuation semantics.

## Open questions and future work

1. **Direct-style surface sugar.** Effect typing is out of scope (see
   [Scope](#scope)). A non-typed syntax that desugars to the monadic API is a
   separate, uncommitted option; it would add no effect tracking, but it would add
   non-PureScript syntax and is not part of this design.
2. **Conversion analysis formalization.** Fix the boundary between selective
   conversion and the existing tail-call, closure, and dictionary lowerings, and
   specify precisely which optimizations must treat a suspension as observable.
3. **Foreign-frame suspension protocol.** Capturing across a non-suspendable host or
   foreign frame is rejected by the lowering; a boundary that needs to suspend across
   it must define an explicit protocol and its lifetime contract.
4. **Performance.** Measure allocation, frame churn, and deep execution to set the
   lowering's performance contract.

## References

- [DEC-03](../../../decision/DEC-03-purescript-faithful-type-system.md),
  [DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md), and
  [compiler cleanup handoff](../../../implementation/backend/effect-state-removal-handoff-2026-10-11.md).
- [Continuing WebAssembly with Effect Handlers](https://arxiv.org/abs/2308.08347)
  and the [WebAssembly stack-switching proposal](https://github.com/WebAssembly/stack-switching),
  proposal-level references, not evidence of this target profile's support.
- [Capability profile](../wasm/capability-profile.md) and
  [DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md) for the target features
  and the rule that runtime support is not a contract.
- [Algebraic effects and handlers](algebraic-effects-and-handlers.md) for the
  library-owned operations and handlers this mechanism runs.
