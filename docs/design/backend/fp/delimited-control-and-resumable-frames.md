# Delimited Control and Resumable Frames

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Selected design; contract v2 is partially implemented, complete
acceptance remains outstanding. The mechanism is selected by
[DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md).
**Prerequisites:** [DEC-03](../../../decision/DEC-03-purescript-faithful-type-system.md),
[DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md),
[functional Core](../../frontend/semantics/functional-core.md),
[CC IR](cc-ir.md), [MIR](mir.md),
[control flow and tail calls](control-flow-and-tail-calls.md),
[representation and evidence](representation-and-evidence.md),
[capability profile](../wasm/capability-profile.md), and the
[library-owned effects](algebraic-effects-and-handlers.md) this mechanism runs.  
**Summary:** The compiler owns an effect-agnostic monadic `Control` surface,
lowered to persistent GC continuation frames driven by a guest loop. The existing
`handle` is generalized to typed source-to-target elimination, with clauses outside
source dispatch and deep/shallow policy in library code. The separate closed `run`
remains a state fold. [DEC-20](../../../decision/DEC-20-typed-control-elimination.md)
and [the scope contract](control-elimination-and-handler-scopes.md) specify this
revision. Focused v2 implementation and execution evidence are recorded in the
acceptance checklist; full verifier and foreign-boundary obligations remain open.

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

The carrier is opaque and monadic over an arbitrary request functor. No `Functor`
constraint or compiler effect-row reasoning is required.

```text
return  : a -> Control f a
bind    : Control f a -> (a -> Control f b) -> Control f b
perform : f a -> Control f a
run     : s
        -> (forall x. s -> f x -> (x -> Control f a) -> (s, Control f a))
        -> Control f a -> (s, a)
handle  : (a -> Control g b)
        -> (forall x. u x -> (x -> Control u a) -> Control g b)
        -> Control u a -> Control g b
```

`return`, `bind` and `perform` construct inert data; `perform` becomes observable
only when interpreted. `bind` runs saved work before consumer work. `run` is a
closed state-threading fold; its clause returns the next source computation.
`handle` constructs an inert source-to-target delimiter. Its raw continuation
constructs the original source remainder without reinstalling the handler. Its
return and operation clauses produce target computations outside source dispatch.
The [scope contract](control-elimination-and-handler-scopes.md) defines exact
phase, lifetime, library derivation and compatibility obligations.

A flat guest loop interprets this data, including nested delimiters. A target
answer of inert `Control g b` needs no synchronous recursive interpreter call.
An arbitrary synchronous answer `r` would require a different contract; this
fact does not justify excluding typed target computations from a trampoline.

The surface is monadic. Direct-style syntax is separate future work. Operations
may use a closed functor, an open `VariantF`, an existential or a rank-N library
encoding; the compiler learns no concrete effect. Reference Run exposes source
to target-monad elimination, but its API is not identical to this surface and
its pinned package acceptance remains unverified.

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

The compiler owns the opaque carriers and eight generic control operations. They are declared as a
compiler-reserved interface module, `Prim.Control`, the way `Prim.Coerce` exposes
`coerce`: `Prim.Control` declares the abstract type `Control` and the
`return`/`bind`/`perform`/`run`/`handle` and `once`/`resume`/`cancel` values, whose schemes the compiler checks against the
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

