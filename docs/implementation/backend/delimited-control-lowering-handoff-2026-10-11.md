# Handoff: Delimited-Control Lowering

**Date:** 2026-10-11
**Branch:** `feat/effect-handlers`
**Starting commit:** `2026238f`; the original checkout was clean.
**Executor commit:** `1bcad5a7`; the ownership/lifetime continuation starts from this clean commit.
**Scope:** Typed `Prim.Control` surface, a shared context executor, persistent bind
sequencing, clause scope, library deep/shallow policies, and ownership/lifetime checks. The ownership/lifetime changes are delivered with this handoff in one topical commit.
**Status:** Partial topic. All eight current primitives are lowered; complete AE/AC acceptance remains outstanding.

## Latest state

This topic commit implements DEC-20 control contract v2. The compiler lock pins
stdlib `5d523805c4e2096e68fab5a4f965f62b225ee2af` with content identity
`fnv1a64-v1:a1e5a2f8f4157041`. All 80 control tests pass under default locked package
selection with mandatory Wasmtime; the official type comparison and nine public
package fixtures are recorded in the v2 checklist. Historical sections below
preserve prior checkpoints; the five-argument API belongs to those checkpoints.
The continuation from compiler `bdaf0cfc` makes machine frame storage fields
nominal and adds malformed CC/MIR construction/projection checks; detailed
validation is recorded in the acceptance checklist's machine storage section.
Full frame/context identity verification, allocation/depth measurements, foreign
resources and caught-trap cleanup remain outstanding.

## Governing contracts

- [DEC-19](../../decision/DEC-19-compiler-generated-resumable-frames.md) and
  [DEC-20](../../decision/DEC-20-typed-control-elimination.md).
- [Delimited Control and Resumable Frames](../../design/backend/fp/delimited-control-and-resumable-frames.md).
- [Library-Owned Algebraic Effects and Handlers](../../design/backend/fp/algebraic-effects-and-handlers.md).
- [Acceptance checklist](algebraic-effects.md).

The type system gains no effect tracking or effect reasoning. The compiler owns
only the carrier and generic mechanism below Typed Core; concrete operations,
rows, and handlers remain library code. The executor is guest code, and
`psrs-runtime` does not inspect control frames.

## Committed starting state

- `1633dd88`: the `Control f a` carrier and the checked schemes for
  `return`, `bind`, `perform`, `run`, and `handle`.
- `64a27de8`: inert `Return`, `Perform`, and `Bind` frames; only `run` of a
  `Return` frame executed, while every other frame trapped.
- `d8fc2508`: a shared direct-call signature table for lowering-generated
  functions. It did not provide emitter `SignatureId` handles.
- `2026238f`: the original lowering handoff.

The original five `control_surface` tests passed at this checkpoint, including
Wasmtime exit 42 for `run (return 42)`. A new request runtime reproducer compiled
but trapped with `unreachable` (exit 134).

## Current lowering

The representation table stays immutable during CC lowering. Layout predeclares
all machine representations and the executor signature; the continuation uses
the existing unary erased callable-slot protocol. Verifier direct-call signatures
are registered from those same declarations.

```text
Computation = Return(value) | Perform(request) | Bind(inner, continuation)
            | Resume(value, savedSequence, boundary)
            | Handle(state, handler, project, inject, inner)
            | Reenter(state, handler, project, inject, inner, boundary)
            | OwnedResume(owner, value)
Sequence    = Empty | Cons(continuation, rest) | Append(left, right)
executor(state, handler, computation, pendingSequence, boundary) -> erased (state, value)
```

`Bind` is inert. The executor pushes its continuation and processes its inner
computation. `Return` consumes one pending continuation. At `Perform`, the
handler receives a resumption capturing the persistent pending sequence; calling
it builds `Resume` data. It does not recursively execute the saved work. `Resume`
joins saved and caller sequences in that order, with loop-driven normalization.

