# Delimited Control and Resumable Frames

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Draft design. The mechanism is selected by
[DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md).
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
WebAssembly stack switching. The selected activation is the frame/trampoline
representation (row 3) with a state-threading fold eliminator and an open-union
`handle` that removes one effect.

## Scope

This document owns the generic control surface, its identity, the selective
CPS/defunctionalization lowering, the frame representation and trampoline (the
executor), the executor's boundary with the runtime, deep/shallow encoding, one-shot
status, continuation lifetime and GC roots, foreign-call capture boundaries, and the
comparison with native stack switching.

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

### Relationship to the withdrawn State mechanism

This mechanism is not the removed compiler-owned State mechanism, and a frame chain
is not a state chain. The State mechanism (`Prim.State`, the `runWorld`/`runRegion`
intrinsics, and the Core/CC/MIR dependency graph) encoded a **linear token** whose
flow through calls a static analysis verified and then erased at MIR. It could order
effects but could not capture, store, or resume control. A frame chain is a
**runtime continuation**: heap data recording the rest of the computation, delivered
to a handler and resumable zero, one, or many times.

They share only that both are compiler-owned representations of "the rest of the
computation", and both risked cross-pass coupling. The State mechanism was removed
for that coupling. This mechanism avoids it by being effect-agnostic — it knows only
an opaque request functor — and additive, below Typed Core, which the State
mechanism was not.

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

### Software and native continuations

A continuation can be implemented in two fundamentally different places, and this is
the distinction the rest of this document turns on.

- **Software continuation (heap).** The compiler reifies the rest of the computation
  as ordinary heap data — a computation AST or a frame chain — and a loop drives it.
  The continuation's state lives on the **language heap**, so every reference it
  captured is an ordinary GC root and nothing special keeps it alive. The driver and
  the frame layout are the compiler's (or a library's) responsibility.
- **Native continuation (stack).** The runtime exposes an opaque continuation handle
  (`cont`) with `suspend`/`resume` and captures the **native execution stack** itself.
  The continuation's state lives in runtime-managed stack memory, not the language
  heap. The runtime, not the compiler, performs the stack capture and restore, and
  must therefore scan captured stack slots for GC roots and own the continuation's
  lifetime and, for multi-shot use, stack cloning.

