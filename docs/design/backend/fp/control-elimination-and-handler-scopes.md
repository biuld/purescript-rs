# Control Elimination and Handler Scopes

**Feature:** [F-02](../../../feature/F-02-portable-programs.md)  
**Status:** Draft  
**Prerequisites:** [Delimited control](delimited-control-and-resumable-frames.md),
[CC IR](cc-ir.md), and [representation and evidence](representation-and-evidence.md).  
**Summary:** Generalize the existing `Prim.Control.handle` to eliminate a source
computation into a distinct target computation. Return and operation clauses run
outside the source handler; ordinary library code chooses deep or shallow
resumption. Dispatch scope and continuation lifetime are separate machine facts.
[DEC-20](../../../decision/DEC-20-typed-control-elimination.md) records this draft
API revision; the implementation checklist separates historical v1 evidence from
focused v2 execution and the remaining verifier/foreign obligations.

## Scope

This topic owns the precise `handle` type, raw continuation behavior, clause
scope, and the frame/lifetime obligations needed to implement it. The parent
control topic owns the other primitives, one-shot ownership, foreign boundaries,
and target profile. Concrete operations, state, union projection, and residual
row policy remain library responsibilities.

## Background

The previous five-argument `handle` gave its operation clause a source result
`Control u a`, then interpreted that result under the same handler to produce
`Control g (s, a)`. This is a fold. It cannot move clause execution outside that
handler without an actual conversion between both request and answer domains.
Changing dispatch alone would confuse `u` with `g` and `a` with `(s, a)`.

Scoped handlers instead distinguish the source remainder from the target answer.
A clause may perform target effects before resuming, abandon the source, or
resume it several times. Shallow resumption leaves subsequent source effects
available to an outer handler, so it cannot in general erase the handled effect
from its result type. These are ordinary type relationships, not effect inference.

The reference `purescript-run` implementation exposes one-layer elimination and
folds into a target monad. Its target domain is distinct from the source carrier.
This motivates the separation here, but does not establish compatibility with a
pinned upstream package. OCaml's deep/shallow distinction provides a scope
reference; its native continuation ownership and lifetime policy are not adopted.

## Model

The existing intrinsic identity `ControlHandle` has this revised checked scheme:

```purescript
handle :: forall u g a b.
          (a -> Control g b)
       -> (forall x. u x -> (x -> Control u a) -> Control g b)
       -> Control u a
       -> Control g b
```

The first argument is the return clause; the second is the operation clause.
The raw continuation receives exactly the request's answer `x` and constructs
an inert computation over the original `u`, returning the original `a`.
It does not reinstall the source handler or apply its return clause. Both
clauses construct target computations over `g`, returning `b`. Constructing
`handle`, `bind`, `perform`, or a raw resumed computation invokes no callback.
Strict evaluation of arguments still follows the ordinary language rules.

`run` retains its existing state-threading closed fold type and behavior. It is
a terminal execution convenience, not the closed case of this scoped operator.
A library can finish an empty-request target with a checked empty-functor runner.
The base continuation remains multi-shot; `once`/`resume`/`cancel` retain their
separate ownership contract.

## Design

### Library derivations

Below, `project` is a library Church-encoded split, and `pure` wraps the compiler
return primitive. `Pair s a` abbreviates `{ _1 :: s, _2 :: a }`.

```text
project : forall x r. u x -> (f x -> r) -> (g x -> r) -> r

deep : (forall x r. u x -> (f x -> r) -> (g x -> r) -> r)
    -> (forall x. s -> f x -> (s -> x -> Control g (Pair s a))
                            -> Control g (Pair s a))
    -> s -> Control u a -> Control g (Pair s a)

deep project clause s c = handle
  (\a -> pure (s, a))
  (\request raw -> project request
    (\op -> clause s op (\s' x -> deep project clause s' (raw x)))
    (\op -> bind (perform op) (\x -> deep project clause s (raw x))))
  c

shallow : (forall x r. u x -> (f x -> r) -> (g x -> r) -> r)
       -> (forall x. s -> f x -> (s -> x -> Control u (Pair s a))
                               -> Control u (Pair s a))
       -> s -> Control u a -> Control u (Pair s a)

shallow project clause s c = handle
  (\a -> pure (s, a))
  (\request raw -> project request
    (\op -> clause s op
      (\s' x -> bind (raw x) (\a -> pure (s', a))))
    (\_ -> bind (perform request)
      (\x -> shallow project clause s (raw x))))
  c
```