After a handler returns its next state/computation, the executor clears its old
pending sequence. Only the returned computation chooses whether to resume,
abandon, or replay saved work. All frame/sequence nodes remain immutable, so
multiple resumptions share the saved chain safely. A bare `Perform` needs only
the request; its identity continuation is the empty sequence. This supersedes the
original handoff's proposed recursive composition closures and mandatory
continuation field on every `Perform` node.

Concrete state, results, handlers, and bind closures cross the existing payload
conversion boundary. The result pair uses the canonical erased `_1`/`_2` record
protocol. Closed/open executors and generated resumption/projection functions are shared across all
`run` sites in the linked module. Self tail calls become MIR loop back edges.
Conservative ordinary call effects preserve ignored-result interpretation; no
control-specific optimizer classification was introduced.

### Code owners

- `crates/psrs-backend/src/cc/layout/control.rs`: predeclared machine layouts and
  signatures.
- `crates/psrs-backend/src/cc/lower/control/`: source primitive lowering, synthesis,
  CC construction helpers, shared driver reductions, and open forwarding.
- CC lowering context and lambda children: shared generated functions/signatures.
  Nested functions resolve generated executor calls during verification.
- `crates/psrs-backend/src/mir/lower/variant.rs`: the general CC-to-MIR variant
  construction contract. A concrete common variant result is constructed at its
  case type before widening; the direct regression covers nullary and
  field-bearing cases at abstract/common result shapes.
- `crates/psrs-driver/tests/control_surface.rs` and `control_runtime.rs`: source
  rejection and mandatory value-sensitive execution evidence.

## Open-handler continuation

`handle` is an inert node. The open executor shares closed-driver reductions,
but returns residual computation data. Its Church-sum projection selects handled
requests or unchanged residual payloads. A residual bind closure captures the
inner state, handler and pending sequence and constructs `Reenter` around
`Resume`; it does not recursively interpret the sequence. Outer replay preserves
independent copies of saved inner state, and outer abandonment discards the inner
remainder. The caller's pending bind stays outside the inner delimiter.

The current machine retains the original union computation, so `inject` is
strictly evaluated and stored but is not called to reconstruct a request. The
normative design records this contract. Native stack use may depend on handler
nesting depth, not on the number of requests at fixed depth.

`control_open_runtime.rs` adds executed open-handler, forwarding, nested-handler,
aggregate, replay, trap and GC evidence. The GC case requires an observed completed
collection in the runtime log before claiming success. Source tests reject an
incompatible residual functor/injection and a monomorphic projection.

## Ownership and lifetime continuation

The compiler interface adds nominal `Resumption f a b` and `once`/`resume`/`cancel`.
`resume` constructs an inert `OwnedResume`; interpretation checks shared ownership,
sets Running before calling the wrapped function, then marks Consumed on its
normal return. Immediate cancellation changes only Pending to Cancelled. Spent
owners clear their callback slot, allowing captured values to be collected.
Separate `once` calls allocate independent owners; ordinary captured functions
remain multi-shot.

`run` creates and closes a delimiter token. Calling a resumption and interpreting
its already-built `Resume` node both check token liveness. Open delimiters refer to
their enclosing token; suspended checkpoints expire when that parent expires.
Residual replay creates a fresh sibling activation, preserving multi-shot replay
without growing the token ancestor chain once per forwarded request.

The baseline at `1bcad5a7` executed an escaped continuation after its `run` returned
with result 42. It now traps. New source/runtime tests cover owned API rejection,
double interpretation, cancellation, re-entrancy, closed/open expiry, outer
abandonment and GC with a pending owner. Checked frame identity, foreign resource
capture and host recovery after a guest trap still require complete acceptance.

## Validation

The locked standard-library revision `1e01310` is materialized at:

```text
/private/var/folders/f2/h3z3zsy51xq3kr06707r9ct40000gn/T/opencode/psrs-stdlib-1e01310
```

