# Library-Owned Algebraic Effects and Handlers

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Draft design. The execution mechanism is selected by
[DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md) and
specified in [delimited control and resumable frames](delimited-control-and-resumable-frames.md);
the lowering is not implemented.  
**Prerequisites:** [DEC-03](../../../decision/DEC-03-purescript-faithful-type-system.md),
[functional Core](../../frontend/semantics/functional-core.md),
[rows and records](../../frontend/type-system/rows-and-records.md),
[representation and evidence](representation-and-evidence.md), and
[delimited control and resumable frames](delimited-control-and-resumable-frames.md).  
**Summary:** Algebraic operations and their handlers are library values, checked
with ordinary PureScript types and rows. The compiler owns only the effect-agnostic
control mechanism the handlers run on, specified separately. This document owns the
library model — operations, handler composition, row-based requirements, residual
rows, and how the library sits on that mechanism.

## Scope

This document owns the meaning of library-defined algebraic operations, handler
composition, row-based operation requirements, residual rows, and the library API
that builds on the compiler's control surface.

It does not own the control mechanism itself — the `Control f a` carrier, the
`return`/`bind`/`perform`/`run` primitives, and their lowering — which is
[delimited control and resumable frames](delimited-control-and-resumable-frames.md).
It also does not own the official Effect/ST representation
([native topic](native-effect-and-st-handlers.md)), a native effect type system, new
source syntax, resource-ownership types, async scheduling, or target stack
switching.

The proposal follows DEC-03's library encoding. It does not reauthorize the
withdrawn compiler-owned `Effect` or `State` mechanisms.

**The type system is unchanged.** This design adds no effect rows, no effect
tracking, and no effect reasoning — no effect inference, propagation, subsumption,
or solving — to CST, AST, HIR, THIR, or Typed Core. An "operation requirement" is an
ordinary library type: a computation value parameterized by its request functor (or
by a row of request functors), checked with the ordinary kind, row, and class rules.
The compiler never reasons about which effects a function may perform.

## Background

### What algebraic effects are

Algebraic effects and handlers separate the *description* of an effectful operation
from its *interpretation*. A computation performs an operation; a **handler** around
that computation decides what the operation means.

- An **operation** (also called a request) is a value describing what the program
  wants to do: "read the current state", "print this line", "get the clock time". It
  names a *result type* but not the way the result is produced.
- An **operation signature** is a set of operations, for example `Get : Unit -> S`
  and `Put : S -> Unit`.
- A **computation** is a value that may perform operations before it returns its
  result; its type records the result and the operations it may expose.
- A **handler** interprets the operations of one signature. It receives each
  operation together with a **continuation** — the rest of the computation from just
  after the performed operation — and chooses what to do with it.
- A **continuation** (also called a **resumption**) represents "the remainder". A
  handler may call it zero times (abort), exactly once (resume), or several times
  (replay).
- A **delimiter** is the boundary within which a handler is installed. Capturing a
  continuation stops at that boundary.
- A **deep handler** installs itself again for every request made after a
  resumption; a **shallow handler** interprets only the current request and runs the
  remainder without reinstalling itself.
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
knowing their meaning. The same console operation can write to a terminal or append
to a test log; the same state operations can thread a pure value or use mutable
storage. The operation says *what*, the handler says *how*.

The name "algebraic" comes from the interpretation: operations form a signature, and
handlers are algebras over it, obeying the laws the signature declares. The
implementation does not need to understand the theory; it only needs ordinary
values, functions, and a way to capture and resume a continuation.

Why not ordinary exceptions? A handler can *resume*. `catch` consumes control
permanently, but an effect handler can supply the operation's result and continue
the computation, possibly more than once. That single difference is what requires
continuations.

PureScript already provides the types to *describe* all of this: algebraic data
types, functions, higher-kinded types, type classes, rank-N polymorphism, ordinary
rows, and monadic `do`. `purescript-run` encodes extensible operations using `Free`
and `VariantF`; its handlers are ordinary interpreters. That establishes
expressibility in the language, not an execution strategy.

An operation row is a library parameter, not a second effect system attached to
every function arrow. Its constraints describe which requests a computation can
expose. They do not prove single use, termination, absence of unsafe FFI, or correct
resource release.

### Encoding operations without native GADTs or existentials

The language has no native GADT or existential syntax. `purs 0.15.16` rejects
`data Box = forall a. Box a` at parse time, while a rank-N field such as
`data Box = Box (forall a. a -> a)` is accepted. GADTs therefore have no direct
form, and an indexed operation cannot be written with a native existential.

