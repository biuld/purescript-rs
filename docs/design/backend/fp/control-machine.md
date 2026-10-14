# The Control Machine: Representation and Driver

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)
**Status:** Draft design. The node inventory, lowering and flat driver match the
implementation; complete verifier and lifecycle acceptance remains outstanding.
The mechanism is selected by
[DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md).
**Prerequisites:** [DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md),
[delimited control and resumable frames](delimited-control-and-resumable-frames.md),
[control elimination and handler scopes](control-elimination-and-handler-scopes.md),
[CC IR](cc-ir.md), [MIR](mir.md),
[data representation](data-representation.md),
[control flow and tail calls](control-flow-and-tail-calls.md), and
[representation and evidence](representation-and-evidence.md).
**Summary:** This document specifies the compiler-generated continuation machine
that the `Control` surface lowers to: its node inventory, how each layer encodes
it, how lowering produces it, and how one flat driver traverses it. It fixes the
concrete machine the rest of the delimited-control design refers to.

## Scope

This document owns the compiler-generated continuation machine: the inventory and
representation of the inert computation, pending-sequence, dispatch-context,
lifetime-extent and one-shot-owner nodes; the generated helper signatures; the
per-layer encoding from Core through CC and MIR to Wasm; the lowering that
produces the machine; the flat driver and how it traverses a saved continuation;
and the machine's own invariants and verification obligations.

It does not own the `Control f a` surface, the eight primitives, or the
source/target elimination contract — those are
[delimited control and resumable frames](delimited-control-and-resumable-frames.md)
and [control elimination and handler scopes](control-elimination-and-handler-scopes.md).
It does not own the library effect encoding
([library-owned algebraic effects](algebraic-effects-and-handlers.md)), the target
structured encoding ([Wasm encoding](../wasm/encoding-and-structuring.md)), or the
runtime ([linking and runtime](../wasm/linking-and-runtime.md)). It defines the
machine those stages produce, consume, or leave untouched.

## Background