The closed `run` fold invokes its clause with state, request and raw source
continuation, and drives the returned next computation. This convenience does
not implement complete scoped-handler semantics. The generic `handle` instead
separates source `Control u a` from target `Control g b`; its callbacks return
inert target data to the shared loop. State, projection and reinstatement are
ordinary library functions. Complete typed deep/shallow derivations and the
return-clause contract are in
[control elimination and handler scopes](control-elimination-and-handler-scopes.md#library-derivations).

A library deep handler removes a selected effect only by actually transforming
source requests to target requests. A shallow remainder retains its source
request domain. Moving an old source-domain clause to target dispatch without
that typed transformation would be unsound.

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

**Elimination and the representation.** A trampoline can interpret a handler's
inert target answer `Control g b` while preserving its raw source remainder
`Control u a`. This provides answer-type separation without a synchronous
`x -> r` interpreter. The former state-fold-only argument incorrectly excluded
this case. The selected `handle` uses typed source and target contexts; `run`
retains the closed state-fold convenience. Concrete state and open-union policy
belong in the library, not compiler selectors.

#### Selective conversion

Selectivity applies to checked control operations and their continuation/context
representation. It is not whole-program dynamic-stack capture. Ordinary calls,
including callbacks that construct `Control` data, keep the existing callable
conventions. `bind` explicitly identifies saved continuation work; interpretation
reifies and captures that work only to its source delimiter. Calling `perform`
constructs data rather than suspending a live pure call stack.

A future direct-style transform would need its own conversion and foreign-frame
contracts. This monadic design requires no conservative call-graph analysis that
turns every unknown higher-order caller into a resumable function. Optimizers
preserve strict argument evaluation, inert construction and observable execution
separately. They may remove unused inert data, but may not erase, reorder or
duplicate interpretation callbacks as though they were pure constructions.

#### Frame construction

The machine uses inert `Return`, `Perform`, `Bind`, raw `Resume`,
`Handle(returnClause, operationClause, body)` and `OwnedResume` nodes. Pending
continuations use immutable `Empty`, `Cons` and `Append` sequences. Raw resume
checks checkpoint liveness, runs the saved sequence before its consumer queue,
and does not automatically reinstall source dispatch. Capture excludes caller
work. Abandonment clears the source queue from the driver; only explicit resume
may run it. Persistent queues support replay without cloning user mutable data.

Explicit heap contexts distinguish source normalization from target execution.
A clause's target answer runs under enclosing dispatch with an extent-finish
marker. The [scope contract](control-elimination-and-handler-scopes.md#machine-phases-and-scope)
specifies context identities, capture provenance and finish boundaries.

CC layout predeclares computation/context layouts and all executor/callback
SignatureIds. Lowering synthesizes bodies using these declarations and cannot
mutate the representation table. Checked source/target contract identities remain
available for verification after payload erasure. The previous open-union
`Handle`/`Reenter` executor is v1 implementation evidence, not this normative shape.

#### Open elimination and residual forwarding

The compiler receives opaque source requests and invokes the supplied operation
clause. Library code projects the union and constructs target residual requests.
It decides whether to wrap the raw source remainder in a new handler. It also
constructs state/result records through ordinary code. Neither union selectors
nor a compiler-owned injection argument are part of revised `handle`.

Forwarding suspends target execution. Capturing and replaying that target context
must preserve nested source checkpoint provenance and instantiate lifecycle
frames per replay, as required by the scope contract. Shared guest context
transitions avoid native recursion per handler or forwarded request.

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

This is a schematic driver, not the exact constructor inventory. The implemented
CC machine uses `Return`, `Perform`, `Bind`, `Handle`, `Resume` and `OwnedResume`,
with explicit queues and source/target contexts.

Because the loop drives control steps, the native Wasm stack does not grow with
the depth of interpreted binds or with long machine request sequences. Ordinary
recursive user callbacks are not converted into frames by this monadic lowering;
their stack behavior follows ordinary call and tail-call lowering. Using
`return_call`/`return_call_ref` for the tail positions of the emitted steps keeps
the constant-stack property even where the machine calls into ordinary code.

#### Deep and shallow handlers

Deep library code reinstalls `handle` around the raw source remainder; shallow
code resumes that remainder directly. Both operation and return clauses execute
outside the source handler. Deep output may remove the selected effect; shallow
output generally retains it. The typed library definitions in the
[scope contract](control-elimination-and-handler-scopes.md#library-derivations)
are authoritative. A compiler mode flag or implicit raw-continuation reinstallation
would change those semantics. Current v1 fold tests do not verify this contract.

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

A reachable continuation keeps its captured values as GC roots. Multi-shot use
neither consumes it nor releases those roots automatically. Completion of its
extent makes it unusable; roots may remain reachable if user code retains the
expired closure. Abandonment makes frames collectible only when no references
remain. Spent one-shot owners clear their private callback captures. Lifetime
validity and heap reachability are separate facts, and optimizations preserve both.

Every interpreted closed `run` opens a lifetime extent and closes it before
returning. Both calling a captured continuation and interpreting an already
constructed resume node check liveness. Moving data into another runner cannot
bypass expiry.

For `handle`, source dispatch removal and extent completion are distinct. Its
extent stays live during its target clause answer and closes when that answer
finishes, even when the parent runner remains live. Replay instantiates compiler
activation frames; it does not share a mutable finish token between independent
branches or clone user state. Parent completion expires abandoned children.
[Lifetime and replay](control-elimination-and-handler-scopes.md#lifetime-and-replay)
defines checkpoint provenance and safe tail finalization groups. Long deep
sequences must not build unbounded inactive lifetime ancestry merely because
library code tail-returns another handler.

Lifetime metadata consists of GC frames and private liveness state, not a native
stack handle. Persistent queues remain replayable while their checkpoint is live.

This is the software-continuation cost model: because the frames are heap objects,
the ordinary GC finds their roots and no separate root information is needed. A
native continuation moves the same obligation to the runtime, which scans captured
stack slots for roots and owns the stack's lifetime
([Software and native continuations](#software-and-native-continuations)).

#### Foreign-call boundaries

Suspension cannot cross a non-suspendable host or foreign frame: a captured
continuation that spans a canonical-ABI call or an in-progress WIT import cannot be
resumed without that boundary's cooperation. The lowering must reject a conversion that
would capture across such a boundary unless the boundary declares an explicit
suspension protocol. A continuation also cannot retain a borrowed canonical-ABI
buffer or a host-resource reference past the boundary's lifetime.

The current explicit monadic lowering captures guest queue/context data, not
native host frames. This does not discharge captured-resource lifetime checks or
caught-host-trap cleanup. Those remain required acceptance obligations; no
complete foreign-boundary verifier or recovery protocol is claimed here.

### The executor and the runtime boundary

The eliminator's driver — the trampoline of [The trampoline](#the-trampoline) — is the
**executor**: the component that interprets `Control` data through typed contexts
and dispatch clauses; closed `run` also threads explicit handler state. Where it lives is a design decision with a strict boundary.

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
- Output: `(s, a)` for the closed `run`. Interpreting scoped `handle` changes
  from source `Control u a` to target `Control g b`, with an explicit finish context.
- It invokes the matching clause once at each source return/request boundary.
  Nested source/target and forwarding transitions use the shared guest loop;
  library callbacks return data rather than invoking a synchronous inner executor.
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

1. Consume checked primitive occurrences, including callable aliases and partial
   applications, using their authoritative Core contracts.
2. Predeclare computation, sequence and context representations plus signatures
   in CC layout before lowering synthesizes any bodies.
3. Lower construction to inert data and interpretation to the guest context loop;
   preserve source spans, source/target domain identities and capture provenance.
4. Leave ordinary functions in the existing calling convention. Adapt callback
   payloads through the shared checked representation operations.

This procedure does not infer effects from function names or unknown callees and
does not capture the native stack of ordinary pure code.

### Perform and continuation

Constructing `perform(request)` stores inert data. At interpretation, the current
source delimiter captures only its pending source queue and supplies a raw
continuation to its operation clause. No callback runs during construction.
The source clause returns target data; saved work executes only if it uses the
raw continuation. Calling that continuation constructs a checked `Resume` node.

### The executor loop

Sequence normalization consumes `Bind`, `Return` and `Resume` iteratively.
`Append(saved, consumer)` runs saved work first without mutating either sequence.
Closed `run` supplies its state-fold clause and final result boundary. Scoped
`handle` saves caller work, normalizes source work, removes source dispatch before
calling either clause, and interprets the target answer under enclosing dispatch.
An explicit finish context closes the extent only at target completion.

The complete transition procedure is in
[scope algorithms](control-elimination-and-handler-scopes.md#algorithms).
Nested context transitions use the shared guest loop. User callbacks' arbitrary
non-tail recursion remains outside the machine guarantee. Invalid tags or contract
mismatches fail explicitly.

### Deep-handler reinstatement

Reinstatement is a library `handle` construction around a raw resumed source.
Shallow use omits that construction. Lowering preserves these ordinary typed
operations; it does not inspect callback bodies to infer concrete handler policy.

### One-shot consumption

```text
transfer(k):
    atomically: if k.status != Pending: reject
                k.status = Running
    ... normal completion sets k.status = Consumed ...
    ... caught-trap recovery requires the separate cleanup contract ...
    ... cancellation sets k.status = Cancelled ...
```

The atomic transition is what makes one-shot reuse rejection a runtime guarantee
rather than a documented expectation.

## Code map

The source primitives and guest machine exist. This map states their owners and
required responsibilities; it does not imply every verifier or foreign-boundary
obligation is implemented.

| Location | Intended responsibility |
| --- | --- |
| `Prim.Control` owner | The abstract `Control f a` carrier and the `return`/`bind`/`perform`/`run`/`handle` primitives, with their checked schemes |
| CC control lowering and MIR | Persistent sequencing and typed contexts below Typed Core; preserve library reinstatement, lifecycle provenance and one-shot status |
| Executor (guest code) | The guest loop over typed source/target contexts, persistent queues and lifetime activations; closed run also threads fold state |
| MIR verifier | Frame identity, capture boundary, continuation result-type, and suspension-observability checks |
| Existing Core/CC/MIR lowering and optimizers | Compile the converted frames and the executor as ordinary higher-order code; preserve generic observable-call semantics |
| `psrs-runtime` (software form) | GC heap, leaf primitives, trap, and linking; provides no continuation and must not learn the frame layout |
| Runtime control owner, only if separately adopted | Stack-switching ABI, captured-root storage, lifecycle checks, and capability requirements |

The implementation locations are `psrs-hir/src/intrinsic/registry.rs`, checked
Core control contracts, `psrs-backend/src/cc/layout/control/`,
`cc/lower/control/`, `cc/verify/control/` and `mir/control/`. Layout planning
reserves nominal Control/Resumption carriers and all synthesized signatures
before CC lowering freezes the representation table. Ordinary carrier parameters,
results, captures and joins use these nominal representations; only explicit
abstract user-payload/storage boundaries erase and recover values. The executor
does not reconstruct a complete source type from erased data.

The eight source primitives retain their identities, with `handle` identity 96
and arity three under contract v2. The independent package/compiler contract is
versioned as `control_protocol: 2`. Full row-library and reference-library
acceptance remain distinct from the existing closed/open machine fixtures.
Upstream `Free`/`Run` executes its ordinary library representation; it is not an
alternative implementation of the compiler's Control machine, and the native
Effect/ST prototype is not evidence that this topic is complete.

The current pure Run migration resolves the unchanged Aff import closure and
passes higher-order row-alias kind checking and signature expansion. It still
rejects the `runMonadEffect` instance member in term checking. This is a frontend
migration blocker; native Aff execution has not been demonstrated as a dependency
of the pure entry. The [reference migration record](../../../implementation/backend/control-reference-library-migration-2026-10-12.md)
keeps compilation, runtime evidence and remaining full acceptance separate.

No compiler-side operation registry, handler IR, or effect-specific solver is
required. The runtime carries no continuation for the software form; runtime control
module names and entry-point signatures remain unselected until a target option is
explicitly adopted.

## Invariants and verification

- Interpreting a suspension is observable control and may not be erased, reordered
  or duplicated as a pure construction. Inert data follows ordinary strict evaluation.
- A continuation is resumed only at its captured result type, and a one-shot
  continuation is transferred at most once, atomically with respect to re-entrancy.
- Deep and shallow handlers behave as declared; the lowering does not silently
  change the semantics.
- Reachable captured frames keep their values rooted; expiry follows the target
  extent and multi-shot resume does not consume the checkpoint; a continuation is never retained across a
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

The source-to-target `handle` contract is selected by DEC-20; the separate `run`
remains a closed fold. Migration, context-frame verification, replay activation
identity and finalization coalescing require the evidence in the scope topic.
The following remain open.

Finish production/library migration with focused execution evidence, then close
the complete structural and malformed-IR checks at the stable stage contracts.
Complete delimited-control acceptance precedes native Effect/ST public migration.
Required acceptance scope is unchanged by this implementation order.

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