Set `PSRS_STDLIB_ROOT` to that directory. Do not modify the lock or substitute the
working standard-library checkout. Run runtime evidence with
`PSRS_REQUIRE_WASMTIME=1`.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --test control_surface --test control_runtime --test control_open_runtime
```

The committed executor slice passed formatting, full clippy, 423 backend library
tests, and 38 control tests with mandatory Wasmtime. Its full workspace run and
archived `2026238f` baseline had the same 9 failing targets and 176 failing test
names. The ownership/lifetime continuation adds focused evidence recorded below.
The acceptance checklist records
the decomposition, logs, tested cases, and remaining obligations.
Compile acceptance, executed runtime checks, and official source comparison are
separate claims.

## Remaining work

- Integrate the independently committed library wrappers into a selected package
  revision; the compiler lock remains unchanged for this continuation.
- Complete generic deep/shallow wrappers, foreign capture-boundary, foreign-resource
  GC, host recovery after a guest trap, and official differential evidence.
  One-shot and normal-return lifetime checks have focused executed evidence.
  Ordinary GC capture recovery during suspension has executed evidence.
- Formalize selective conversion beyond the monadic surface. This closed executor
  does not establish direct-style capture or arbitrary user recursion safety.
- Extend verification evidence for complete frame identity and lifetime claims.
  Current generic verifiers check ordinary shapes, calls, tags, and control flow;
  they trust checked answer types and the generated machine's sequencing rules.
- Repair the separate mismatch between the locked package's old Effect bindings
  and the compiler's removed Effect/State mechanism. A locked-package failure is
  not evidence about the self-contained control programs.

No complete AE/AC requirement is marked Verified by these slices. All current
`Prim.Control` primitives lower and execute, including one-shot consumption and
normal-return delimiter expiry. The complete foreign, host recovery and frame
verification guarantees remain unverified.

## Latest continuation validation and package handoff

All 66 control tests and 424 backend library tests pass with mandatory Wasmtime.
Formatting and full workspace clippy pass. The full workspace `--no-fail-fast`
run remains failing at the same 9 baseline targets: 175 names versus 176 at
`2026238f`, with no new failures. The recovered case is the newtype constructor /
pattern runtime test. Retained final logs and decomposition are in the acceptance
checklist's checked primitive values section.

The continuation also repairs mandatory primitive-value elaboration for compiler
interface globals, so valid aliases and partial applications use their checked
Core types before CC. Two direct runtime regressions cover scalar/control aliases
and partial applications. It does not add a feature-specific global exception.

Independent library wrappers are committed at `dda3ba3` in
`/private/tmp/psrs-stdlib-control-20261011`, based on locked stdlib `1e01310`.
Public `PSRS.Control` wrappers execute one-shot, multi-shot, open forwarding,
double-use rejection and cancellation rejection through the package-owned Node
runner. The original dirty stdlib checkout is preserved, and the compiler package
lock is unchanged. Integrating this package revision with the unrelated existing
stdlib changes is still separate work; these wrappers are not yet in the locked
compiler package.

The next required slices are deep/shallow handler behavior with official library
comparison, complete frame identity verification, foreign borrowed resources and
host recovery/cleanup after a caught guest trap. Normal-return lifetime and
single-thread callback ownership evidence do not discharge those obligations.

## Main stdlib continuation (2026-10-11)

The main stdlib worktree is now clean on `control-library` at `28997b5`, containing
`dda3ba3` plus official type comparison and semantic trap acceptance. Withdrawn
State-protocol changes were archived in a stash; the main checkout no longer
contains that implementation. The compiler lock remains on `1e01310`.
The clean temporary stdlib control worktree was removed after its commits were
retained on the main package branch.

The official type comparison discovered and fixed the wrapper's invalid imported
type exports by using local type aliases. Both compilers agree on five accepting
and four `TypesDoNotUnify` rejection cases; the official side uses an explicitly
opaque interface shim and makes no control runtime claim. The five public
Wasmtime fixtures and 12 Node tool tests pass. Full Rust checks were not repeated
because this continuation changes only library/tool sources and compiler docs.

The next concrete blocker is retained in stdlib `ClauseScope.purs`: compilation
succeeds, execution returns 0 against required 42. The current fold interprets a
clause's fresh request under the same inner handler, while complete scoped-handler
semantics require the outer handler to see it. Deep resumed execution and clause
execution need distinct generic delimiter evidence; shallow removal also remains
unexpressed. The checklist records the owning code, report and required next
boundary. Do not treat existing residual forwarding as proof of clause scope.


## Selected contract correction and next implementation

The [scope contract](../../design/backend/fp/control-elimination-and-handler-scopes.md)
and [DEC-20](../../decision/DEC-20-typed-control-elimination.md) resolve the next
foundation. Generalize the existing `handle` rather than adding `step`:

```text
handle : (a -> Control g b)
      -> (forall x. u x -> (x -> Control u a) -> Control g b)
      -> Control u a -> Control g b