Existentials are nonetheless a standard library encoding. The vendored `Data.Exists`
provides `Exists f` (isomorphic to `exists a. f a`) through `mkExists`/`runExists`
over an internal `unsafeCoerce`; its rank-N eliminator compiles and runs on this
target (verified 2026-10-11), and the hidden type variable must not escape the
eliminator's result. A library may therefore encode a typed request with `VariantF`
and rows, with `Data.Exists`, or with a rank-N eliminator, and the choice is the
library's.

The compiler-facing control surface is independent of that choice: its request
functor is opaque, so how a request carries its own payload/result relation never
reaches the compiler.

### What the compiler must not know

The compiler never learns what a particular operation means. It does not recognize
the spelling `State`, `Console`, `Run`, or `Effect`, does not inspect a handler body
to infer a hidden protocol, and keeps no operation graph, frozen source inventory,
or cross-pass witness. It owns only the abstract control surface and its
representation. Everything specific — the operation signatures, the handler
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
actual API must preserve the upstream row constraints, including label absence where
needed. State here is an ordinary value `s`, not a primitive token.

A handler handles its selected slot and forwards every other request with a
continuation that re-enters the handler (deep semantics). Distinct instances use
distinct labels; a duplicate-label or shadowing policy must be explicit rather than
relying on a silent row-map collapse.

### Handler semantics

Deep and shallow are properties of a handler and are not part of the control
surface. A deep State handler carries its state value forward and reinstalls itself
around the continuation, so later requests at the same slot are handled again; a
shallow handler returns control outside the delimiter after the current request.
The library chooses by how it calls the continuation inside its `run` handler.

A handler that does not handle a request forwards it, and the residual row describes
exactly what remains. A pure handler collects or discards a slot for testing; a
handler that reaches a host service needs an explicit native boundary, proposed in
the [native Effect/ST topic](native-effect-and-st-handlers.md).

## Design

### Ownership

| Owner | Responsibility |
| --- | --- |
| Type system | Ordinary kinds, rows, class evidence, quantifiers, roles, constructor visibility, and applicable region constraints. No effect rows or handlers. |
| Library | Operation signatures and functors, computation construction, handler interpretation, residual rows, continuation composition, and the one-shot/multi-shot API choice. |
| Compiler | The effect-agnostic `Control f a` carrier, `return`/`bind`/`perform`/`run`, and the selective CPS/defunctionalization lowering — owned by [delimited control and resumable frames](delimited-control-and-resumable-frames.md). |
| Runtime | Real IO/storage primitives; an optional trampoline or driver; if separately adopted, target stack switching and native resumption lifetime checks. |

No pass discovers an operation by the spelling `State`, `Console`, `Run`, or
`Effect`. No pass interprets handler source to infer a hidden protocol. Generic type,
representation, and CFG verification remain required.

### The library on the control surface

The library builds on the compiler's surface — `Control f a` with `return`/`bind`/
`perform`/`run`, specified in
[delimited control and resumable frames](delimited-control-and-resumable-frames.md).
It defines its computation as that carrier over an operation functor:

```purescript
type Run r a = Control (VariantF r) a
```

An operation is a library functor member, lifted with the perform primitive:

```purescript
get :: forall s r. Run (state :: StateF s | r) s
get = perform (inj (StateF (Get identity)))     -- schematic; the library owns inj
```

A handler is `run`'s rank-2 argument: it receives the handler state, the request, and
the continuation, and returns the next state together with the next computation,
deciding to resume, abort, or replay. Deep semantics is the library re-entering `run`
when it forwards a request it does not handle; the compiler does not decide
deep/shallow.