These are not layers of one another. A defunctionalized representation plus a
trampoline is a *software* continuation, built on the heap precisely because the
language cannot capture its own native stack; stack switching is a *native*
continuation, which exists so the compiler does **not** have to build the heap
representation. They are two implementations of the same abstract continuation, and a
mechanism can move from one to the other behind an unchanged surface
([Migration to native control](#migration-to-native-control)).

The distinction sets the cost model. The software form is portable and GC-friendly,
but makes the compiler own the driver, the frame layout, and frame-identity
verification. The native form is cheaper to execute and reaches the direct style, but
requires a target capability and moves stack capture, GC-root scanning, and lifetime
onto the runtime.

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
run     : s
        -> (forall x. s -> f x -> (x -> Control f a) -> (s, Control f a))
        -> Control f a
        -> (s, a)
handle  : s
        -> (forall x. s -> f x -> (x -> Control u a) -> (s, Control u a))
        -> (forall x. u x -> Either (f x) (g x))
        -> (forall x. g x -> u x)
        -> Control u a
        -> Control g (s, a)
```

- `Control f a` is opaque; no pass inspects a request.
- `perform` suspends: it captures the continuation up to the nearest `run`,
  delivers the request value `f a` to that delimiter, and resumes with an `a`.
- `run` is a **state-threading fold**. The driver threads a handler state `s`; the
  handler is rank-2 in the request's answer `x`, receives the current state, the
  request, and the continuation, and returns the next state together with the next
  computation. A stateless handler uses `s = ()`.
- `handle` is the open eliminator: it interprets the `f` operations of a computation
  over a combined effect `u` and leaves the rest as a residual `Control g (s, a)`. The
  library supplies the open union `u` and its split, `project : u x -> Either (f x)
  (g x)` and `inject : g x -> u x`, so the compiler never reasons about rows or
  labels. `run` is the closed case, with an empty residual.
- `bind` composes continuations directly (freer style), so no `Functor f` constraint
  is required and a library functor needs no mapping instance.

The driver is a flat loop:

```text
run h s (Return a)        = (s, a)
run h s (Perform op cont) = let (s', next) = h s op cont in run h s' next
```

It recurses in tail position and the handler never re-enters it, so execution is
constant-stack independently of tail-call support and of the number of requests or
binds. This is the row-3 eliminator. A *CPS-answer* eliminator
`(forall x. f x -> (x -> r) -> r) -> Control f a -> (a -> r) -> r` also threads state
(`r = s -> (s, a)`) but makes the driver *nested*, so it belongs to row 2 and is not
this mechanism's eliminator ([Design space](#design-space),
[Handler interface for a flat loop](#handler-interface-for-a-flat-loop)).

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
an explicit continuation chain, so binding is stack-safe without materializing a tree,
and the handler is handed the continuation directly.

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

**One-shot is a separate library API.** It exposes an opaque
`Resumption f a b`. A library can wrap the multi-shot function received by a
clause once and expose only this owner to its clients. The compiler provides:

```text
once   : (a -> Control f b) -> Resumption f a b
resume : Resumption f a b -> a -> Control f b
cancel : Resumption f a b -> Unit
```

All three type parameters have nominal roles: safe coercion cannot change the
owned request, input or answer contract. `once` allocates independent ownership;
wrapping the same unrestricted function twice deliberately creates two owners.
It does not make the original function linear. Aliases of one owner share its
status, and a library that promises one-shot use must expose that owner rather
than the unrestricted function or a factory for additional owners.

`resume` constructs an inert node and evaluates its arguments strictly. When an
executor interprets that node, it requires `Pending`, enters `Running` before
invoking the wrapped function, and enters `Consumed` when that function returns
the next computation. Re-entrant interpretation or cancellation sees `Running`
and is rejected. A second interpretation of the same node also fails. Once the
wrapped function returns, subsequent suspended work is ordinary `Control` data;
the owner grants one invocation, not a linear-use rule for every value produced
by that invocation. No optimization may consume ownership during construction.

`cancel` immediately transitions a `Pending` owner to `Cancelled`; cancelling a
running, consumed or cancelled owner traps. Cancelling an owner does not cancel
the delimiter or other owners. Discarding an inert resume node leaves ownership
pending. Status transitions use private mutable GC cells and happen without a
callback between the check and update. This is atomic with respect to re-entrancy
under the pinned single-threaded guest profile; it is not a cross-thread atomic
protocol. Enabling shared guest execution requires a separate ownership design.

The callback is stored in a private mutable reference slot and removed on
consumption or cancellation. A retained spent owner does not retain that
callback's captures. The executor's local reference roots a running callback.
Existing unrestricted PureScript arrows remain multi-shot, and no optimization
may silently replace them with an owner.

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

### Handler interface for a flat loop

Row 3's eliminator is the state-threading fold in
[Model](#the-computation-carrier-and-primitives). The handler is called once per
request and returns the next state and computation, so the driver stays flat. For a
State request functor:

```text
h s (Get f)    cont = (s, cont (f s))
h s (Put s' n) cont = (s', cont n)

runState s0 c = run h s0 c          -- (s, a)
```

A stateless handler uses `s = ()` and returns `((), next)`.

**Removing one effect (residual rows).** The fold above interprets the *whole* effect
`f`. The library's `Run r a = Control (VariantF r) a` and
`handleState :: s -> Run (state :: StateF s | r) a -> Run r { state :: s, value :: a }`
need the output to change effect, from `VariantF (label :: f | r)` to `VariantF r`.
The mechanism follows the industry standard — an **open union** indexed by the effect
collection — but keeps the compiler free of it: `handle` is generic over the combined
effect functor `u`, and the library supplies the union and its split.

```text
handle s h project inject c :: Control g (s, a)

go s (Return a)        = Return (s, a)
go s (Perform uop cont) =
    case project uop of
      Left  fop -> let (s', next) = h s fop cont in go s' next   -- handled
      Right gop -> Bind (Perform_g gop) (\x -> go s (cont x))    -- forwarded to residual
```

The library's `VariantF r` is the open union for the row; it supplies
`project : VariantF (label :: f | r) x -> Either (f x) (VariantF r x)` and
`inject : VariantF r x -> VariantF (label :: f | r) x` from ordinary row membership
(`Prim.Row.Cons`/`Lacks`). The compiler therefore never reasons about rows or labels;
the row machinery is library code, as with operations and handlers generally. This is
the row-indexed open union the ecosystem uses (`purescript-run`'s `VariantF`, Haskell's
`Union` with `Member`), realized here with a generic primitive rather than a
compiler-known row.

The driver stays flat for handled requests; a forwarded request builds a residual
`Control g (s, a)` by binding `perform_g gop` to a continuation that resumes handling
with the saved state, so re-entry is bounded by the number of handlers, not by the
computation.

**Why not the CPS-answer eliminator.**
`run :: (forall x. f x -> (x -> r) -> r) -> Control f a -> (a -> r) -> r` also threads
state, by choosing `r = s -> (s, a)`, but its handler's continuation `x -> r` must
produce the answer synchronously, so the driver becomes a *nested* `run`
(`\x -> run h (cont x) k`). That is not a flat loop and loses the row-3 stack
guarantee; it is the row-2 shape, and this mechanism does not adopt it.

### Lowering: selective CPS to resumable frames

The mechanism compiles the regions that can suspend into an explicit continuation
representation. The concept is **continuation-passing style (CPS)**; the chosen
representation is **defunctionalized resumable frames** driven by a **trampoline**.

#### Design space

The design space has three independent axes; earlier drafts were confusing because
they mixed them.

**Axis 1 — continuation representation.**
- *Closure (CPS).* The continuation is a closure; `perform op = \k h -> h op k` and
  `run h c k = c k h`. There is no distinct continuation node: the continuation is
  ordinary function data.
- *Defunctionalized.* The continuation is data: either a *free/freer monad* (the whole
  computation is an AST, `Pure`/`Impure`, with a bind queue) or a *frame chain* (the
  continuation itself is a chain of explicit frame records).
- *Native `cont`.* The continuation is a runtime object capturing the execution stack
  (stack switching).

**Axis 2 — driver.**
- *Tail call.* Each continuation step is a tail call (`return_call*`); constant stack
  only if every step is truly in tail position.
- *Trampoline.* A loop repeatedly takes the next step/computation/frame; constant
  stack regardless of the step's position.
- *Runtime `suspend`/`resume`.* The runtime captures and reinstalls the stack.

**Axis 3 — ownership.** The structure is *library-owned* (the library builds and folds
it, as `purescript-run` does) or *compiler-owned* (the compiler lowers the surface to
it).

**Dispatch.** With a single delimiter there is no dispatch. With several effects or
stacked handlers, a request is routed by the dynamic delimiter chain, by an operation
tag/label, or statically by *evidence passing* (Koka), which removes the runtime
lookup.

The candidate architectures combine the axes as follows.

| # | Representation | Driver | Ownership | Continuation state / roots | Notes |
| --- | --- | --- | --- | --- | --- |
| 1 | free/freer monad AST | trampoline (library) | library | heap; GC scans it | `purescript-run`, `freer-simple`; the library folds, so handlers thread state naturally; little compiler involvement |
| 2 | closure (CPS) | tail call | compiler | heap; GC scans it | smallest lowering, no new backend construct; stack safety depends on every step being a tail call; the continuation is opaque (not verifiable) |
| 3 | frame chain (defunctionalized continuation) | trampoline (compiler-generated loop) | compiler | heap; GC scans it | DEC-19's design; tail-call-independent; frame identity / capture boundary / one-shot status are verifiable |
| 4 | native `cont` | runtime `suspend`/`resume` | compiler + runtime | runtime stack; the runtime scans roots | stack switching; a target capability |

Rows 1–3 are **software continuations** and row 4 is a **native continuation**
([Software and native continuations](#software-and-native-continuations)): rows 1–3
keep the continuation on the language heap, so the GC finds its roots for free, while
row 4 keeps it in runtime stack memory, so the runtime must scan captured stack slots
and own the lifetime. Only row 2 depends on tail calls: rows 3 and 4 are
tail-call-independent — a trampoline loops, and `suspend` captures non-tail frames
just as well as tail ones.

Terminology: *capture/reinstall* of a defunctionalized continuation (rows 1 and 3) is
not *suspend/resume* of a native stack (row 4). The frame model's "self-implemented
suspend/resume" is really *capture/reinstall*, driven by a trampoline; the
suspend/resume words belong to stack switching.

Rows 3 and 1 are the same idea — defunctionalized representation plus an interpreter
loop — differing in what is defunctionalized (the continuation vs the whole
computation) and in ownership, which is an independent axis: a library or a compiler
can own either. Row 2 is not defunctionalized at all: its continuation is a closure
and it leans on tail calls.

**Whole-program CPS** (CPS-transforming every effectful user function) is a further
candidate but is rejected: it changes calling conventions across Core/CC/MIR and needs
adapters between converted and direct functions, for a direct style this mechanism
does not provide (operations and handlers are monadic here).

This document's design is row 3. Row 2 is a shorter alternative: a source CPS State
handler written over a rank-2 newtype field compiled and executed on this target (a
`wasmtime` run returning 42), and the official `purs` compiler accepts the same source.
Row 2 is not rejected for expressibility; row 3 is chosen for the guarantees above.

**The `run` typing is coupled to the representation.** Row 3 naturally supports a
*structure-returning* handler `forall x. f x -> (x -> Control f a) -> Control f a`,
whose result is the next computation, with `run :: ... -> Control f a -> a`. A
*CPS-answer* handler `forall x. f x -> (x -> r) -> r` with a polymorphic `r` is what
threads state (`r = s -> (s, a)`) and changes effects (`r = Control g b`); it is what
row 2 uses directly (`run h c k = c k h`), but a trampoline (row 3) does not obviously
carry it, because the handler's continuation `x -> r` must produce the answer
synchronously.

How existing systems thread handler state:

- Row-1 libraries fold a **concrete** computation, with state in the target monad
  (`StateT`) or in the fold's own recursion. This mechanism's carrier is opaque, so
  the library cannot fold it.
- Row-4 systems (OCaml 5, Koka, Eff) capture the stack and thread state through
  handler-local (often mutable) state.
- CPS encodings (`Cont`, `shift`/`reset`) thread state through the answer type —
  exactly the CPS-answer `run`, i.e., row 2.

This mechanism resolves the coupling as row 3 with the **state-threading fold**
([Handler interface for a flat loop](#handler-interface-for-a-flat-loop)): the driver
threads the handler state, so state threading no longer needs the CPS-answer
eliminator and the driver stays flat. Effect removal uses the open-union `handle`,
whose `project`/`inject` the library supplies so the compiler stays row-agnostic.

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

The monadic surface is represented by inert computation nodes and a persistent
sequence of continuation frames. Constructing a `bind` never interprets its input
computation or invokes its continuation. Strict evaluation of primitive arguments
still follows the ordinary language rules.

```text
Computation = Return(value) | Perform(request) | Bind(inner, continuation)
            | Resume(value, savedSequence, boundary)       -- internal only
            | Handle(state, handler, project, inject, inner)
            | Reenter(state, handler, project, inject, inner, boundary)
            | OwnedResume(owner, value)                   -- internal only
Sequence    = Empty | Cons(continuation, rest) | Append(left, right)
Machine     = (handler, state, computation, pendingSequence)
```

`Bind` records a checked function from the inner result to the next computation.
The executor pushes this function onto its pending sequence and descends into the
inner computation. A `Return` consumes one pending function; that function returns
a computation node to the executor. A `Perform` passes the unchanged request and a
resumption capturing the entire pending sequence to the handler. The resumption
constructs `Resume(value, savedSequence, boundary)`; it does not execute the sequence or
re-enter the executor. A bare `Perform` therefore needs only a request field: its
identity continuation is represented by the empty pending sequence.
Suspension becomes observable when an executor interprets the node. An unused
inert node may be eliminated only while preserving strict argument evaluation;
an interpretation that invokes a handler remains an observable call.

All sequence nodes are immutable GC objects. `Append` joins sequences without
copying them, including when a resumed computation is itself bound by the handler.
The saved sequence runs before the caller's pending sequence. Each normalization
step is driven by the executor loop; recursively nested composition closures are
not the sequence representation. This is the explicit continuation sequencing used
by operational free/freer implementations, adapted to compiler-owned frames.

The handler returns the next state and computation. The executor clears its old
pending sequence: that remainder belongs to the resumption, so a clause may discard
it, resume it once, or construct several independent resumptions. Retaining the old
sequence would execute abandoned work or execute a resumed remainder twice.
Persistent nodes preserve multi-shot replay without mutating shared continuations.

CC layout predeclares these representations and the executor call signature. The
continuation uses the existing unary erased callable-slot signature. The result
pair uses the canonical erased record protocol for `_1` and `_2`; ordinary payload
conversion adapts concrete states, results, handlers, and bind functions. CC lowering
synthesizes function bodies and registers their direct-call signatures, but cannot
mutate the representation table. Signature handles and verifier signatures come
from the same layout declarations.

#### Open elimination and residual forwarding

`handle` constructs an inert `Handle` node. It evaluates its arguments strictly,
but invokes neither the handler nor `project` during construction. Interpretation
enters a second instance of the same sequence machine with a fresh empty sequence;
the caller's pending sequence stays outside this delimiter. Finishing the inner
machine produces `Return((state, value))`, which the caller interprets with its own
pending sequence.

The checked compiler interface encodes `Either` using its Church eliminator:

```text
project : forall x r. u x -> (f x -> r) -> (g x -> r) -> r
```

The machine applies it to two generated selectors, yielding an internal tagged
payload. The handled branch calls the supplied clause and continues the inner
loop with the returned state and computation. The residual branch stops this
inner loop and returns:

```text
Bind(Perform(gop), forward(savedState, handler, project, inject, savedSequence))
forward(x) = Reenter(savedState, handler, project, inject,
                     Resume(x, savedSequence, boundary), boundary)
```

The outer executor delivers `gop` unchanged to its handler. Calling `forward`
only constructs data; re-entry occurs when the outer executor interprets the
returned `Reenter`. Saved state is immutable: separate outer resumptions each
start with the same saved inner state. Subsequent changes within one branch do
not overwrite the other branch's state. An outer clause can abandon the entire
inner delimiter by discarding its resumption.

`inject` remains a checked, strictly evaluated library argument. This machine
retains the original `u` computation and its pending sequence, so forwarding an
answer does not reconstruct a `u` request or call `inject`. It needs no inverse
law between `project` and `inject`. Requests remain opaque throughout.

Both executors use loop-driven sequence normalization. Nested open delimiters
may call the open executor while retaining their caller; stack use can depend on
handler nesting depth, but does not grow with bind or forwarded request count
at fixed handler depth. This guarantee does not cover arbitrary recursive user
callbacks or direct-style capture across foreign frames.

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
  `Pending`, `Running`, `Consumed`, or `Cancelled`. The first interpreted `resume` transitions out of `Pending` without an intervening callback; a second attempt is rejected rather than silently
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

Every interpreted closed `run` allocates a fresh delimiter token. It closes
that token before returning the result. A captured resumption and its `Resume`
node both retain the token: calling the function and interpreting an already
constructed node each check liveness. Moving the node into another `run` cannot
bypass expiry. A live multi-shot resumption may be interpreted re-entrantly under
a nested `run`; it still captures only its original pending sequence.

An open `Handle` allocates a child token referring to its enclosing delimiter.
Completion closes that activation's token. Forwarding suspends it; the saved
checkpoint remains valid while its parent is live. Each residual replay starts
a fresh activation with the same enclosing parent, so completing one replay does
not invalidate the saved checkpoint used by another replay. New resumptions
captured in that replay belong to its own activation and expire when it completes.
Re-entry checks the checkpoint before creating the new activation. Tokens follow
actual delimiter nesting, not forwarded request count; checkpoint re-entry does
not build a new ancestor link for every request. Parent expiry also invalidates
suspended children, including ones abandoned by an outer clause.

Tokens are immutable tagged GC objects containing private mutable liveness cells
and an optional parent. A shared generated loop checks the token and its ancestors.
The token is not an abstract target-native stack handle. Frame payloads and saved
sequences remain persistent, so lifetime checks do not consume multi-shot data.

This is the software-continuation cost model: because the frames are heap objects,
the ordinary GC finds their roots and no separate root information is needed. A
native continuation moves the same obligation to the runtime, which scans captured
stack slots for roots and owns the stack's lifetime
([Software and native continuations](#software-and-native-continuations)).

#### Foreign-call boundaries

Suspension cannot cross a non-suspendable host or foreign frame: a captured
continuation that spans a canonical-ABI call or an in-progress WIT import cannot be
resumed without that boundary's cooperation. The lowering rejects a conversion that
would capture across such a boundary unless the boundary declares an explicit
suspension protocol. A continuation also cannot retain a borrowed canonical-ABI
buffer or a host-resource reference past the boundary's lifetime.

### The executor and the runtime boundary

The eliminator's driver — the trampoline of [The trampoline](#the-trampoline) — is the
**executor**: the component that turns a `Control` value, a handler, and a handler
state into a result. Where it lives is a design decision with a strict boundary.

- **The executor is guest code.** It is a compiler-generated loop over the frame
  representation, or a library function over a compiler-provided frame view. It runs
  in the compiled program because it must call the guest handler and guest
  continuations; a host function cannot call back into arbitrary guest closures
  without an explicit boundary.
- **The host runtime does not interpret frames.** `psrs-runtime` provides the GC heap
  (where frames live), the leaf primitives a request may call (storage, host services,
  trap), and linking. It does not inspect the frame layout, index frames, or own a
  continuation. Nothing about the software representation crosses the host boundary.
- **The runtime may provide a generic driver loop.** A runtime that loops calling a
  guest "step" function is admissible, as long as it treats the step as opaque guest
  code and learns nothing about the frame layout. This is optional; a guest-side loop
  is the baseline.
- **Under a native continuation the executor disappears.** With stack switching the
  runtime captures and resumes the stack itself, so there is no software driver to
  place ([Migration to native control](#migration-to-native-control)).

The executor's contract:

- Input: a `Control` value, a handler, and the initial handler state `s`.
- Output: `(s, a)` for the closed `run`, or a residual `Control g (s, a)` for `handle`.
- It calls the handler once per handled request and never re-enters itself for a
  handled request; forwarded requests build residual data, so re-entry is bounded by
  the handler count.
- It preserves the [invariants](#invariants-and-verification): frame identity, capture
  boundary, and one-shot status.

What the compiler and the runtime each own:

| Concern | Compiler (guest code) | Runtime (`psrs-runtime`) |
| --- | --- | --- |
| Continuation | frame objects and the executor loop | nothing for the software form; `cont` capture/resume for a native form |
| GC roots | frames are GC objects, scanned by the GC | stack scanning only under a native form |
| Leaf operations | ordinary calls to checked bindings | storage, host services, trap |
| Frame layout | defines and verifies it | must not know it |

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

### Migration to native control

The frame representation also eases a later move to WebAssembly stack switching,
because the two share one shape: the continuation is explicit and the suspension
protocol is explicit. The mapping is mechanical — a frame chain becomes a `cont`
reference, the compiler-inserted suspension becomes `suspend`, the trampoline's
resume becomes `resume`, and the one-shot status becomes the native consumption.
The `Control` surface and all library and user code stay unchanged; only the
lowering and the target capability change. A closure/CPS representation would need
an added suspension protocol before it mapped as directly. Adopting the native
option remains a separate target-capability decision with its own evidence.

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

### The executor loop

The closed executor drives both computation and pending sequence in a flat loop:

```text
exec(h, s, Bind(c, k), q) = exec(h, s, c, Cons(k, q))
exec(h, s, Resume(v, saved), q) = exec(h, s, Return(v), Append(saved, q))
exec(h, s, Return(v), Empty) = (s, v)
exec(h, s, Return(v), Cons(k, q)) = exec(h, s, k(v), q)
exec(h, s, Return(v), Append(Empty, q)) = exec(h, s, Return(v), q)
exec(h, s, Return(v), Append(Cons(k, rest), q)) =
    exec(h, s, k(v), Append(rest, q))
exec(h, s, Return(v), Append(Append(a, b), q)) =
    exec(h, s, Return(v), Append(a, Append(b, q)))
exec(h, s, Perform(op), q) =
    let cont(x) = Resume(x, q)
    let (s', next) = h(s, op, cont)
    exec(h, s', next, Empty)
```

Every recursive `exec` edge is a self tail call lowered to a MIR loop. Calls to
ordinary bind functions and handlers return one computation step; they do not
traverse the saved sequence. User code's own non-tail recursion is outside this
machine's stack-safety guarantee. Invalid internal tags trap explicitly.

The open executor uses the same sequence rules. At a request it projects the opaque
union, calls the matching handler for a handled request, or constructs residual
control data with a continuation that reinstates the saved handler state and
sequence:

```text
handle h s project inject (Return a)        = Return (s, a)
handle h s project inject (Perform uop cont) =
    case project uop of
      Left  fop -> let (s', next) = h s fop cont in handle h s' project inject next
      Right gop -> Bind (Perform_g gop) (\x -> handle h s project inject (cont x))
```

`handle` threads the handler state and forwards unhandled requests to the residual;
the loop recurses in tail position for handled requests, so the executor never grows
the native stack with computation depth. A handler that returns without resuming
leaves its continuation unreachable; deep and shallow differ only in whether `h`
re-wraps the continuation.

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
| `Prim.Control` owner | The abstract `Control f a` carrier and the `return`/`bind`/`perform`/`run`/`handle` primitives, with their checked schemes |
| CC control lowering and MIR | Persistent continuation sequencing and synthesis below Typed Core; selective conversion, deep/shallow wrapping, and one-shot status |
| Executor (guest code) | The loop that drives frames against a handler and threads its state; compiler-generated or a library function over a frame view; never a runtime interpreter of the frame layout |
| MIR verifier | Frame identity, capture boundary, continuation result-type, and suspension-observability checks |
| Existing Core/CC/MIR lowering and optimizers | Compile the converted frames and the executor as ordinary higher-order code; preserve generic observable-call semantics |
| `psrs-runtime` (software form) | GC heap, leaf primitives, trap, and linking; provides no continuation and must not learn the frame layout |
| Runtime control owner, only if separately adopted | Stack-switching ABI, captured-root storage, lifecycle checks, and capability requirements |

No compiler-side operation registry, handler IR, or effect-specific solver is
required. The runtime carries no continuation for the software form; runtime control
module names and entry-point signatures remain unselected until a target option is
explicitly adopted.

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

For the software form the runtime carries no continuation: the executor is guest code
and `psrs-runtime` provides only the GC heap, leaf primitives, and trap
([The executor and the runtime boundary](#the-executor-and-the-runtime-boundary)).

The baseline target profile does not gain stack switching through this design. A
native resumption option requires a pinned target capability, an MIR/lowering
contract, validator coverage, and actual execution evidence. An unsupported
capability is an error, never a silent change to continuation semantics.

## Open questions and future work

The eliminator is settled — the state-threading fold with the open-union `handle`
([Handler interface for a flat loop](#handler-interface-for-a-flat-loop)). The
following remain open.

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
   lowering's performance cost.

## References

- [DEC-03](../../../decision/DEC-03-purescript-faithful-type-system.md),
  [DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md), and
  [compiler cleanup handoff](../../../implementation/backend/effect-state-removal-handoff-2026-10-11.md).
- [Reflection without Remorse](https://okmij.org/ftp/Haskell/Reflection.html) for
  explicit type-aligned continuation sequences.
- [Continuing WebAssembly with Effect Handlers](https://arxiv.org/abs/2308.08347)
  and the [WebAssembly stack-switching proposal](https://github.com/WebAssembly/stack-switching),
  proposal-level references, not evidence of this target profile's support.
- [Capability profile](../wasm/capability-profile.md) and
  [DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md) for the target features
  and the rule that runtime support is not a contract.
- [Algebraic effects and handlers](algebraic-effects-and-handlers.md) for the
  library-owned operations and handlers this mechanism runs.
