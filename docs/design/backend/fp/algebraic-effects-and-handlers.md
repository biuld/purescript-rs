# Library-Owned Algebraic Effects and Handlers

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Draft proposal; no implementation or accepted native Effect replacement.  
**Prerequisites:** [DEC-03](../../../decision/DEC-03-purescript-faithful-type-system.md),
[functional Core](../../frontend/semantics/functional-core.md),
[rows and records](../../frontend/type-system/rows-and-records.md),
[representation and evidence](representation-and-evidence.md),
[control flow](control-flow-and-tail-calls.md), and
[runtime linking](../wasm/linking-and-runtime.md).  
**Summary:** Algebraic operations and their handlers are library values, checked
using ordinary PureScript types and rows. The compiler owns generic language
semantics and representation, not a State/IO protocol or operation dependency
graph. Explicit library continuations provide a portable semantic model;
runtime delimited continuations are a separately specified execution option,
not an assumed ability of an ordinary Wasm import. The separate
[native Effect/ST proposal](native-effect-and-st-handlers.md) specifies public
facades without changing this generic operation model.

## Scope

This topic owns the meaning of library-defined algebraic operations, handler
composition, row-based operation requirements, and continuation boundaries.
It specifies the division of responsibility between library, compiler, and
runtime and the conditions for adopting runtime control support.

The following are separate decisions: official Effect/ST representation and
source adaptation (proposed in the native topic), a native effect type system, new surface syntax, resource
ownership types, async scheduling, and target stack switching. Designing
algebraic effects does not authorize rebuilding the withdrawn compiler-owned
Effect or State mechanisms.

The proposal follows DEC-03's library encoding. A runtime option that requires
new compiler control primitives must be reviewed against DEC-03 before being
selected. This document does not silently amend that accepted decision.

## Background

An algebraic operation describes a request without choosing its interpretation.
A handler supplies that interpretation and decides how the remaining
computation proceeds. The same console operation can write to a terminal or
append to a test log; the same state operations can thread a pure value or use
mutable storage.

A continuation represents the remainder of a computation. It can be explicit
data and functions in a library, or a suspended execution segment managed by
a control runtime. These representations have different implementation costs;
neither requires a compiler to know the meaning of a particular operation.

PureScript already provides ADTs, functions, higher-kinded types, type classes,
rank-N polymorphism, ordinary rows, and monadic `do`. `purescript-run` encodes
extensible operations using `Free` and `VariantF`; its handlers are ordinary
interpreters. This establishes expressibility in the language, not acceptance
by this compiler or availability of a native IO implementation.

An operation row is a library parameter, not a second effect system attached
to every function arrow. Its constraints describe which requests a computation
can expose. They do not prove single use, termination, absence of unsafe FFI,
or correct resource release.

## Model

### Computations and operation signatures

Use the following schematic public model, matching the kind of `Run`:

```purescript
Run :: Row (Type -> Type) -> Type -> Type

data StateF s next
  = Get (s -> next)
  | Put s next

data ConsoleF next
  = Print String next
```

Each signature has a Functor instance. `Get` receives the operation result
before continuing; `Put` and `Print` have a unit result. Operations are lifted
into a row-indexed computation by library functions. Binding constructs the
remaining computation without performing requests.

The semantic computation tree has `Return a` and `Request op continuation`.
This is a mathematical model, not a required runtime layout. An implementation
may use freer/Coyoneda representations and queued binds to avoid restrictive
Functor implementations and inefficient left-associated bind chains.

### Handlers and residual rows

These are schematic interfaces, not new compiler intrinsics:

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

Row membership and removal use ordinary row relations and library evidence.
The actual API must preserve the upstream row constraints, including label
absence where needed; the schematic signatures do not authorize weakening
those constraints. State here is an ordinary value `s`, not a primitive token.