A defunctionalized continuation is the continuation represented as data rather
than as a closure or a captured native stack. The
[design space](delimited-control-and-resumable-frames.md#design-space) of
delimited control selects a **frame chain** — inert nodes linked by an explicit
pending sequence — driven by a **compiler-generated loop** rather than by tail
calls or runtime stack switching. This machine is that frame chain and that loop.

The choice has three consequences this document makes concrete:

- The continuation is ordinary heap data, so the collector finds its roots without
  runtime help and the same captured sequence may be traversed more than once.
- The compiler owns the frame layout, so it can state and verify frame identity,
  capture boundaries and one-shot status as ordinary IR contracts.
- Stack safety comes from the driver loop, not from a native stack, so it does not
  depend on every step being a tail call.

The machine is effect-agnostic. It never inspects a request, an answer or a
handler; the library decides what an operation means.

## Model

### The continuation machine

This machine realizes row 3 — a defunctionalized frame chain driven by a
compiler-generated loop — of the
[design space](delimited-control-and-resumable-frames.md#design-space). The eight
operations lower to an inert node graph interpreted by one flat driver
(the executor loop, [The flat driver](#the-flat-driver)). The node graph *is* the
captured continuation: there is no separate stack object, only persistent nodes
reachable from the driver, plus one mutable status cell per lifetime extent and
one mutable callback/status pair per one-shot owner. This subsection fixes the
value inventory and the driver's entry contract; the per-representation encodings
are in [Representation across IR layers](#representation-across-ir-layers).

Source payloads (`f a` request, `a` answer, user state, callbacks) cross the
machine boundary erased, because the machine is polymorphic over them and never
inspects them. The machine's own fields — computations, queues, contexts,
extents and owners — are nominal: their representation is planned before
lowering and never reconstructed from an erased value. Only the explicit
payload/storage boundary erases and recovers a source value.

**Computation nodes.** `Control f a` is a tagged node. `f` and `a` are erased; a
payload field below is erased unless the node holds another machine value.

| Tag | Node | Fields | Meaning |
| --- | --- | --- | --- |
| 0 | `Return(a)` | answer (erased) | a completed source answer |
| 1 | `Perform(f a)` | request (erased) | an unhandled request |
| 2 | `Bind(inner, k)` | inner computation, unary callback | run `inner`, then `k` on its answer |
| 3 | `Resume(v, q, e)` | answer (erased), pending queue, captured extent | a raw continuation invocation |
| 4 | `Handle(ret, op, body)` | return clause, operation clause, body | an inert source-to-target delimiter |
| 5 | `OwnedResume(owner, v)` | one-shot owner, answer (erased) | a one-shot resumption invocation |

Tags 0, 1, 2, 4 and 5 are the public constructors of `return`, `perform`, `bind`,
`handle` and `resume`. Tag 3 is not a source constructor: only the raw
continuation code target builds it, so `Resume` (tag 3, the raw node) and
`OwnedResume` (tag 5, the public `resume` node) are distinct even though both
invoke a captured continuation. An unknown tag reaches the driver's trapping
default.

**Pending sequence.** The pending queue is a persistent, shareable sequence.
Nodes are never mutated, so a captured sequence may be traversed again by a
second resumption.

| Tag | Node | Fields | Meaning |
| --- | --- | --- | --- |
| 0 | `Empty` | — | no pending work |
| 1 | `Cons(k, rest)` | unary callback, tail | apply `k` to the current answer, then `rest` |
| 2 | `Append(left, right)` | left, right | run `left` before `right` |
| 3 | `Finish(e, rest, parent)` | extent, remaining, parent | close `e`, restore `parent`, then run `rest` |

`Append` is associative but not flattened; the driver reassociates it lazily so
that a shared sequence tail keeps its identity and is never copied.

**Dispatch context.** The context records where requests are currently handled
and where completion returns to. It is a two-case node.

| Tag | Node | Fields |
| --- | --- | --- |
| 0 | `Root(e)` | root extent `e` |
| 1 | `Scoped(ret, op, callerQueue, callerExtent, callerContext, ownedExtent)` | return clause, operation clause, caller's pending queue, caller's extent, caller's context, this delimiter's extent |

`Root` is the closed-`run` state fold: requests are handled by the `run` clause
in place. `Scoped` is a `handle` delimiter: its clauses run in the caller's
context, and the delimiter's own extent bounds the captured continuation.

**Lifetime extent.** One extent per delimiter, plus one for each replayed
activation. The extent is a two-case node over a one-element mutable status
cell.

| Tag | Node | Fields |
| --- | --- | --- |
| 0 | root extent | status cell |
| 1 | child extent | status cell, parent extent |

The status cell holds `0` while live and `1` once closed. Closing is idempotent
for the checker but is applied exactly once per delimiter on its normal
completion path. A closed extent rejects any later capture or resume.

**One-shot owner.** `Resumption f a b` is a single-case node over two one-element
cells.

| Tag | Node | Fields |
| --- | --- | --- |
| 0 | `Owner(callback, status)` | erased callback slot, integer status cell |

The status cell is `0` Pending, `1` Running, `2` Consumed, `3` Cancelled.
On acquisition, the callback is loaded into a local root and its slot is
overwritten with the owner's status-cell reference before the callback runs.
Cancellation also clears the slot. The replacement cannot root the callback's
captures. The mutable callback/status cells and extent status cells are the only
mutable machine storage; computation, queue and context fields stay immutable.

**Driver and helper signatures.** The driver and its helpers are ordinary
functions with predeclared signatures, so a call is checked by identity before
it is lowered.

```text
executor        : (state, handler, computation, queue, extent, context) -> state/answer
cloneSequence   : (queue, todo, reversed, active, parent) -> (queue, extent)
rebuildSequence : (reversed, result, active, parent) -> (queue, extent)
start           : () -> extent
check, close    : extent -> extent
once            : callback -> owner
cancel          : owner -> owner
```

`state` and `handler` belong to closed `run`; a scoped delimiter threads them
unchanged. `handler` is the `run` clause, `once`/`cancel` are the one-shot API,
and `start`/`check`/`close` manage extent lifetime. [The flat driver](#the-flat-driver)
defines how these are used.

## Design

### Representation across IR layers

The same machine value has a different encoding at each layer. No layer recovers
a machine value from an erased source payload, and no layer reconstructs a source
type from a machine node.

**Core.** `Control f a` and `Resumption f a b` are opaque nominal carriers
(`CONTROL`, `RESUMPTION`); the eight operations stay intrinsic calls. Each
checked occurrence carries a `CheckedControlContract` — result type, argument
types, and the source/target `(request, answer)` domains. Core introduces no node
and no payload layout.

**CC.** The immutable representation table stores target-neutral symbolic values.
The computation, pending sequence, dispatch context and lifetime extent are
`Variant`s over their case tables
([The continuation machine](#the-continuation-machine)); the callback and status
slots are one-element `Array`s; the replay result is a `Product` of a queue and an
extent. `CONTROL` and `RESUMPTION` reserve nominal identities so source
parameters, results, captures and joins keep the machine shape. Every field is a
`ValueShape`: a machine field is a nominal `Reference(Repr)` or a closure
signature, while a source payload is `Erased`. The executor and helper signatures
are declared before any body is lowered, and the table is frozen afterwards.

**MIR.** A variant lowers to a non-final abstract supertype that carries only an
immutable `i32` tag at field 0, plus one final subtype per case that repeats the
tag and appends the case fields (see
[data representation](data-representation.md#concrete-layouts)). Construction
emits `struct.new` at the case type, then `ref.cast` to the common type when one
is declared; a tag read casts to the supertype and reads field 0; a field
projection casts to the case type and reads field `index + 1`. Arrays and products
lower to GC arrays and structs. Source payloads become the target's erased
reference. The executor's self tail calls become a preheader that forwards the six
parameters to a loop header, and every self call becomes a back edge to that
header.

**Wasm.** The structurer encodes the loop as a Wasm `loop`, the computation
dispatch as `br_table` over the `i32` tag, and projections and constructions as
GC struct/array operations. Stack safety follows the loop, not a native stack.
The [structured encoding](../wasm/encoding-and-structuring.md) owns these
encodings.

### Lowering the machine

Lowering fixes the representation before it emits any code, so the synthesized
bodies can only consume the plan:

1. **Reserve and plan.** Before source signatures and aggregate layouts, reserve
   the nominal `Control`/`Resumption` identities. Then append the carrier and
   helper tables and allocate the generated symbols: the raw resumption code
   target and the eight helpers (`executor`, `cloneSequence`, `rebuildSequence`,
   `start`, `check`, `close`, `once`, `cancel`). Publish the helper contracts and
   the raw-resumption contract in the representation table, and quote each
   checked Core contract into a plan that maps every primitive to a computation
   case or a helper call.
2. **Lower operands.** For each primitive occurrence, lower the arguments into
   their declared storage roles: a source payload through the shared erased
   payload protocol, a callback through the shared curried callback protocol, a
   machine carrier directly as its nominal reference. A checked `ControlEvidence`
   assignment records the proof and the chosen implementation.
3. **Construct frames.** `return`, `perform`, `bind`, `handle` and `resume`
   simply build their computation node. `run` builds an empty queue, a root
   extent, a `Root` context, calls the executor on
   `(state, handler, computation, empty, boundary, root)`, and closes the
   boundary. `once` and `cancel` call their helpers.
4. **Synthesize helpers.** On first use, synthesize the raw resumption target and
   the eight helpers from the fixed tables, and verify each synthesized function
   before publishing it. Bodies are built from the same shared operations that
   ordinary lowering uses, so no helper gets a private representation path.
5. **Lower to MIR.** The ordinary CC-to-MIR pass resolves symbolic
   representations to physical layouts, and the tail-call pass loopifies the
   executor's self calls. The MIR verifiers recheck the machine relations in
   [Invariants and verification](#invariants-and-verification).
6. **Structure to Wasm.** The structured encoding emits the loop and its
   dispatch; nothing control-specific survives into the encoder.

The generated symbols are ordinary functions with predeclared signatures, so
their calls are checked by identity and lowered like any other call. Nothing in
this pipeline recognizes a concrete effect, operation or handler.

### Capture and frame construction

Raw resume checks checkpoint liveness, runs the saved sequence before its consumer
queue, and does not automatically reinstall source dispatch. Capture excludes
caller work. Abandonment clears the source queue from the driver; only explicit
resume may run it. Persistent queues support replay without cloning user mutable
data.

Explicit heap contexts distinguish source normalization from target execution.
A clause's target answer runs under enclosing dispatch with an extent-finish
marker. The [scope contract](control-elimination-and-handler-scopes.md#machine-phases-and-scope)
specifies context identities, capture provenance and finish boundaries.

CC layout predeclares computation/context layouts and all executor/callback
SignatureIds. Lowering synthesizes bodies using these declarations and cannot
mutate the representation table. Checked source/target contract identities remain
available for verification after payload erasure. The previous open-union
`Handle`/`Reenter` executor is v1 implementation evidence, not this normative shape.

### The flat driver

The machine is run by one flat function, the **executor**. It is not a recursive
interpreter: every transition computes the executor's own next parameters, and
MIR lowering turns those self tail calls into one loop. The executor's state is
exactly its six parameters:

```text
(state, handler, computation, queue, extent, context)
```

`computation` is the node to step, `queue` is the pending sequence, `extent` is
the live lifetime boundary, `context` is the current dispatch, and `state` /
`handler` are the closed-`run` fold, threaded unchanged by scoped delimiters.

One step dispatches on the computation tag:

| Node | Transition |
| --- | --- |
| `Return(v)` | descend the pending queue with `v` (see below) |
| `Perform(req)` | in a `Root` context, apply the `run` clause to `state`, `req` and a raw continuation, and continue with the clause's `(nextState, next)` and an empty queue; in a `Scoped` context, apply the operation clause to `req` and a raw continuation and enter the resulting target under the caller's context |
| `Bind(inner, k)` | push `Cons(k, queue)` and continue with `inner` |
| `Resume(v, saved, e)` | check `e`, replay `saved` into a fresh activation, push the replayed queue in front of the current queue, make that activation current, and continue with `Return(v)` |
| `Handle(ret, op, body)` | install a `Scoped` context for `body` and continue with an empty queue; a leading `Finish` is coalesced so a tail-returned handle shares its completion instead of adding a second one |
| `OwnedResume(owner, v)` | transfer the owner from Pending to Running, then Consumed around one callback application, and continue with the callback's next computation |
| unknown tag | trap |

Descending the pending queue with a `Return` value is the traversal of the saved
continuation:

| Queue node | Action |
| --- | --- |
| `Empty` | the delimiter has finished: a `Root` context returns `(state, value)`; a `Scoped` context runs its return clause and enters the target answer under the caller's context |
| `Cons(k, rest)` | apply `k` to the value and continue with `rest` |
| `Append(left, right)` | inspect `left`: an `Empty` continues with `right`; a `Cons` becomes `Append(rest, right)` and applies its callback; an `Append` is reassociated to run `left` first; a `Finish` moves the finish in front of `Append(rest, right)` |
| `Finish(e, rest, parent)` | close `e`, make `parent` current, and continue with `rest` |

A raw continuation is a closure over the current `queue` and the checkpoint
selected from the current dispatch context: `Root` field 0 or `Scoped` field 5.
The checkpoint can differ from the current execution extent during replay.
Invoking the closure builds `Resume` (tag 3), and the driver replays that sequence with
`cloneSequence`/`rebuildSequence` ([Replaying a captured sequence](#replaying-a-captured-sequence)).
Replay never clones a user callback or a one-shot owner: only lifecycle
activating nodes (`Finish`) are rebuilt, each with a fresh child extent.

Because the loop drives control steps, the native Wasm stack does not grow with
the depth of interpreted binds or with long machine request sequences. Ordinary
recursive user callbacks are not converted into frames by this monadic lowering;
their stack behavior follows ordinary call and tail-call lowering. Using
`return_call`/`return_call_ref` for the tail positions of the emitted steps keeps
the constant-stack property even where the machine calls into ordinary code.

### The executor and the runtime boundary

The eliminator's driver — the flat loop of [The flat driver](#the-flat-driver) — is the
**executor**: the component that interprets `Control` data through typed contexts
and dispatch clauses; closed `run` also threads explicit handler state. Where it lives is a design decision with a strict boundary.

- **The executor is guest code.** It is a compiler-generated loop over the frame
  representation, or a library function over a compiler-provided frame view. It runs
  in the compiled program because it must call the guest handler and guest
  continuations; a host function cannot call back into arbitrary guest closures
  without an explicit boundary.
- **The runtime does not interpret frames.** The Wasm engine manages the GC heap
  containing frames. `psrs-runtime` supplies checked guest runtime leaves and
  linear-memory allocation; host services enter through explicit bindings, and
  the compiler-owned linker assembles the artifact. None of these components
  inspects continuation layouts or owns the control driver.
- **The runtime may provide a generic driver loop.** A runtime that loops calling a
  guest "step" function is admissible, as long as it treats the step as opaque guest
  code and learns nothing about the frame layout. This is optional; a guest-side loop
  is the baseline.
- **Under a native continuation the executor disappears.** With stack switching the
  runtime captures and resumes the stack itself, so there is no software driver to
  place ([migration to native control](delimited-control-and-resumable-frames.md#migration-to-native-control)).

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
| GC roots | frames and live callback locals are Wasm GC roots | the Wasm engine collects them; native-form roots need a separate ABI |
| Leaf operations | ordinary calls to checked bindings | storage, host services, trap |
| Frame layout | defines and verifies it | must not know it |

## Algorithms

### The executor loop

The driver is a single loop over the computation node. Its step procedure is:

```text
step(state, handler, computation, queue, extent, context):
    match computation.tag:
        Return(v):
            return finish(state, handler, v, queue, extent, context)
        Perform(request):
            if context is Root:
                pair = handler(state, request,
                               rawContinuation(queue, dispatchExtent(context)))
                return step(pair.state, handler, pair.next, Empty, extent, context)
            else:                                   # Scoped
                target = context.op(request,
                                    rawContinuation(queue, dispatchExtent(context)))
                return enterTarget(state, handler, target, context)
        Bind(inner, k):
            return step(state, handler, inner, Cons(k, queue), extent, context)
        Resume(v, saved, e):
            check(e)
            (replayed, activation) =
                cloneSequence(saved, Empty, Empty, extent, dispatchExtent(context))
            return step(state, handler, Return(v), Append(replayed, queue),
                        activation, context)
        Handle(ret, op, body):
            (saved, parent, owned) = coalesceLeadingFinish(queue, extent)
            return step(state, handler, body, Empty, owned,
                        Scoped(ret, op, saved, parent, context, owned))
        OwnedResume(owner, v):
            transfer(owner): Pending -> Running
            callback = loadOwnerCallback(owner)       # local GC root
            clearOwnerCallback(owner)                # before invoking guest code
            next = callback(v)
            set(owner, Consumed)
            return step(state, handler, next, queue, extent, context)
        unknown tag:
            trap

finish(state, handler, v, queue, extent, context):
    match queue:
        Empty:
            if context is Root: return (state, v)
            else:               return enterTarget(state, handler,
                                    context.ret(v), context)
        Cons(k, rest):
            return step(state, handler, k(v), rest, extent, context)
        Append(left, right):
            return appendStep(state, handler, v, left, right, extent, context)
        Finish(e, rest, parent):
            close(e)
            return step(state, handler, Return(v), rest, parent, context)

enterTarget(state, handler, target, context):
    finish = Finish(context.ownedExtent, context.callerQueue, context.callerExtent)
    return step(state, handler, target, finish, context.ownedExtent, context.callerContext)

appendStep(state, handler, v, left, right, extent, context):
    match left:
        Empty:               return step(state, handler, Return(v), right, extent, context)
        Cons(k, rest):       return step(state, handler, k(v),
                                    Append(rest, right), extent, context)
        Append(a, b):        return step(state, handler, Return(v),
                                    Append(a, Append(b, right)), extent, context)
        Finish(e, rest, p):  return step(state, handler, Return(v),
                                    Finish(e, Append(rest, right), p), extent, context)
```

`coalesceLeadingFinish` exposes a leading `Append` head first: an `Empty` left
continues with the right; otherwise it rebuilds the head with
`Append(left.rest, right)`. When the head is a `Finish`, it reuses that finish's
extent, remaining queue and parent instead of allocating a new extent, so a
tail-returned `Handle` shares one completion with the existing finish.

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

### Replaying a captured sequence

Replay turns a persistent captured sequence into a fresh activation. It is two
loops, `cloneSequence` and `rebuildSequence`, with the captured sequence as the
shared input:

```text
cloneSequence(queue, todo, reversed, active, parent):
    # `todo` is a LIFO stack of pending Append right-halves.
    while todo is Append(left, right):
        todo = right; queue = left
    if todo is not Empty: trap
    match queue:
        Empty:
            return rebuildSequence(reversed, todo, active, parent)
        Cons(k, rest):
            return cloneSequence(rest, todo, Cons(k, reversed), active, parent)
        Finish(e, rest, _):
            check(e)                       # reject an expired saved extent
            return cloneSequence(rest, todo, Finish(e, reversed, parent), active, parent)
        Append(left, right):
            return cloneSequence(left, Append(right, todo), reversed, active, parent)

rebuildSequence(reversed, result, active, parent):
    match reversed:
        Empty:
            return (result, active)
        Cons(k, rest):
            return rebuildSequence(rest, Cons(k, result), active, parent)
        Finish(e, rest, _):
            fresh = newExtent(parent)      # fresh lifecycle activation
            return rebuildSequence(rest, Finish(fresh, result, active), fresh, fresh)
```

`cloneSequence` flattens and reverses the sequence while checking each saved
extent; `rebuildSequence` walks the reversed sequence from the outside inward,
rebuilding each `Finish` with a fresh child extent. Ordinary callbacks and
one-shot owners are shared, never rebuilt, so replay does not re-run or clone
user mutable data. Only lifecycle activating nodes are instantiated, and the
fresh-parent chain stays finite even when the same sequence is replayed
repeatedly.

## Code map

The machine's owners and their required responsibilities are:

| Owner | Required responsibility |
| --- | --- |
| Checked Core control contracts | Quote each primitive's result, arguments and source/target domains before lowering |
| CC layout planning | Reserve the nominal `Control`/`Resumption` carriers, the machine representations and every generated helper signature before bodies are lowered; keep the table immutable afterwards |
| CC control lowering | Emit the frame node for each primitive, route `run`/`once`/`cancel` to helpers, and synthesize the raw resumption target and the eight helpers from the fixed tables |
| MIR lowering and optimization | Resolve symbolic representations to physical layouts and loopify the executor's self tail calls |
| CC/MIR machine verifiers | Check dispatch, actual exits, transition operands, capture and finish ownership, replay freshness, ordered private-cell updates and publication; prove lifecycle helper bodies and account for observable operations |
| Structured encoding | Emit the loop and its dispatch as ordinary Wasm control flow |

The intended code organization is a machine-layout module (carrier tables and
helper signatures), a lowering module that constructs nodes and synthesizes
helpers, and paired CC/MIR verification modules for each relation. No module
recognizes a concrete effect, operation or handler, and no helper gets a private
representation path.

Stage-independent transition relations and ordered execution events have one
shared owner under `control/`. CC and MIR adapters supply representation,
reference-origin, selected-case and execution-order evidence; they must not
independently define the machine's state transitions. Observable-effect summaries
are shared with ordinary optimization, with separate facts for writes, allocation,
possible traps, calls and termination. A no-write summary alone cannot discharge
an unobservable-operation obligation.

## Invariants and verification

- Every machine field keeps its planned nominal representation at every layer;
  only a declared source payload/storage boundary erases and recovers a source
  value, and no layer reconstructs a source type from a machine node.
- A computation dispatch covers exactly the live tags. An unknown tag reaches a
  trapping default, and a projection is taken only from the case the dispatch
  actually selected.
- A source `Return` applies its current scoped context's return clause and a
  source `Perform` applies its operation clause; CC and MIR reject an applied
  clause taken from another field, another frame or a boundary.
  The scoped branch must use the complete application result as its next
  computation, consuming the current answer/request; an operation supplies the
  raw resumption over its current queue/checkpoint. State and the root handler
  retain their identities. MIR requires the application to dominate that
  transition inside the scoped branch. A deleted or dead application and a
  discarded result do not discharge this contract.
- An executor transition's result reaches the actual normal exit. CC checks
  result forwarding through enclosing structured alternatives; locating a valid
  nested dispatch does not establish this property. MIR checks recursive call
  results on subsequent normal exits and checks scoped transitions at actual
  terminators, rather than stopping at the first call instruction.
  Identity-preserving casts, copies and joins with the same normal origin are
  accepted. MIR joins and execution dominance are resolved within the current
  step's reachable CFG region; the executor loop header starts the next step.
- MIR checks every nominal variant constructor against its planned case, including
  ordinary data variants: the physical tag is an immutable `i32` initialized to
  that case's tag. Constant copies and equal constants at normal joins retain
  this evidence. A common variant base cannot be constructed directly. Structural
  MIR without a planned nominal layout has no source case witness to recheck.
- `Bind` enters its own body with its own callback prepended to the current
  persistent queue. `Resume` returns its saved answer and appends the replayed
  queue to the current queue, installing the activation from that same replay
  result. CC and MIR check these operands on the actual executor transition.
- The pending sequence is persistent: replay never mutates a shared node, and the
  driver is the only code that traverses a saved sequence.
- Root `Perform` applies the current handler to state, request and the raw
  continuation, consumes state/computation from one result pair, and clears the
  captured source queue. Terminal root `Return` publishes the current state and
  answer. `Cons` consumes its callback, answer and tail once; `Append` preserves
  left-to-right order for each selected left case; `Finish` closes its extent
  before restoring its parent and remaining work. Required actions need actual
  validated exits; a trapping arm cannot replace an entire required action.
- Owned execution orders Pending guard, Running write, callback load, slot clear,
  one application, Consumed write and executor transition. Cancellation orders
  Pending guard, Cancelled write and slot clear. Start/Once publish fresh private
  cells with their required current contents; constructor contents are not a
  proof of contents after intervening writes.
- `Check` and `Close` have body contracts as well as callee identities. `Check`
  tests the current extent's live status and every ancestor, rejecting closed
  extents. `Close` sets that extent's status to closed and returns the same extent.
  A call to the correct symbol does not prove these behaviors. Verifiers must
  check retained bodies and preserve the proof across inlining.
- Every executed write, call and explicit trap must be justified by its machine
  step, including operations before dispatch and paths ending in a trap. Required
  frame allocation is distinguished from unrelated mutation. Read-only static
  calls may trap or diverge, so permitting them requires an explicit step role or
  stronger evidence. Normal-exit traces alone do not prove exceptional behavior.
- Replay rebuilds only lifecycle activating nodes; ordinary callbacks and
  one-shot owners are shared, never cloned, and each rebuilt finish gets a fresh
  child extent whose parent chain stays finite.
- The flat driver loops over every step, so execution is stack-safe independently
  of the native stack; emitted tail positions use `return_call*`.
- Closure conversion, adapters and optimizations preserve evaluation behavior
  under ordinary IR contracts; they do not verify a private protocol.

Evidence must cover frame identity, capture boundaries, replay activation
freshness, one-shot status and stack safety separately from library-level
interpretation. The [acceptance checklist](../../../implementation/backend/algebraic-effects.md)
records coverage. CC and MIR check retained Check/Close bodies, ordered owned
transfer, root/queue operands, and current private contents at publication.
Execution events distinguish allocation, mutation, reference refinement, calls
and explicit traps. A failed guard terminates before any subsequent observable
operation; preceding events must match a prefix of a validated normal path.
Entry initialization is part of the proof, before the first dispatch. Copies and
normal joins preserve operand identity without suppressing executed events. Prefix
comparison resolves observed inputs through each path's aliases but retains the
instruction result IDs that identify the operation and its checked definition.

Static calls admitted as unobservable need a total-function summary: no fallible
instructions, writes, dynamic calls, reachable CFG cycles or recursive call
cycles. The summaries use a least fixed point; no-write recursive SCCs are not
assumed terminating. Narrowing casts need static subtyping, selected immutable
case evidence (or the sole case of a nominal variant), or an actual checked
callback/result role. These checks trust ordinary SSA/layout verification and
checked source callback typing; they do not reconstruct source types from payloads.

Ordinary inlining preserves the physical Start/Once frame and the source witness.
MIR replays current private-cell contents from allocation to that witness (Run
checks its root status before executor entry). Mutually exclusive Run alternatives
can allocate both candidate contexts; those ordinary caller allocations are not
part of the private-status proof. Helper-body allocation accounting still requires
all allocations to feed the actual result/transition. A compiler transform that
inlines Check/Close must preserve their lifecycle-body evidence; today's ordinary
inliner retains these branching helpers and verifies every intermediate module.

The [2026-10-14 review](../../../implementation/backend/delimited-control-architecture-review-2026-10-14.md)
records the historical defects and their repairs. Caught-host-trap cleanup,
foreign borrowed-resource capture and complete AE/AC acceptance remain open.

## Worked example

Interpreting `run` over `bind (return 1) k` where `k x = return (x + 1)`:

1. `run` builds an empty pending sequence, a root extent and a `Root` context, and
   calls the executor on `(state, handler, Bind(Return(1), k), Empty, root, Root)`.
2. The executor sees `Bind` and continues with
   `(state, handler, Return(1), Cons(k, Empty), root, Root)`.
3. The executor sees `Return` and descends the queue with `1`: `Cons` applies `k`
   to `1`, producing `Return(2)`, with the queue's tail `Empty`.
4. The executor sees `Return` and descends `Empty` under a `Root` context, so it
   returns `(state, 2)`. `run` closes its root and recovers the source answer `2`.

A `Perform` instead reaches the executor's case 1, hands the request and a raw
continuation over the current sequence to the `Root` clause, and continues with the
clause's next computation; a `Scoped` context routes to its operation clause and
enters the target under the caller's context.

## Boundaries and interfaces

- **Core** supplies the checked `Control`/`Resumption` carriers and the
  per-occurrence contract the machine planning consumes.
- **CC** owns the symbolic machine representations, the generated helper
  identities and the frame-construction rules; its verifier checks the local
  construction and projection shapes.
- **MIR** owns the physical layouts, the loop and the machine relations rechecked
  after lowering and optimization.
- **Wasm** owns only the structured encoding of the loop and its dispatch.
- **Library** owns operation meaning, deep/shallow policy and source/target
  conversion; it sees only the checked surface and the callbacks it supplies.
- **Wasm engine** manages the frame GC heap and guest roots.
- **Guest runtime and host bindings** supply checked leaves, allocation and
  services; they provide no continuation and must not learn the frame layout.
  The compiler-owned linker assembles the guest runtime with the program.

## Open questions and future work

- Audit full machine acceptance beyond the repaired lifecycle and observable-step
  checks, including all required reference-library and cost evidence. Focused
  verification does not establish complete AE/AC acceptance.
- Caught-host-trap cleanup and foreign borrowed-buffer/resource capture, which the
  software machine does not yet prove.
- A native stack-switching accelerator, which would replace the frame chain and
  driver behind the same surface under a separate decision and ABI.

## References

- [Delimited control and resumable frames](delimited-control-and-resumable-frames.md)
- [Control elimination and handler scopes](control-elimination-and-handler-scopes.md)
- [DEC-19 — Compiler-generated resumable frames](../../../decision/DEC-19-compiler-generated-resumable-frames.md)
- [CC IR](cc-ir.md), [MIR](mir.md), [data representation](data-representation.md)
- [Structured Wasm encoding](../wasm/encoding-and-structuring.md)
- [Acceptance checklist](../../../implementation/backend/algebraic-effects.md)