Deep continuation reinstalls the library handler around the raw source remainder.
Its clause's fresh requests belong to `g` and execute outside that installation.
State updates are arguments to the continuation; independent replay branches
start from the state each invocation supplies. This does not copy mutable user
objects or roll back IO. Deep forwarding re-emits the residual `g` payload.

Shallow continuation resumes the raw source directly and maps its final answer.
Its result retains `u`; claiming a general `Control g` result would be unsound.
Before the selected operation is reached, forwarding re-emits the original `u`
request and reinstalls the shallow search after its answer. No inverse injection
law or compiler union selector is needed. More elaborate shallow APIs may widen
or transform the remaining effect collection explicitly in library code.

### Machine phases and scope

The guest loop uses explicit context frames, not a recursive interpreter call
per handler or request:

```text
Computation = Return | Perform | Bind | Resume | Handle(ret, op, body) | OwnedResume
Sequence    = Empty | Cons | Append
Context     = Source(contract, ret, op, sourceQueue, extent, caller)
            | Target(contract, targetQueue, finishExtent, caller)
```

A contract records the checked source request/answer domain and target
request/answer domain. A source context captures only its source queue. Caller
work is outside that capture. When a clause returns target data, its source
dispatch is absent; the enclosing target dispatch handles fresh requests.
An explicit finish marker retains the source extent until that target computation
finishes with `b`, then supplies `b` to the saved caller queue. Returning `a`
from the source is not equivalent to completing the target with `b`.

### Lifetime and replay

Dispatch removal does not close a delimiter's lifetime extent. A raw continuation
captured by a clause stays valid while its target answer is being computed.
Once that answer completes, calling it or interpreting a previously constructed
resume node must fail, even if an enclosing runner is still live. An ancestor's
completion also expires suspended children abandoned by an outer clause.

Captured queues are persistent. A replay creates fresh activation state for
captured lifecycle frames; completing one replay must not close the original
suspended checkpoint needed by another. Captures created inside a replay expire
with that replay's activation. When capture includes inner finish markers, those
markers are activation templates, not shared mutable close tokens. Only compiler
lifecycle frames are instantiated again: user closures, mutable values, and
one-shot owners remain shared. Frame identity and checkpoint provenance must
survive erasure; request names cannot reconstruct them.

A deep library handler can tail-return another `Handle` on every request.
Constant native stack alone does not prevent an ever-growing chain of inactive
lifetime ancestors. The machine may coalesce finalization groups only when the
remaining target queue contains compiler finish markers, no user continuation,
and both checked target contracts and completion boundaries agree. It must
retain separate extents when a clause has work after a resumed branch or a
branch completes independently. Shared tail finish groups close at their common
answer boundary; replay-local captures still expire at branch completion.
Long request sequences require allocation and ancestor-depth evidence as well
as stack evidence. Any optimization must preserve the observable expiry rules.
The cost tests inspect actual parent chains at extent allocation using ordinary
checked IR probes and record post-collection live heap and collection counts at
fixed initial heap size. This distinguishes retained ancestry from allocation
churn; collection counts are not a measurement of exact total allocated bytes.

### Machine storage contracts

The representation table owns the recursive field schema. Computation links,
queue tails, finish extents, caller contexts, owner callback cells and status
cells have explicit `ReprId` shapes; stored bind/handler callbacks have the
predeclared unary callable shape. Only user requests, answers and the mutable
callback-cell contents use erased storage. A spent owner clears its callback
slot with another erased reference while its integer status cell stays shared.

The source carrier and user callback protocol use explicit reference casts to
recover these existing machine objects before construction. This is not source
payload recovery: adapting array contents here would copy a status cell and invalidate
ownership/lifetime sharing. Projections produce their declared stored shape;
erasure for a user callback is a separate conversion. CC verifies construction
and projection shapes, and the target planner retains them as MIR struct field
types, including recursive queue/context/extent references.