A handler handles its selected slot and forwards every other request with a
continuation that re-enters the handler. This is deep-handler semantics: after
resumption, subsequent requests at the same slot are handled again. Distinct
instances use distinct labels; a duplicate-label or shadowing policy must be
explicit rather than relying on a silent row-map collapse.

### Resumption and lifetime

The portable model uses persistent explicit continuations. A handler may resume
zero, once, or multiple times when its API permits it. A pure state handler
supplies a state value to each branch; replaying a continuation does not roll
back mutations or external IO already performed by another branch.

Single-use native resumptions require a distinct abstract API. Existing
PureScript arrows are unrestricted: hiding a constructor or adding a nominal
type parameter does not prevent callers from invoking a value twice. A native
one-shot implementation must check consumption at runtime unless a separate
static usage discipline has been adopted.

## Design

### Ownership

| Owner | Responsibility |
| --- | --- |
| Type system | Ordinary kinds, rows, class evidence, quantifiers, roles, constructor visibility, and applicable region constraints |
| Library | Operation signatures, computation construction, handler interpretation, residual operation rows, continuation composition, and policies |
| Compiler | Strict evaluation, ordinary closures/ADTs/dictionaries, generic calls and ABI checks, and sound optimization/control lowering |
| Runtime | Real IO/storage primitives; if separately adopted, delimited control and native resumption lifetime checks |
| Target profile | Explicitly adopted Wasm features and rejection of unavailable capabilities |

No pass discovers an operation by the spelling `State`, `Console`, `Run`, or
`Effect`. No pass interprets handler source to infer a hidden protocol. No
operation graph, frozen source inventory, or cross-pass State witness is
required. Generic type, representation, and CFG verification remain required.

The compiler preserves observable calls and traps even when their results are
unused. The library cannot make an optimizer sound by providing a handler:
generic call semantics and conservative effect summaries remain compiler
responsibilities. Inlining and specialization must work for these values under
the same contracts as other ordinary higher-order programs.

### Portable execution

The default proposal under DEC-03 is an ordinary library interpreter over
explicit continuations, with stack-safe library machinery. This is not a
decision to represent native Effect as `Unit -> a`. Pure handlers need no native
control runtime. Host-facing handlers eventually invoke checked foreign/runtime
operations; this topic does not select the currently missing native Effect ABI.

Reuse upstream `purescript-run`/`purescript-free` semantics and sources where
appropriate rather than making a compiler-known replacement monad. Audit their
actual FFI and representation needs before claiming target compatibility.
Performance needs a measured implementation: a naive tree interpreter or
recursive bind evaluator is not sufficient stack-safety evidence.

### Runtime delimited continuation option

Delimited control limits capture to a particular execution boundary. A handler
can suspend a computation at a request, examine its payload, and resume it with
an operation result. This can support efficient direct execution without
materializing every intermediate computation tree, but is not free stack access.

Two valid runtime architectures are candidates:

1. A compiler-generated continuation representation (CPS or resumable frames)
   whose frames are managed by a runtime scheduler.
2. A separately enabled target continuation/stack-switching facility, with
   compiler lowering and validation for that facility.

A normal imported Rust/Wasm function cannot capture the caller's Wasm execution
stack. Rust's private linear-memory stack is not that execution stack; moving
its stack pointer or saving registers is not a valid continuation implementation.

Any native option must define all of the following before implementation:

- A typed boundary answer, request payload/result relation, and abstract
  resumption. Runtime dispatch uses checked identity/evidence; it must not cast
  an arbitrary label and payload into an unrelated result type.
- Deep versus shallow resumption and the placement of the handler delimiter.
  Handler clauses execute outside the handled delimiter unless explicitly
  re-entered, so their own requests are forwarded or handled by an outer scope.
- A one-shot or multi-shot API. One-shot consumption is checked before transfer;
  multi-shot support needs an explicit frame/stack cloning contract and cannot
  be implemented by bypassing a one-shot guard.