```

The old fold's source-domain clause cannot execute in its target domain merely
by changing dispatch. Both new clauses return target computations outside source
dispatch, while a raw continuation retains its original source type. Deep/shallow,
state and union projection are library compositions; shallow output preserves
remaining source effects. `run` stays a closed state fold.

Dispatch scope and lifetime must migrate together. Explicit heap source/target
contexts save caller work outside capture and retain extent-finish markers during
clause execution. Target completion closes the extent. Replay instantiates
compiler lifecycle activation frames, preserving original checkpoints and sharing
user values/one-shot owners. Safe tail finalization groups prevent a deep wrapper
from accumulating inactive ancestors per request; non-tail branches retain their
independent expiry boundaries.

Implement the registry/Core scheme and checked contracts, predeclare all CC
layout/signatures, implement and verify the shared context machine, then migrate
the independent package wrappers and acceptance cases. Retain ID 96 and eight
operations; invalidate incompatible artifact/package contracts. Do not update
`stdlib.lock.json` before package source/type/runtime acceptance. The checklist
records the requirement-to-evidence matrix and actual design proofs: official
`purs` accepts both wrappers and rejects four invalid relationships; an eight-case
rewrite model passes. Neither proof establishes guest lifetime or stack safety.

This handoff now distinguishes implemented v1 (`95982bea`, stdlib `28997b5`) from
selected v2 design. No production source changed in the design correction.
The older clause failure remains a v1 reproducer; migrate it to the revised typed
wrapper before using it as v2 target acceptance. AE-06/AC-05, full frame identity,
foreign resources and caught-trap cleanup remain Unverified.


## Implemented v2 continuation

The selected contract is now implemented: existing ID 96 has three arguments,
Core checks control carrier/callable relationships, and CC generates one flat
source/target context executor. Clause and return-clause requests execute outside
source dispatch, with finish markers keeping the target lifetime live. Persistent
replay creates fresh lifecycle activations; tail handler entry normalizes queue
heads and shares compatible finish boundaries. No new intrinsic was added.

The independent package provides the new `handle`/`handleOnce` and typed ordinary
`handleDeep`/`handleShallow` wrappers with protocol 2. Public ClauseScope now
returns 42. Nine public runtime fixtures and 17 official type comparisons pass;
these remain separate from pinned Run/VariantF acceptance. Existing compiler
fold cases are retained as ordinary `foldOpen` library regressions; new scope
cases test the actual revised contract.

The checklist's v2 section records 80 control tests (78 in the full workspace run
plus two subsequent nested replay tests), 424 passing backend tests, formatting
and clippy, and the unchanged nine-target / 175-name workspace failure baseline.
Full CC/MIR semantic frame identity, allocation/depth measurement, foreign borrowed
resources and caught-trap cleanup remain Unverified. Continue with those owner
contracts rather than adding concrete effects or a dispatch mode to the compiler.
