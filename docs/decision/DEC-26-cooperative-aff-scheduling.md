# DEC-26: Cooperative Aff scheduling over WASI 0.2

**Status:** Accepted, 2026-10-25\
**Feature:** [F-02](../feature/F-02-portable-programs.md)

## Context and constraints

Issue #161 owns the official Aff v7.1.0 API, including fibers, cancellation,
parallel composition and masked bracket cleanup. Its pinned JavaScript foreign
implementation is a stack machine and scheduler, not a set of scalar host
functions. WASI 0.2 supplies monotonic clocks and pollable resources; it does
not supply Node's event loop or JavaScript objects. Synchronous blocking delay
inside one fiber would prevent sibling fibers and cancellation from progressing.

## Decision

Implement computation nodes, continuation/unwind stacks, fiber state,
supervision and parallel trees in ordinary target-library PureScript. Use
existing checked cells and per-binding static storage. Do not add an Aff
intrinsic, compiler-known fiber type, thread, stack-switch proposal, or JS FFI
runtime. Preserve the official pure declarations and public signatures/roles.
Keep Aff/ParAff's official foreign-data declarations. Their fully applied
values already use the shared erased-reference convention for applied opaque
types. Only nullary Supervisor uses a constructor-free ordinary target type:
nullary foreign data denotes a WASI resource-table index, while the supervisor
owns GC cells and closures. No public constructor is added.

A generic library event loop owns a FIFO ready queue, deferred turns and owned
WASI pollables. Ready work drains synchronously as the official Aff scheduler
does; asynchronous callbacks enqueue resumptions. Zero delay is a deferred
turn. Positive delays register real monotonic deadlines through exact clock
words, and the loop polls all pending resources together. JavaScript's separate
setImmediate/timer phases are not a WASI service; their incidental phase order
is not promised. Finite timer normalization follows the Node timer domain;
process-specific warning formatting is outside the target contract.

The ordinary configured Effect command runner drains this loop after the
main action returns. An unresolved fiber without pending events does not
manufacture a result or keep the command alive. Pure Int entry is a project
extension with no ambient JS event loop; embeddings using unsafePerformEffect
must supply an explicit event-loop boundary for deferred work.

A pending operation owns a revocable token. Only its first callback can queue
a result, and cancellation revokes a queued resumption before it can run.
Registration, waiting and completion are distinct states; no invented canceler
stands in for a registration that has not returned. User callbacks are ordinary
calls. Every state write after a callback rereads current state, preserving
reentrant cancellation rather than overwriting it with an earlier snapshot.

Acquisition and finalization mask cancellation. Save the interruption epoch
on unwind frames; a newly observed unmasked cancellation bypasses recovery,
while cleanup runs to completion. Join removal and cancellation waiting are
separate operations: revoking an observer does not undo the target's kill.
Parallel application/race wait for losing cancellation/finalizers before
publishing their result. Supervision tracks children and removes completed ones.

## Consequences and required evidence

Retained memory is proportional to live fibers, pending events, observers and
continuations, not completed history. Continuation processing and queue/tree
walks must be stack-safe. Pollable ownership has exactly one release path on
fire or cancellation. Ordinary source errors use the existing checked native
exception channel; engine traps/exhaustion remain outside source catches.

The acceptance checklist must execute ordinary, synchronous-callback, deferred,
reentrant, cancellation, bracket, supervision and parallel cases against the
pinned JS implementation where comparable. Source audit, compiler acceptance,
runtime correctness and the broad corpus gate remain separate claims. This
decision does not mark #161 or BE-21 complete before those checks run.

## Reference implementation boundary

Pinned Aff v7.1.0 JS attempts to call its temporary Aff node as a canceler when
killed inside async registration; the independently executed reproducer throws
`TypeError: step is not a function`. A typed GC node is not a callable canceler.
The target records the interrupt until the real canceler is installed, then
runs it exactly once. This explicit target behavior is tested separately from
JS agreement; reproducing a JS internal shape error would violate the target's
distinct registration/canceler invariant. The target otherwise retains observed
reference behavior, including body-outcome restoration after finalizer failure
and the measured left-error result when both Alt branches fail.