- Boundary lifetime, captured GC roots, cancellation, abandonment, and return
  to the handler. Suspended continuations cannot retain borrowed canonical ABI
  buffers or invalid host-resource references.
- Foreign-call boundaries. Capture across a non-suspendable host/import frame
  is rejected unless that boundary has an explicit suspension protocol.
- Representation of the resumable computation in existing PureScript types.
  Raw capture primitives must not inhabit an unrestricted pure function type
  while changing its control behavior. An abstract computation API or another
  justified typed boundary is required.

Conceptually, a one-shot continuation has `Pending`, `Running`, `Consumed`, or
`Cancelled` status. The transfer from `Pending` must atomically consume the
resumption with respect to re-entrancy; failure does not silently permit a
second transfer. The status belongs to a runtime object, not a compiler State
token analysis. Return, cancellation, and non-resuming clauses release captured
storage under the selected lifetime contract.

No new control intrinsic, Rust feature, Wasm proposal, or opaque public type is
selected by this draft. DEC-03 and target capability decisions must explicitly
authorize a native option. Runtime one-shot and portable multi-shot behavior
must have distinguishable APIs; an optimization cannot silently change one
into the other.

### Rejected shortcuts

- Attaching effect requirements to all function arrows without a language/type
  system decision.
- Reconstructing handler behavior in Core, CC, or MIR to recover an implicit
  State protocol.
- Calling a generic runtime import a stack-capture primitive without a valid
  target or compiler-generated continuation representation.
- Treating a row requirement as linearity or resource-lifetime evidence.
- Replacing native Effect/ST or public foreign APIs merely to get an algebraic
  effects example to compile.

## Algorithms

### Interpretation and forwarding

For a pure State handler carrying value `s`:

```text
handle(s, Return(a))       = Return({ state: s, value: a })
handle(s, Get(k))          = handle(s, k(s))
handle(s, Put(s', next))   = handle(s', next)
handle(s, Other(op, k))    = Request(op, x -> handle(s, k(x)))
```

Actual evaluation uses an explicit stack-safe work loop or upstream trampoline
machinery. The final rule preserves the captured state while the outer handler
interprets another operation. Re-entering the State handler ensures deep
semantics. It does not statically inspect the future continuation.

Type checking is ordinary application checking: operation lifting proves row
membership; handler application produces the residual row; `runPure` accepts
only the closed empty row. Any residual request is a type mismatch at that
boundary, assuming safe construction and the library's declared contracts.

### Native control, if selected

Establish a typed delimiter, suspend the continuation at a request, transfer
to the appropriate handler clause, and consume/resume the continuation under
the native API. GC roots and delimiter state are retained until resumption or
cancellation completes. These are generic runtime/control operations; no
operation-specific analysis is performed during compilation.

## Code map

This is the proposed owner map, not a claim that these APIs are implemented:

| Location | Intended responsibility |
| --- | --- |
| Independent `psrs-stdlib` package | Upstream computation libraries, ordinary operation modules, handlers, source audit, and conformance fixtures |
| Existing kind/row/class/rank-N owners | Check library source using standard PureScript rules; repair shared deficiencies without effect-specific solvers |
| Existing Core/CC/MIR lowering and optimizers | Compile library code as ordinary higher-order code and preserve generic observable-call semantics |
| `psrs-runtime` ABI/catalog/executable owners | Small native IO/storage contracts and implementations, independent of operation-row or library identities |
| Runtime control owner, only if separately adopted | Delimiter/resumption ABI, captured-root storage, lifecycle checks, and capability requirements |

No new compiler crate, handler IR, State graph directory, or compiler-side
operation registry is required for the portable model. Runtime control module
names and entry-point signatures remain unselected until the native contract
above is complete.

## Invariants and verification

- Library computations are inert until interpreted, while ordinary strict
  arguments are evaluated according to PureScript semantics.
