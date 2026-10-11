# Library-Owned Algebraic Effects and Handlers

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Draft design. The generic mechanism and its representation are
selected by [DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md);
the lowering is not implemented.  
**Prerequisites:** [DEC-03](../../../decision/DEC-03-purescript-faithful-type-system.md),
[DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md),
[functional Core](../../frontend/semantics/functional-core.md),
[rows and records](../../frontend/type-system/rows-and-records.md),
[representation and evidence](representation-and-evidence.md),
[control flow and tail calls](control-flow-and-tail-calls.md),
[capability profile](../wasm/capability-profile.md), and
[runtime linking](../wasm/linking-and-runtime.md).  
**Summary:** Algebraic operations and their handlers are library values, checked
with ordinary PureScript types and rows. The compiler owns only a generic,
effect-agnostic suspension/handler mechanism, lowered to explicit resumable frames
(a selective, defunctionalized CPS) that need no new target capability. Runtime
delimited control through WebAssembly stack switching is a separately reviewed
target option, not an assumed ability of an ordinary Wasm import.

## Scope

This document owns the meaning of library-defined algebraic operations, handler
composition, row-based operation requirements, and continuation boundaries. It
also owns the generic control mechanism that makes such handlers executable — the
suspension, delimiter, and resumption primitives — and their lowering through MIR
to Wasm, including the comparison with native stack switching.

The following are separate decisions: official Effect/ST representation and source
adaptation (proposed in the [native topic](native-effect-and-st-handlers.md)), a
native effect type system, new source syntax, resource-ownership types, async
scheduling, and target stack switching. Designing algebraic effects does not
authorize rebuilding the withdrawn compiler-owned `Effect` or `State`
mechanisms, nor adding effect rows or handlers to the type system.

The proposal follows DEC-03's library encoding and the mechanism decision in
DEC-19. DEC-19 narrowly amends DEC-03's blanket exclusion of delimited
continuations below Typed Core; the type-system decision is unchanged. Any option
that needs a new target capability or a new frontend feature must be reviewed
against DEC-03 before it is selected, and this document does not silently amend
that accepted decision.

## Background

### What algebraic effects are

Algebraic effects and handlers separate the *description* of an effectful
operation from its *interpretation*. A computation performs an operation; a
**handler** around that computation decides what the operation means.

- An **operation** (also called a request) is a value describing what the program
  wants to do: "read the current state", "print this line", "get the clock time".
  It names a *result type* but not the way the result is produced.
- An **operation signature** is a set of operations, for example
  `Get : Unit -> S` and `Put : S -> Unit`.
- A **computation** is a value that may perform operations before it returns its
  result; its type records the result and the operations it may expose.
- A **handler** interprets the operations of one signature. It receives each
  operation together with a **continuation** — the rest of the computation from
  just after the performed operation — and chooses what to do with it.
- A **continuation** (also called a **resumption**) represents "the remainder".
  A handler may call it zero times (abort), exactly once (resume), or several
  times (replay).
- A **delimiter** is the boundary within which a handler is installed. Capturing a
  continuation stops at that boundary.
- A **deep handler** installs itself again for every request made after a
  resumption; a **shallow handler** interprets only the current request and runs
  the remainder without reinstalling itself.
- A **one-shot** continuation may be resumed at most once; a **multi-shot**
  continuation may be resumed arbitrarily many times.
- The **residual row** is the set of operations not handled by a given handler,
  which remains in the computation's type.

The intuition for a reader new to the idea is a `throw`/`catch` pair where the
catcher can *resume the thrower*. `catch` receives the thrown value; here the
handler also receives a way to continue, so it can supply a result and let the
original computation proceed. This is the same idea as `yield` in a generator,
generalized so that a handler can resume zero, one, or many times and can choose a
different interpretation for each request.

A minimal State signature shows the shape:

```purescript
data StateF s next
  = Get (s -> next)
  | Put s next
```

`Get` carries its result as a function argument (`s -> next`); `Put` has a unit
result (`next` alone). A `Functor` instance lets operations be composed without
knowing their meaning. The same console operation can write to a terminal or
append to a test log; the same state operations can thread a pure value or use
mutable storage. The operation says *what*, the handler says *how*.