Machine helper signatures preserve the callable, computation, queue, extent,
context and owner shapes. A raw resumption captures a typed queue and typed
checkpoint extent; closure capture verification checks those same shapes.
MIR checks each locally constructed closure against the surviving capture
projections of its code target before nullable `eqref` storage erases slot types.
Receiver copies and reference casts preserve this relation. The contract is
sparse after dead projection elimination: unused stored slots may remain, but
every surviving slot must exist and supply a compatible type. Repeated reads
of one receiver slot must agree. Imported code and projections from unrelated
closure values remain trusted boundaries; this check does not infer their
capture contracts from a physical closure type.
Replay returns a dedicated product of `(queue, activeExtent)`, distinct from
the erased user `(state, answer)` protocol. Ordinary helpers and self tail-call
loop parameters retain those types; crossing the user callback protocol erases
only the callback arguments/results that require it.

These storage contracts reject a queue supplied as a finish extent or helper
argument, and an erased projection declared without an explicit conversion.
Source semantic contracts have a separate owner. Core exposes a sealed checked
contract for each of the eight operations; both its verifier and P8 consume the
same carrier, callable and quantified-answer checks. Planning quotes the complete
result and operand types before freezing the representation table. The quotation
retains stable constructors, free variables, lexical `forall` binders and open
rows without retaining Core arena indices. Canonical source type identities use
Core's rigid equality, including alpha-equivalence and its existing row relation.
CC does not implement another effect type checker.

At each source erasure boundary, `ControlEvidence` connects the immutable
contract to the represented result, original erased operands and actual frame
construction or helper call. CC checks the predeclared frame case/representation
or helper signature and ordered operands through reference erasure/recovery
aliases. Frame results must be the proven construction. When an operand comes
from another control witness in the same CC assignment sequence, its canonical source type must match the
consumer's checked operand type. Thus identical runtime shapes do not permit
exchanging a known source computation with a target computation.

P9 preserves the witness as an SSA instruction. MIR checks its contract and
arity, verifies ordinary SSA use/definition and result type rules, and compares
known producer domains through copies and reference casts. Optimizations remap
all evidence operands with their ordinary value substitutions. Live witnesses
survive P10; dead, pure witnesses may disappear with unused inert results. Wasm
encoding finally erases the proof and emits only the represented value copy.
There is no additional source intrinsic, runtime frame or host callback protocol.

This is a partial semantic provenance contract. Parameter values, imported and
ordinary higher-order call results, aggregate projections and control-flow joins
without a local witness remain trusted Core/P8 producer boundaries. MIR trusts
P9's translation of the checked factory relationship; it does not reconstruct
that relationship from physical struct instructions. A matching helper signature
does not prove a helper's implementation identity. Exact checkpoint ownership,
replay activation and matched finish provenance must still be carried and
verified; neither a well-shaped cast nor a quoted source type proves them.

### Alternatives

- Moving old clause execution to the caller changes neither its source type nor
  its answer type, and is unsound without a real typed conversion.
- A deep/shallow Boolean cannot supply the missing output domain or return clause.
- A new `step` intrinsic duplicates the elimination supplied by generalized
  `handle`; a one-layer observation alone also does not define lifetime extent.
- A synchronous arbitrary answer `r` requires a different execution contract.
  Here the answer is inert `Control g b`, which the shared loop interprets later.
  This representation supports target-answer separation without nested execution.
- Separate compiler deep/shallow operations are unnecessary: the typed library
  derivations express their difference. No concrete effect policy enters lowering.

## Algorithms

1. Interpreting `Handle` saves caller work, opens an extent, and enters its source
   with an empty source queue.
2. Normalize source `Bind`/`Return`/`Resume` using persistent sequence operations.
   A resume checks checkpoint liveness and runs its saved queue before consumer
   work; it does not automatically restore source dispatch.
3. At source `Return a` with an empty queue, remove source dispatch and invoke
   the return clause once. Enter the returned target data with a finish marker.
4. At source `Perform request`, package its source queue as raw continuation,
   remove source dispatch, and invoke the operation clause once. Clear the old
   queue: only explicit continuation use may run it. Enter target data with a
   finish marker, leaving the captured checkpoint live.