The surface is monadic, so effectful code is written with the library's operations
and `bind` (via `do`). There is no direct-style effect syntax and no effect typing
(see the [scope](delimited-control-and-resumable-frames.md#scope) of the mechanism
document).

### Rejected shortcuts

- Making a concrete operation, `Run`, or an effect row a compiler-known identity;
  the compiler sees only opaque request functors.
- Attaching effect requirements to all function arrows without a language/type
  system decision.
- Reconstructing handler behavior in Core, CC, or MIR to recover an implicit State
  protocol.
- Treating a row requirement as linearity, single-use, or resource-lifetime
  evidence.
- Replacing native Effect/ST or public foreign APIs merely to get an algebraic
  effects example to compile.
- Providing a whole-function intrinsic for a library loop merely to bypass a
  representation gap.

## Algorithms

### Interpretation and forwarding

For a pure State handler carrying value `s`:

```text
handle(s, Return(a))       = Return({ state: s, value: a })
handle(s, Get(k))          = handle(s, k(s))
handle(s, Put(s', next))   = handle(s', next)
handle(s, Other(op, k))    = Request(op, x -> handle(s, k(x)))
```

The library expresses this as a `run` handler over the control surface; the
mechanism that captures `k` and drives the trampoline is
[delimited control and resumable frames](delimited-control-and-resumable-frames.md).
The final rule preserves the captured state while the outer handler interprets
another operation, and re-entering the State handler gives deep semantics. It does
not statically inspect the future continuation.

Type checking is ordinary application checking: operation lifting proves row
membership; handler application produces the residual row; a pure runner accepts
only the closed empty row. Any residual request is a type mismatch at that boundary,
assuming safe construction and the library's declared contracts.

## Code map

This is the design target, not a claim that these APIs exist yet:

| Location | Intended responsibility |
| --- | --- |
| Independent `psrs-stdlib` package | Upstream computation libraries, the `Run`/operation/handler wrapper over the control surface, ordinary operation modules, source audit, and conformance fixtures |
| Existing kind/row/class/rank-N owners | Check library source using standard PureScript rules; repair shared deficiencies without effect-specific solvers |

The control mechanism's code map is in
[delimited control and resumable frames](delimited-control-and-resumable-frames.md).

## Invariants and verification

- Library computations are inert until interpreted, while ordinary strict arguments
  are evaluated according to PureScript semantics.
- Operation results match their checked signature. Safe dispatch cannot exchange
  unrelated payload/result types.
- A handler removes exactly its advertised operation requirement and preserves
  residual requests and the appropriate continuation.
- The pure boundary requires a closed empty operation row. This is not a proof about
  unsafe coercions or arbitrary FFI hidden in user code.
- Resource safety is a separate contract. A bare handler does not prove
  release-on-trap or prevent use-after-release.

Evidence must separately cover type rejection, pure interpretation, and generic
representation; the lowered mechanism's evidence (frame identity, deep/shallow,
one-shot/multi-shot, stack safety) belongs to its own topic. The
[acceptance checklist](../../../implementation/backend/algebraic-effects.md) records
the requirements.

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

Constructing `program` does not allocate a mutable cell or execute native IO.
Interpretation changes the handler's ordinary state value to 41, supplies 41 to the
continuation of `get`, and returns `{ state: 41, value: 42 }`. The compiler sees
constructors, functions, and calls; there is no State token to project, no
write/read dependency graph, and no operation-specific witness. The frame-level
trace of the same program, under the control mechanism, is in
[delimited control and resumable frames](delimited-control-and-resumable-frames.md#worked-example).

Adding a Console slot leaves that slot in the residual row after `handleState`. A
pure log-collecting handler can remove it for testing. A host-writing handler needs
an explicit native IO boundary, proposed in the
[native Effect/ST topic](native-effect-and-st-handlers.md); a pure runner cannot be
used as that boundary.

## Boundaries and interfaces

Source syntax, type arrows, and standard row rules remain unchanged. This topic adds
no effect syntax and no effect rows; the compiler adds no ambient effect inference
and does not recognize a `Run` or `Effect` constructor specially.

The library and the compiler divide cleanly: operations, rows, handlers, and the
one-shot/multi-shot API are library code; only the carrier, the four primitives, and
their lowering are compiler code
([delimited control and resumable frames](delimited-control-and-resumable-frames.md)).
A library cannot make the optimizer unsound, and the compiler cannot change an
operation's meaning.

Native services remain explicit checked bindings with ordinary value/ABI contracts.
Command execution uses an ordinary configured runner where supported, without
discovery of `Effect Unit` or hidden interpretation of operation rows. This topic
does not reopen the deleted legacy runner path.

## Open questions and future work

1. **Reference-library acceptance.** Audit the pinned upstream Run/Free/Variant
   packages, accept the reference library on this target, and build the row-based
   (`VariantF`) library over the closed prototype. No compile or runtime acceptance
   has been measured for the reference library.
2. **Operation-row encoding.** Confirm the reference encoding (`VariantF` over an
   open row) is supported end to end, or record the compiler gap it exposes.
3. **Scoped and higher-order effects, bracket/finalization.** These need their own
   semantics; ordinary first-order operation rows do not provide those guarantees.
4. **Async and concurrency.** Scheduling, cancellation, and thread interaction are
   separate tracks.
5. **Native Effect/ST.** Validate the separate
   [native proposal](native-effect-and-st-handlers.md); its current package
   consumers remain unsupported after compiler cleanup.

## References

- [DEC-03](../../../decision/DEC-03-purescript-faithful-type-system.md) and the
  [compiler cleanup handoff](../../../implementation/backend/effect-state-removal-handoff-2026-10-11.md).
- [Delimited control and resumable frames](delimited-control-and-resumable-frames.md),
  the compiler mechanism these handlers run on.
- [PureScript Run](https://github.com/natefaubion/purescript-run) and its
  [API](https://pursuit.purescript.org/packages/purescript-run/5.0.0/docs/Run).
- [PureScript Free](https://github.com/purescript/purescript-free) and
  [Stack Safety for Free](https://functorial.com/stack-safety-for-free/index.pdf).
- [Native Effect and ST handlers](native-effect-and-st-handlers.md).