The name "algebraic" comes from the interpretation: operations form a signature,
and handlers are algebras over it, obeying the laws the signature declares. The
implementation does not need to understand the theory; it only needs ordinary
values, functions, and a way to capture and resume a continuation.

Why not ordinary exceptions? A handler can *resume*. `catch` consumes control
permanently, but an effect handler can supply the operation's result and continue
the computation, possibly more than once. That single difference is what requires
continuations.

PureScript already provides the types to *describe* all of this: algebraic data
types, functions, higher-kinded types, type classes, rank-N polymorphism,
ordinary rows, and monadic `do`. `purescript-run` encodes extensible operations
using `Free` and `VariantF`; its handlers are ordinary interpreters. That
establishes expressibility in the language, not an execution strategy.

An operation row is a library parameter, not a second effect system attached to
every function arrow. Its constraints describe which requests a computation can
expose. They do not prove single use, termination, absence of unsafe FFI, or
correct resource release.

### Two ways to execute a handler

Once handlers are expressible, they still have to run. There are exactly two
execution strategies.

1. **Library encoding.** Operations are data constructors, a computation is an
   ADT, and a handler is a recursive interpreter. Binding builds a tree without
   performing requests. This is portable and needs no compiler support, and it is
   the default baseline of DEC-03. Its costs are indirect style (all effectful
   code lives in the monad) and stack safety that depends on a library trampoline
   rather than a compiler guarantee.

2. **Native delimited control.** The compiler provides a generic way to suspend
   the current computation and capture the remainder, and a handler is ordinary
   code that receives an operation and a resumption and decides whether to resume.
   Effectful code keeps its direct style, and the compiler can guarantee stack
   safety. This requires a compiler/runtime mechanism, which DEC-19 selects.

Neither strategy changes what operations, rows, or handlers mean; they differ only
in how a continuation is represented and resumed.

### What the compiler must not know

The compiler never learns what a particular operation means. It does not recognize
the spelling `State`, `Console`, `Run`, or `Effect`, does not inspect a handler
body to infer a hidden protocol, and keeps no operation graph, frozen source
inventory, or cross-pass witness. It owns only an abstract suspension boundary and
its representation. Everything specific — the operation signatures, the handler
interpretations, the residual rows, and the continuation-composition policies —
belongs to library code.

## Model

### Library computations and operation signatures

The library-visible model matches the kind of `Run`:

```purescript
Run :: Row (Type -> Type) -> Type -> Type

data StateF s next
  = Get (s -> next)
  | Put s next

data ConsoleF next
  = Print String next
```

Each signature has a `Functor` instance. Operations are lifted into a row-indexed
computation by library functions. Binding constructs the remaining computation
without performing requests. The semantic computation tree has `Return a` and
`Request op continuation`; this is a mathematical model, not a required runtime
layout. An implementation may use freer/Coyoneda representations and queued binds.

Schematic handler interfaces, which are library values, not compiler intrinsics:

```purescript
get :: forall s r. Run (state :: StateF s | r) s
put :: forall s r. s -> Run (state :: StateF s | r) Unit

handleState
  :: forall s r a
   . s
  -> Run (state :: StateF s | r) a
  -> Run r { state :: s, value :: a }

runPure :: forall a. Run () a -> a
```

Row membership and removal use ordinary row relations and library evidence. The
actual API must preserve the upstream row constraints, including label absence
where needed. State here is an ordinary value `s`, not a primitive token.

A handler handles its selected slot and forwards every other request with a
continuation that re-enters the handler (deep semantics). Distinct instances use
distinct labels; a duplicate-label or shadowing policy must be explicit rather
than relying on a silent row-map collapse.

### The generic control model

The compiler-owned model is deliberately smaller than any effect API. It consists
of four abstract notions.

- **A computation carrier** `Control a`: an opaque type for "a computation that
  yields `a` and may suspend". Only the primitives below inspect it; no pass
  pattern-matches on a concrete operation.
- **Suspension**: a primitive that captures the current continuation up to the
  nearest delimiter, delivers an opaque, library-typed payload to that
  delimiter's handler, and waits for a result of the suspension's result type.