5. Process target requests in the enclosing dispatch. A library deep continuation
   constructs a new `Handle`; a shallow continuation constructs a raw resume.
6. At target completion, close that activation's extent and restore caller work.
   Capture/replay of context frames instantiates lifecycle templates as described
   above. Normalization and context transitions use one iterative guest machine.

## Code map

| Owner | Required responsibility |
| --- | --- |
| Intrinsic registry and checked Core | One revised `ControlHandle` scheme and checked occurrence contract; quantified request/answer relationships |
| CC layout | Predeclare computation/context layouts, callable signatures and contract identities; representation table remains immutable in CC lowering |
| CC lowering | Synthesize guest loop and callbacks using those SignatureIds; preserve source/target domains, capture boundaries and lifecycle provenance |
| CC/MIR verification | Validate contracts, queue/context composition, capture ownership and matched entry/finish operations |
| Independent stdlib | State, union projection, residual forwarding, deep/shallow wrappers and source/runtime comparisons |
| Target encoding | Encode verified MIR using the existing capability profile |

## Invariants and verification

Source checking owns quantified type relationships. Lowered verification checks
that generated frames and erased calls use the retained contract identities and
predeclared signatures; it trusts checked source typing, not arbitrary erased
payload casts. Missing metadata, cross-domain queue composition, mismatched
finish frames and forged checkpoint ownership are errors. No stage adds effect
inference, reinterprets operation spellings or mutates representation layout late.

Required behavior includes clause and return-clause effects outside source
dispatch; deep reinstatement; shallow escape with a truthful residual type;
forwarding, abandonment and independent replay; caller work outside capture;
expiry after target completion; liveness throughout clauses; replay of nested
finish frames; one-shot reuse rejection; GC roots; long request sequences; and
foreign-resource/host-trap cleanup. Type proofs and a rewrite model do not verify
Wasm lifetime, GC, stack safety or foreign cleanup. The
[implementation checklist](../../../implementation/backend/algebraic-effects.md)
keeps these claims separate.

## Worked example

An inner handler receives `F`, performs a fresh target request `G`, then invokes
its deep continuation with the supplied answer. `G` reaches the outer handler;
`F` in the resumed source reaches a fresh inner installation. For shallow use,
the resumed `F` instead reaches the outer handler because the raw remainder
still has source domain `u`. If the clause abandons the continuation, no saved
source work executes. The return clause also runs in the target domain.

## Boundaries and interfaces

This is a breaking revision of the draft control interface, designated contract
v2 here. It retains intrinsic identity 96 and the existing eight-operation set,
changing `handle` from five arguments to three. This designation is not a claim
that a compatibility marker already exists in artifacts or packages. Registry,
Core verifier, CC layouts, call expansion, executor, wrappers and tests must
migrate together. Old source signatures must fail checking; cached artifacts
must be invalidated through the artifact contract, and an explicitly revised
package/compiler contract must reject incompatible checked dependencies.
No hidden legacy overload or ninth intrinsic is selected.

Historical compiler/runtime evidence belongs to contract v1. Focused v2 evidence
is recorded separately in the implementation checklist. The compiler lock moves
only after the revised independent package passes source, checked interface and
runtime acceptance. The selected design does not imply that
`purescript-run`, full VariantF rows, or scoped Wasm handlers already work.

## Open questions and future work

The type and scope correction is selected. Concrete context layouts, replay
activation instantiation, and safe tail finalization coalescing require compiler
implementation and the stated acceptance evidence. Foreign-frame suspension and
host recovery remain obligations of the parent topic. Direct-style syntax and
native stack switching remain separately governed future work.

## References

- [DEC-20](../../../decision/DEC-20-typed-control-elimination.md).
- [Run source](https://raw.githubusercontent.com/natefaubion/purescript-run/master/src/Run.purs),
  reviewed 2026-10-11 as an unpinned design reference, not package acceptance.
- [OCaml 5.3 effect handlers](https://ocaml.org/manual/5.3/effects.html), scope reference
  only; this design retains its own software multi-shot and delimiter lifetime policy.