- Operation results match their checked signature. Safe row dispatch cannot
  exchange unrelated payload/result types.
- A handler removes exactly its advertised operation requirement and preserves
  residual requests and the appropriate continuation.
- The pure boundary requires a closed empty operation row. This is not a proof
  about unsafe coercions or arbitrary FFI hidden in user code.
- Closure conversion, adapters, and optimizations preserve evaluation behavior
  under ordinary IR contracts; they do not verify a private effect protocol.
- Aborting/resuming/replaying a continuation follows the chosen API. Native
  one-shot reuse is rejected; multi-shot execution does not imply IO rollback.
- Resource safety is a separate contract. A bare handler does not prove
  release-on-trap or prevent use-after-release.

Evidence must separately cover type rejection, pure interpretation, generic
representation, observable runtime behavior, and any native control capability.
The [acceptance checklist](../../../implementation/backend/algebraic-effects.md)
records proposed requirements; none are verified by this document.

## Worked example

The following is schematic library code using the signatures above:

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
Interpretation changes the handler's ordinary state value to 41, supplies 41
to the continuation of `get`, and returns `{ state: 41, value: 42 }`.
The compiler sees constructors, functions, and calls; there is no State token
to project, no write/read dependency graph, and no operation-specific witness.

Adding a Console slot leaves that slot in the residual row after `handleState`.
A pure log-collecting handler can remove it for testing. A host-writing handler
needs an explicit native IO boundary, proposed in
[native Effect/ST](native-effect-and-st-handlers.md); `runPure` cannot be used
as that boundary.

## Boundaries and interfaces

Source syntax, type arrows, and standard row rules remain unchanged. Library
type parameters express operation requirements. The compiler neither adds
ambient effect inference nor recognizes the `Run` constructor specially.

Native services remain explicit checked bindings with ordinary value/ABI
contracts. Command execution uses an ordinary configured runner where supported,
without discovery of `Effect Unit` or hidden interpretation of operation rows.
This topic does not reopen the deleted legacy runner path.

The baseline target profile does not gain stack switching through this design.
Native resumption support requires a pinned target capability, MIR/lowering
contract, validator coverage, and actual execution evidence. An unsupported
capability is an error, never a silent change to continuation semantics.

## Open questions and future work

1. Confirm whether explicit library continuations are the execution design or
   whether runtime delimited control is required. The library baseline follows
   DEC-03; the native alternative needs a separate reviewed decision.
2. Audit the pinned upstream Run/Free/Variant packages, their FFI, and this
   compiler's generic representation support. No compile/runtime acceptance
   has been measured for this proposal.
3. Validate the separate [native Effect/ST proposal](native-effect-and-st-handlers.md).
   Its current package consumers remain unsupported after compiler cleanup.
4. If adopting native control, settle answer typing, one-/multi-shot semantics,
   capture boundaries, and GC/lifetime contracts before choosing primitives.
5. Scoped/higher-order effects, bracket/finalization, async, and concurrency need
   their own semantics. Ordinary first-order operation rows do not provide
   those guarantees automatically.

## References

- [DEC-03](../../../decision/DEC-03-purescript-faithful-type-system.md) and
  [compiler cleanup handoff](../../../implementation/backend/effect-state-removal-handoff-2026-10-11.md).
- [PureScript Run](https://github.com/natefaubion/purescript-run) and its
  [API](https://pursuit.purescript.org/packages/purescript-run/5.0.0/docs/Run).
- [PureScript Free](https://github.com/purescript/purescript-free) and
  [Stack Safety for Free](https://functorial.com/stack-safety-for-free/index.pdf).
- [Continuing WebAssembly with Effect Handlers](https://arxiv.org/abs/2308.08347),
  a proposal-level reference, not evidence of this target profile's support.

External references were consulted earlier in this conversation. A fresh
online verification attempt on 2026-10-11 failed due to connection errors;
current upstream APIs and target support must be verified before implementation.