- **A delimiter/handler boundary**: a primitive that installs a handler over a
  computation. The handler receives each suspension's payload and its resumption
  and returns the computation's overall answer.
- **A resumption** `Resumption a`: an abstract, typed value representing a captured
  continuation whose result type is `a`. Resuming applies it to a value of that
  type.

The essential, effect-agnostic contracts — the ones the compiler guarantees and
the ones that make concrete effects safe — are:

- A suspended payload is delivered unchanged to its delimiter; the compiler never
  inspects or reinterprets it.
- A resumption is resumed at exactly the result type at which it suspended. The
  compiler preserves this relationship through erasure, boxing, and call
  adaptation; it cannot be exchanged for an unrelated result type.
- A resumption captures only up to its delimiter. Frames outside the delimiter are
  not part of it.
- A resumption's lifetime is bounded by its delimiter, and the values it captured
  remain valid for as long as the resumption is live (they are GC roots).
- Resumption multiplicity is an API property. A one-shot resumption is consumed by
  its first resume; a multi-shot resumption is persistent. The compiler provides
  the representation and the check; the library chooses which API it exposes and
  may not silently change one into the other.

Because the payload and the resumption's result type are related existentially,
the surface is stated as a contract rather than a single surface type; the exact
signatures, and the rank-N eliminator that recovers per-operation typing, are
settled with the implementation. The contracts above are normative:

```text
Control a                    -- computation yielding an a that may suspend
Request                      -- an opaque, library-typed request value
Resumption <a>               -- a captured continuation yielding an a

suspend : Request -> Control a
    -- capture the continuation up to the nearest delimiter and deliver a
    -- request; the suspension's own result type is the a

handle  : Handler -> Control a -> Answer
    -- install a delimiter; the handler receives each Request together with the
    -- Resumption <a> for that suspension's own result type a, and returns the
    -- delimiter's Answer

resume  : Resumption <a> -> a -> Control a
    -- resume a captured continuation at its own result type
```

The payload type and the resumption's result type are linked by the library, not
by this surface: the library encodes typed requests with a rank-N eliminator, as
the [native topic](native-effect-and-st-handlers.md) sketches for heterogeneous
storage requests. The primitive only has to preserve the resumption's result type
and deliver the payload opaquely.

### Deep versus shallow, one-shot versus multi-shot

Deep and shallow are properties of a handler, expressed by whether the library
reinstalls the handler around the resumption. A deep state handler carries its
state value forward and handles later requests at the same slot; a shallow handler
returns control to the code outside the delimiter after the current request.

One-shot and multi-shot are properties of the resumption. The portable library
encoding is naturally multi-shot: continuations are persistent data, so replaying
one re-runs the interpreted remainder, and replaying does not roll back mutations
or external IO already performed by another branch. A native one-shot API is
naturally cheap because the captured frames can be consumed. Existing PureScript
arrows are unrestricted, so hiding a constructor or adding a nominal parameter
does not by itself stop a caller from invoking a value twice; a one-shot guarantee
must be checked at the representation boundary, not asserted by the type.

## Design

### Ownership

| Owner | Responsibility |
| --- | --- |
| Type system | Ordinary kinds, rows, class evidence, quantifiers, roles, constructor visibility, and applicable region constraints. No effect rows or handlers. |
| Library | Operation signatures, computation construction, handler interpretation, residual operation rows, continuation composition, and one-shot/multi-shot API choice. |
| Compiler | The abstract control surface; strict evaluation; ordinary closures/ADTs/dictionaries; generic calls and ABI checks; the selective CPS/defunctionalization lowering and its verification; sound optimization. |
| Runtime | Real IO/storage primitives; an optional trampoline or driver; if separately adopted, target stack switching and native resumption lifetime checks. |
| Target profile | Explicitly adopted Wasm features and rejection of unavailable capabilities. |

No pass discovers an operation by the spelling `State`, `Console`, `Run`, or
`Effect`. No pass interprets handler source to infer a hidden protocol. Generic
type, representation, and CFG verification remain required. The compiler preserves
observable calls and traps even when their results are unused; a library cannot
make an optimizer sound by providing a handler.

### The effect-agnostic control surface

The compiler owns only the four notions of the [control model](#the-generic-control-model).
It carries no operation registry and adds no operation-specific solver. The surface
is exposed to source as an abstract typed API; a library in the independent
standard library wraps it into whatever operation/handler API users see. Because
the surface is abstract, a raw capture cannot inhabit an unrestricted pure function
type while changing its control behavior: the caller must use `Control`,
`Resumption`, and the primitives, whose types state the suspension.

The surface must be declared with a stable identity and a checked type contract,
like other compiler primitives. It is not a host import and not a WIT binding: it
has no runtime implementation of its own, because the lowering produces ordinary
guest code.

### Lowering: selective CPS to resumable frames

The mechanism is implemented by compiling the regions that can suspend into an
explicit continuation representation. The concept is **continuation-passing style
(CPS)**; the representation is **defunctionalized resumable frames** driven by a
**trampoline**.

#### CPS in one paragraph

In direct style, a function `f :: A -> B` returns its result to its caller, and
the caller's remaining work lives in the engine's execution stack. In CPS, the
same function becomes `A -> (B -> r) -> r`: it does not return; it calls a
**continuation** `k` that names the rest of the computation. Every call can then be
a tail call, so the "rest" is a value (`k`) rather than a stack of return
addresses. Capturing a continuation is, conceptually, taking that `k` and keeping
it. Resuming it is calling it.

The concept alone is not enough, for two reasons specific to this target:

1. **Capture needs reification.** If the "rest" is spread across the live Wasm
   stack, it cannot be stored, passed to a handler, inspected, or resumed twice.
   The continuation must be **data**.
2. **Stack safety must not depend on the engine.** The pinned profile enables tail
   calls, so pure closure-CPS can be stack-safe, but a captured continuation
   still needs a representation the compiler can reason about.

The lowering therefore *defunctionalizes* CPS: instead of closures, a continuation
is an explicit chain of **frame objects** in the GC heap, and a loop runs the
machine one frame at a time.

#### Selective conversion

Converting a whole program to CPS would be slow and would change the representation
of code that never suspends. The compilation is **selective**:

- A function is *convertible* when it can reach a suspension in its dynamic extent
  — directly, through a call, or through a value it invokes.
- Only convertible functions are lowered to resumable form. Pure, non-convertible
  functions keep the ordinary direct representation and calling convention.
- A suspension converts exactly the region from the suspension point out to its
  delimiter; frames below the delimiter are ordinary calls and are not captured.

Selectivity keeps the lowering proportional to the effectful part of the program
and keeps pure code fast. It is also where optimization interacts with the
mechanism: an inliner or specializer must not merge a convertible function into a
non-convertible one without preserving the conversion of the merged region, nor
erase a suspension whose result is unused, because the suspension is observable
control, not a pure computation.

#### Frame construction

Each step of a converted computation is represented by a frame. A frame stores:

- the free variables the step needs (its captured environment),
- a small tag identifying which continuation step it is ("after the `Get`, apply
  this function to the read value"),
- the pointer to the next frame (the previously captured remainder).

A suspension builds a frame chain from the current point out to the delimiter,
hands the opaque payload and a `Resumption` (a handle to the chain) to the
handler, and stops running the current computation. `resume` reinstalls the chain
and supplies the response as the step's result. Frames are ordinary GC objects, so
captured references are roots the engine scans automatically; this is a decisive
advantage over capturing a raw execution stack, where the runtime must establish
stack maps to find roots.

#### The trampoline

A **trampoline** is the loop that runs the machine:

```text
run(state):
    loop:
        step = state.current
        match step:
            Return(v)          -> return v
            Eval(frame)        -> state.current = frame.step(state)
            Suspend(payload, k) -> state = call_handler(payload, k)   // may return
            Resume(k, v)       -> state.current = k.install(v)
```

Because the loop drives every step, the native Wasm stack does not grow with the
depth of binds or with long request sequences. Deep recursion in direct style
becomes bounded frame churn, the same way a self tail call becomes a loop. Using
`return_call`/`return_call_ref` for the tail positions of the emitted steps keeps
the constant-stack property even where the machine calls into ordinary code.

#### Deep and shallow handlers

The delineated region is a marker in the frame chain. A **deep** handler is one
whose resumption re-installs the handler before continuing, so subsequent requests
at the same delimiter are handled again; the lowering wraps the resumed chain with
the handler frame. A **shallow** handler resumes the chain without re-wrapping, so
the handler runs once and later requests escape to an outer delimiter. The library
selects the semantics; the lowering must implement both and must not silently
deepen or shallow a handler.

#### One-shot and multi-shot

The frame chain is the resumption. Two representations are possible:

- **Persistent frames** (the natural multi-shot representation): a resumed frame is
  not mutated, so the same `Resumption` may be resumed again. Replay re-runs the
  captured remainder against the handler's supplied values. Mutable references and
  external IO already performed by another branch are shared, exactly as the pure
  model requires; multi-shot execution does not roll back IO.
- **Owned frames** (the one-shot representation): a `Resumption` holds a status of
  `Pending`, `Running`, `Consumed`, or `Cancelled`. The first `resume` atomically
  transitions out of `Pending`; a second attempt is rejected rather than silently
  reinterpreted. Transfer must be atomic with respect to re-entrancy, so a
  re-entrant resume of a running resumption fails rather than corrupting the chain.

The two are distinct APIs. An optimization may not turn a multi-shot resumption
into a one-shot one.

#### Lifetime, GC roots, and abandonment

A resumption keeps its captured frames reachable, so anything those frames captured
stays alive until the resumption is dropped, resumed to completion, cancelled, or
abandoned. When a handler returns without resuming, its resumption becomes
unreachable and the GC reclaims the frames. The compiler must not drop a live
resumption to an optimization, and must not let a dead one keep roots alive beyond
its contract.

#### Foreign-call boundaries

Suspension cannot cross a non-suspendable host or foreign frame: a captured
continuation that spans a canonical-ABI call or an in-progress WIT import cannot
be resumed without that boundary's cooperation. The lowering rejects a conversion
that would capture across such a boundary unless the boundary declares an explicit
suspension protocol. The same rule applies to a canonical ABI buffer: a resumption
cannot retain a borrowed buffer or a host-resource reference past the boundary's
lifetime.

### Compiler CPS versus native stack switching

DEC-19 selects compiler-generated frames. The alternative, WebAssembly stack
switching, is a real implementation with different trade-offs. This section is the
comparison the mechanism decision rests on.

The two implementations differ in where the "remainder" lives. **Native stack
switching** extends the instruction set with a continuation reference type and
`suspend`/`resume` instructions: a delimiter becomes a continuation object, and
suspending captures the current execution stack into it, which the runtime
relocates off the main Wasm stack; resuming reinstalls the captured segment. The
runtime must then scan captured stack slots for GC roots. **Compiler CPS** never
touches the execution stack: the compiler rewrites the functions that can suspend
into a state machine whose states are explicit GC frame objects, and a work-loop
(trampoline) drives them, so the "remainder" is ordinary heap data the GC already
understands.

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

- Attaching effect requirements to all function arrows without a language/type
  system decision.
- Reconstructing handler behavior in Core, CC, or MIR to recover an implicit State
  protocol.
- Capturing the Wasm stack from a generic host import; a normal imported function
  cannot capture its caller's execution stack.
- Adopting Wasm stack switching while it is a Phase 3, off-by-default proposal that
  a second disabled feature gates.
- Letting a raw capture primitive inhabit an unrestricted pure function type while
  changing its control behavior.
- Treating a row requirement as linearity, single-use, or resource-lifetime
  evidence.
- Replacing native Effect/ST or public foreign APIs merely to get an algebraic
  effects example to compile.

## Algorithms

### Interpretation and forwarding (library)

For a pure State handler carrying value `s`:

```text
handle(s, Return(a))       = Return({ state: s, value: a })
handle(s, Get(k))          = handle(s, k(s))
handle(s, Put(s', next))   = handle(s', next)
handle(s, Other(op, k))    = Request(op, x -> handle(s, k(x)))
```

Actual evaluation uses an explicit stack-safe work loop or upstream trampoline
machinery. The final rule preserves the captured state while the outer handler
interprets another operation, and re-entering the State handler gives deep
semantics. It does not statically inspect the future continuation.

Type checking is ordinary application checking: operation lifting proves row
membership; handler application produces the residual row; `runPure` accepts only
the closed empty row. Any residual request is a type mismatch at that boundary,
assuming safe construction and the library's declared contracts.

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

### Suspension and resumption

```text
suspend(payload):
    k = capture_frames(current, up_to = nearest_delimiter)
    transfer(payload, k) to the delimiter's handler

resume(k, value):
    assert k.status == Pending            // for a one-shot resumption
    k.status = Running
    reinstall(k.frames, value)            // continue with value as the step result
    k.status = Consumed                   // on normal or trapping completion
```

The `capture_frames` step reads the current chain up to the delimiter without
copying outer frames. For a persistent (multi-shot) resumption it does not mutate
the chain; for an owned (one-shot) resumption the status transition is atomic so a
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
resuming produces a normal answer, and its resumption becomes unreachable.

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
| Independent `psrs-stdlib` package | Upstream computation libraries, the abstract-control wrapper, ordinary operation modules, handlers, source audit, and conformance fixtures |
| Existing kind/row/class/rank-N owners | Check library source using standard PureScript rules; repair shared deficiencies without effect-specific solvers |
| Compiler control-surface owner | The abstract carrier, suspension/delimiter/resumption primitives, and their checked type contract |
| MIR control-lowering owner | Selective CPS/defunctionalization, the frame table, the trampoline, deep/shallow wrapping, and one-shot status |
| MIR verifier | Frame identity, capture boundary, resumption result-type, and suspension-observability checks |
| Existing Core/CC/MIR lowering and optimizers | Compile library code and the converted frames as ordinary higher-order code; preserve generic observable-call semantics |
| `psrs-runtime` ABI/catalog/executable owners | Small native IO/storage contracts and implementations, independent of operation rows; an optional trampoline driver |
| Runtime control owner, only if separately adopted | Stack-switching ABI, captured-root storage, lifecycle checks, and capability requirements |

No compiler-side operation registry, handler IR, State graph directory, or
effect-specific solver is required. Runtime control module names and entry-point
signatures remain unselected until a target option is explicitly adopted.

## Invariants and verification

- Library computations are inert until interpreted, while ordinary strict arguments
  are evaluated according to PureScript semantics.
- Operation results match their checked signature. Safe dispatch cannot exchange
  unrelated payload/result types.
- A handler removes exactly its advertised operation requirement and preserves
  residual requests and the appropriate continuation.
- The pure boundary requires a closed empty operation row. This is not a proof about
  unsafe coercions or arbitrary FFI hidden in user code.
- A suspension is observable control: it is never erased, reordered across, or
  duplicated by an optimizer, and a converted function is never merged into a
  direct one without preserving its conversion.
- A resumption is resumed only at its captured result type, and a one-shot
  resumption is transferred at most once, atomically with respect to re-entrancy.
- Deep and shallow handlers behave as declared; the lowering does not silently
  change the semantics.
- Captured frames keep their values valid until the resumption is dropped, resumed
  to completion, or cancelled; a resumption is never retained across a
  non-suspendable foreign boundary.
- Closure conversion, adapters, and optimizations preserve evaluation behavior
  under ordinary IR contracts; they do not verify a private effect protocol.
- Resource safety is a separate contract. A bare handler does not prove
  release-on-trap or prevent use-after-release.

Evidence must separately cover type rejection, pure interpretation, generic
representation, the lowered control mechanism (frame identity, deep/shallow,
one-shot/multi-shot, stack safety), and any native control capability. The
[acceptance checklist](../../../implementation/backend/algebraic-effects.md)
records the requirements.

## Worked example

A schematic library program:

```purescript
program :: Run (state :: StateF Int) Int
program = do
  put 41
  value <- get
  pure (value + 1)

result :: { state :: Int, value :: Int }
result = runPure (handleState 0 program)
```

Constructing `program` does not allocate a mutable cell or execute native IO. Under
the library encoding, interpretation changes the handler's ordinary state value to
41, supplies 41 to the continuation of `get`, and returns
`{ state: 41, value: 42 }`. The compiler sees constructors, functions, and calls;
there is no State token to project, no write/read dependency graph, and no
operation-specific witness.

Under the native mechanism, the same program is a small frame chain. Tracing the
trampoline:

```text
run(handleState 0 program)
  Put 41     -> handler state 0 becomes 41; continue
  Get k      -> handler supplies 41 to k; k is a Resumption of result Int
  Return(42) -> the state handler returns { state: 41, value: 42 }
```

Each `Put` and `Get` builds a frame from the current point to the `handleState`
delimiter, delivers it to the handler, and resumes through the trampoline. Depth of
`do` binding does not grow the native stack.

Adding a Console slot leaves that slot in the residual row after `handleState`. A
pure log-collecting handler can remove it for testing. A host-writing handler needs
an explicit native IO boundary, proposed in the
[native Effect/ST topic](native-effect-and-st-handlers.md); `runPure` cannot be used
as that boundary.

## Boundaries and interfaces

Source syntax, type arrows, and standard row rules remain unchanged. The generic
control surface is abstract and library-facing; it is not new effect syntax. The
compiler adds no ambient effect inference and does not recognize a `Run` or
`Effect` constructor specially.

The library and the compiler divide cleanly: operations, rows, handlers,
continuation composition, and the one-shot/multi-shot API are library code; only the
suspension/delimiter/resumption mechanism and its lowering are compiler code. A
library cannot make the optimizer unsound, and the compiler cannot change an
operation's meaning.

Native services remain explicit checked bindings with ordinary value/ABI contracts.
Command execution uses an ordinary configured runner where supported, without
discovery of `Effect Unit` or hidden interpretation of operation rows. This topic
does not reopen the deleted legacy runner path.

The baseline target profile does not gain stack switching through this design. A
native resumption option requires a pinned target capability, an MIR/lowering
contract, validator coverage, and actual execution evidence. An unsupported
capability is an error, never a silent change to continuation semantics.

## Open questions and future work

1. Confirm the final type signatures and the abstract carrier of the control
   surface, including how the library links operation payload types to resumption
   result types with a rank-N eliminator.
2. Fix the boundary between selective conversion and the existing tail-call,
   closure, and dictionary lowerings, and specify which optimizations must treat a
   suspension as observable.
3. Decide the default representation for one-shot versus multi-shot resumptions and
   how a library selects each without weakening the other.
4. Derive the exact rejection rules for capturing across a host/foreign frame,
   including canonical-ABI and WIT boundaries.
5. Audit the pinned upstream Run/Free/Variant packages and this compiler's generic
   representation support; no compile/runtime acceptance has been measured for this
   design.
6. Validate the separate [native Effect/ST proposal](native-effect-and-st-handlers.md).
   Its current package consumers remain unsupported after compiler cleanup.
7. Measure interpreter allocation, frame churn, and deep execution to set the
   performance contract for the lowering.
8. Scoped/higher-order effects, bracket/finalization, async, and concurrency need
   their own semantics. Ordinary first-order operation rows do not provide those
   guarantees automatically.

## References

- [DEC-03](../../../decision/DEC-03-purescript-faithful-type-system.md),
  [DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md), and
  [compiler cleanup handoff](../../../implementation/backend/effect-state-removal-handoff-2026-10-11.md).
- [PureScript Run](https://github.com/natefaubion/purescript-run) and its
  [API](https://pursuit.purescript.org/packages/purescript-run/5.0.0/docs/Run).
- [PureScript Free](https://github.com/purescript/purescript-free) and
  [Stack Safety for Free](https://functorial.com/stack-safety-for-free/index.pdf).
- [Continuing WebAssembly with Effect Handlers](https://arxiv.org/abs/2308.08347)
  and the [WebAssembly stack-switching proposal](https://github.com/WebAssembly/stack-switching),
  a proposal-level reference, not evidence of this target profile's support.
- [Capability profile](../wasm/capability-profile.md) and
  [DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md) for the target features
  and the rule that runtime support is not a contract.
