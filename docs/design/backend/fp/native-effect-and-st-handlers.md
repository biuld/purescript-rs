# Native Effect and ST over Algebraic Handlers

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Accepted 2026-10-16. NE-01–NE-13 are Verified in the [native checklist](../../../implementation/backend/native-effect-and-st.md). The independent package commit and compiler lock revision are `4c73aab52226e8f5e7b1f17052f1a19d717a472e`. D-04 scoreboard numbers were not remeasured.  
**Prerequisites:** [algebraic effects and handlers](algebraic-effects-and-handlers.md),
[DEC-03](../../../decision/DEC-03-purescript-faithful-type-system.md),
[roles and coercions](../../frontend/type-system/prim.md),
[polymorphism and erasure](polymorphism-and-erasure.md), and
[runtime linking](../wasm/linking-and-runtime.md).  
**Summary:** Effect and ST are constructor-hidden library newtypes over the same
ordinary algebraic computation representation. A library native handler executes
typed storage and host requests using existing generic primitives and bindings.
ST safety follows from its rank-N runner, nominal region roles, and restricted
safe constructors, not compiler State analysis. The common representation
preserves the official ST.Global-to-Effect conversion contract.

## Scope

This topic proposes the native Effect/ST implementation beneath the official
public signatures, exports, instances, roles, and ordinary pure definitions.
It includes Effect.Ref, STRef, mutable-array boundaries, native execution, and
the bridge from user-defined algebraic effects to native operations.

It does not add syntax, ambient effect inference, primitive State types, a
special compiler-known Effect constructor, or a new execution intrinsic. The
generic handler mechanism is selected by
[DEC-19](../../../decision/DEC-19-compiler-generated-resumable-frames.md) as
compiler-generated resumable frames; this representation can be interpreted by an
explicit library interpreter or by that mechanism without changing its public API.
Compiler cleanup remains in force. This design is not authorization to change
the independent package, publish a lock, or claim restored runtime support.

## Background

The official libraries distinguish unrestricted native IO (`Effect`) from
region-restricted mutation (`ST`). `ST.run` has the rank-N signature
`forall a. (forall r. ST r a) -> a`. ST and STRef have nominal region roles.
`ST.Global.toEffect :: ST Global ~> Effect` is implemented with `unsafeCoerce`
in official source, relying on compatible native representations.

The withdrawn compiler protocols and the development package's current State
declarations cannot provide the foundation for this implementation. Existing
ArrayFill, ArrayIndex, ArrayWrite, ArrayLength, and WIT/WASI binding facilities
provide useful leaf operations. Their existence does not establish a complete
native handler or a safe ST API.

General algebraic operation rows express composable requests. The official
`Effect a` and `ST r a` APIs do not expose such a row. Their implementation can
seal a native request vocabulary and retain their original public types.

## Model

### Common computation representation

The private target library supplies this schematic representation:

```purescript
data Val
data NativeView
  = Ret Val
  | Request (NativeF Val)
  | Bind (NativeProgram Val) (Val -> NativeProgram Val)
data NativeProgram a = NativeProgram NativeView

newtype Effect a = Effect (NativeProgram a)
type role Effect representational

newtype ST :: Region -> Type -> Type
newtype ST r a = ST (NativeProgram a)
type role ST nominal representational

newtype Ref a = Ref (Cell a)
type role Ref representational

newtype STRef :: Region -> Type -> Type
newtype STRef r a = STRef (Cell a)
type role STRef nominal representational
```

`r` retains the official Region kind. Region and Global remain library marker
declarations; the compiler gives them ordinary declaration identity and role
checking, not a State protocol. NativeProgram does not import ST's Region kind,
which avoids a cycle between the representation owner and ST.Internal.

Both wrappers have exactly the same declared field type for the same `a`.
No distinct operation-row layout or region-specialized representation is
hidden behind the official Global conversion. Newtype lowering remains generic.
The chosen computation representation must support the official representational
result role. If its inferred roles do not, fix the library representation or
record the design blocker; do not force an invalid role annotation.

The implementation's `PSRS.Effect.Native.Program` uses this phantom-result free
view. Importing public `Run` here would cycle through its dependency on Effect.
The erased answer transport follows the private checked constructor signatures,
as in the official Free representation; it is not an operation tag-to-type map.
Every bind constructs a node. In particular, `bind (pure a) k` must delay `k`
until interpretation, and running the stored program twice invokes `k` twice.
Strict source arguments to the constructor still evaluate normally.

The native driver keeps pending callbacks in an explicit persistent heap stack.
Bind pushes one frame, Ret pops one frame or returns, and Request performs one
leaf before continuing with the same stack. Request methods apply their supplied
continuation to the checked leaf result. No driver transition recurses through
a composed chain of callbacks. Deep binds, public loops, and mixed native
requests require executed stack-safety evidence.

Effect/ST constructors remain hidden. No public Newtype instance or coercion
witness exposes their NativeProgram fields. In particular, do not export
`NativeProgram a -> ST r a`: it would allow host requests to enter safe ST.
Internal construction helpers belong to their owning library implementation;
any unavoidable exposed unsafe adapter must be identified as unsafe and audited.

### Native requests

```purescript
data NativeF next
  = Storage (StorageF next)
  | Host (HostF next)
  | Abort
```

StorageF represents allocation, read, and write requests. HostF is the library's
composition of typed operation families for the selected WIT services. Abort
has no continuation and cannot return normally. Functor mapping changes only
the continuation, never performs an operation.

NativeF is a shared private vocabulary, not a compiler operation registry.
Safe ST constructors expose only storage requests; safe Effect constructors
may expose storage and host requests. This restriction is established by the
library implementation and its export/coercion boundary, not a phantom region
alone. The fixed internal native row is not a claim that a row solver proves
all ST requests are storage-only.

### Heterogeneous storage requests

One program may allocate an Int cell and a String cell. Plain PureScript ADTs
must not be written as if they supplied GADT existential constructors. Use a
rank-N eliminator or an equivalent established library encoding. For example:

```purescript
newtype StorageF next = StorageF
  (forall z.
    { new :: forall a. a -> (Cell a -> next) -> z
    , read :: forall a. Cell a -> (a -> next) -> z
    , write :: forall a. Cell a -> a -> next -> z
    } -> z)
```

This sketch preserves the relationship between a cell's element type, the
operand, and the continuation result at each request. A smart constructor
selects a method with one checked `a`; an interpreter method accepts every `a`.
Host families similarly preserve each operation's checked payload/result type.
Do not encode requests as integer tags plus untyped values or use unsafeCoerce
to recover arbitrary payload types.

The interpreter's generic leaf calls still require ordinary checked boxing,
unboxing, and ABI adaptation. Those are representation responsibilities, not
native request interpretation by the compiler.

## Design

### Construction, composition, and execution

`pure` constructs a return node. `map`, `apply`, and `bind` use ordinary
computation-library operations. Building an action does not allocate a native
cell, call WASI, or execute a callback belonging to a future request. Strict
factory arguments retain normal PureScript evaluation behavior.

The native interpreter selects a library request clause, performs the leaf
operation, and continues exactly once with its result. It does not expose
multi-shot native resumptions through the Effect/ST APIs. The stored program
is reusable: running it twice executes it twice and performs fresh allocations
for allocation requests on each run. References captured before either run
retain their identity and mutations, as in the official native semantics.

The private execution boundary has schematic type
`executeNative :: forall a. NativeProgram a -> a`. Its body is an ordinary
library interpreter invoking observable primitives, not a pure `Run ()`
eliminator or a compiler intrinsic. This function is an unsafe native boundary;
it must not be advertised as a generally pure handler or exposed as an
unrestricted safe runner.

`Effect.Unsafe.unsafePerformEffect` unwraps Effect in its owning implementation
and invokes this boundary. Its public type and unsafe status remain official.
`PSRS.Command.run :: Effect Unit -> Int` invokes it once and returns zero on
normal completion. A trap propagates. The compiler uses the configured ordinary
runner and never identifies `Effect Unit` specially.

ST.run uses the same execution machinery behind its rank-N boundary. Its
observable purity relies on the safe API permitting only isolated mutation;
unsafe coercions, arbitrary unsafe FFI, and explicitly unsafe array operations
are outside that guarantee. Generic optimizer treatment of the interpreter's
observable calls remains conservative; no new purity annotation is justified.

### ST region safety

Preserve the exact signature `run :: forall a. (forall r. ST r a) -> a` and
nominal roles on ST, STRef, and STArray region parameters. A result containing
an externally usable region-specific reference or action cannot satisfy a
region-independent `a` through safe construction. The type checker owns
skolem scope, quantifier checking, and Coercible visibility in the ordinary way.

The handler needs no runtime region token and no scan of captured references.
Its objects reside in the GC language heap. Rank-N isolation does not imply
deterministic arena reclamation: unreachable cells are collected by GC.
Nested ST runs receive independent type scopes through normal checking.

Constructor hiding, region roles, rank-N checking, and trusted library bodies
are all necessary. A phantom `r` by itself proves neither isolation nor
storage-only behavior. Negative source cases must test cross-region use,
reference/action escape, region coercion, and attempts to inject host programs.

### Global conversion

Preserve the official `ST.Global.toEffect = unsafeCoerce` implementation only
after source-field and executed-behavior evidence establishes common NativeProgram
representation. A converted action is interpreted by the native handler,
which already handles storage requests. Global references can survive and be
used by subsequent converted actions in Effect.

Do not reconstruct this conversion by a compiler-recognized region name, a
State/RealWorld cast, or a distinct row reinterpretation. Generic representation
code must preserve the common field contract; adapter failures are generic
compiler defects, not reasons to add a Global special case.

### Leaf operations and cells

| Native request | Existing capability | Library behavior |
| --- | --- | --- |
| New cell | ArrayFill | Allocate a fresh one-slot cell at interpretation time |
| Read cell | ArrayIndex | Read the current slot with the checked element type |
| Write cell | ArrayWrite | Mutate the same slot; discard the returned array as appropriate |
| Array length | ArrayLength | Inspect backing storage without changing public mutable-array contracts |
| Host operation | Typed WIT/WASI foreign binding | Call the selected import with its ordinary checked parameters/result |
| Abort | Existing non-returning trap support | Stop interpretation; produce no fabricated result or successor |

The independent GC storage provider may implement equivalent checked leaf
operations where its generic external-binding contract is available. Do not
require both routes or retain State-shaped source signatures. Audit the
existing provider ABI's historical State comments and contracts before reuse;
physical exports alone do not prove a valid generic source binding.

One cell must retain its physical identity across all polymorphic reads and
writes. An erased-array adaptation that copies the array on write is invalid.
Generic optimizer rules must treat reads from mutable backing storage and
in-place writes correctly; a trapping-read annotation alone is not a proof
against caching or reordering a read across a write. These are ordinary memory
and call contracts, not a request for a new State dependency analysis.

Cell can use `Array (Maybe a)` privately. Initialized allocation publishes
`Just value`; Effect.Ref.newWithSelf first allocates Nothing, calls its callback
exactly once with that cell, then publishes the callback result. Reading an
unpublished cell traps under the existing target adaptation. This is a stated
Wasm representation boundary, not fabricated initialization and not a claim
that JavaScript's transient null value is observationally identical.

Effect.Ref.write returns Unit; ST.write returns the supplied value. modify'
reads once, invokes the source callback once, writes its state result, and
returns its value result. It is not a concurrency/atomicity primitive. Preserve
re-entrant behavior where an explicitly unsafe callback performs other effects.

### Mutable arrays and official source fidelity

Retain official loop, traversal, callback, and MonadRec definitions. Provide
ordinary target adapters for native foreign slots rather than whole-function
intrinsics. Safe thaw/freeze must satisfy their copy/isolation contracts;
unsafe thaw/freeze may share only according to their explicit unsafe contract.
Arrays reachable outside a safe ST run must not be mutated through a supposed
safe thaw. Backing storage growth and replacement belong to library algorithms
and sufficiently small checked primitives, with real identity behavior tested.

ArrayFill/ArrayWrite are not evidence that every STArray operation is already
implementable. Missing resize, storage transport, or generic callback support
must remain explicit until a focused case establishes the owning gap.

Public Effect and ST signatures, roles, instances, modules, and exports remain
official. Replacing foreign data declarations with constructor-hidden newtypes
and replacing native foreign bodies is a target representation adaptation
requiring an independent package audit and runtime evidence. Do not rewrite
official pure functions or remove declarations to accommodate compiler gaps.

### User-defined algebraic effects

Public user computations may use an extensible Run row. A library interpreter
can handle a Console or application-specific operation into Effect; other
handlers may remain pure. Interpretation into Effect constructs a native
program and does not execute it prematurely.

The native row remains private and fixed for the official facade. Arbitrary
user handlers and multi-shot continuations are not accepted as an ST isolation
proof or silently installed as the native runner. Public algebraic APIs and
the opaque official native APIs have distinct construction boundaries.

## Algorithms

```text
native(Return(a)):
    finish with a
native(Storage(New(value, k))):
    cell = allocate_and_publish(value)
    continue k(cell)
native(Storage(Read(cell, k))):
    value = checked_read(cell)
    continue k(value)
native(Storage(Write(cell, value, next))):
    checked_in_place_write(cell, value)
    continue next
native(Host(request, k)):
    result = checked_foreign_call(request)
    continue k(result)
native(Abort):
    trap
```

This is semantic pseudocode. The implementation uses the established
stack-safe computation machinery, not recursive evaluation of a naive bind
tree. Host request typing and storage rank-N elimination determine checked
calls; no erased tag-to-type recovery occurs in the interpreter.

Generic tail calls, adapters, and optimization must preserve this behavior.
Interpreter stack safety must be tested with long mixed request/bind sequences,
not just with a tail-recursive pure counter.

## Code map

| Owner | Intended contents |
| --- | --- |
| Independent package's private native module(s) | Shared NativeProgram, typed request encodings, leaf bindings, and native interpreter |
| Effect owning module and unsafe adapter | Hidden wrapper, ordinary combinators, safe native constructors, and unsafe execution bridge |
| ST.Internal owning module | Hidden wrapper, nominal region facade, source foreign-slot bodies, and rank-N run |
| Effect.Ref and ST.Ref adapters | Typed Cell wrappers and official return/callback behavior |
| ST.Global | Official Global marker and representation-compatible conversion |
| Existing mutable-array target adapters | Official array API algorithms and small storage calls |
| PSRS.Command | Ordinary explicitly configured command runner |
| Existing compiler and runtime owners | Generic representation, observable primitive semantics, ABI validation, and actual leaf implementations |

Module names are an implementation choice; do not create a compiler native
operation enum or new crate. Import dependencies must remain acyclic. Helpers
must not expose a safe arbitrary-program-to-ST conversion. Constructor scope
and imports are checked explicitly before choosing module boundaries.

## Invariants and verification

1. Effect and ST of the same result type have the same NativeProgram field and
   the official Global conversion works without protocol-specific lowering.
2. Construction is inert; one native interpretation performs each encountered
   request once, in program order. Repeated interpretation repeats operations.
3. Every storage request retains its element/operand/result relationship
   through polymorphism, boxing, and callback invocation.
4. Safe ST construction permits isolated storage, with nominal region roles,
   hidden constructors, and rank-N run preserving ordinary scope rules.
5. No uninitialized value, missing binding, invalid adapter, or host failure is
   replaced with a successful-looking value.
6. Native execution uses generic observable-call contracts. No compiler pass
   recognizes native operation families or reconstructs their chain.
7. Source fidelity, compile acceptance, and executed behavior are independently
   measured. This proposal establishes none of those measurements.

The [acceptance checklist](../../../implementation/backend/native-effect-and-st.md)
names required evidence, including real public API and unsafe-boundary tests.

## Worked example

```purescript
answer :: Int
answer = ST.run do
  ref <- STRef.new 0
  _ <- STRef.write 41 ref
  value <- STRef.read ref
  pure (value + 1)
```

The library builds New, Write, and Read requests. ST.run interprets them under
the region-independent result type. The handler allocates one cell, mutates it
to 41, supplies 41 to the read continuation, and returns 42. The compiler sees
ordinary request values, closures, and intrinsic calls; it sees no region token.

A Global variant converted with toEffect uses the same program and interpreter.
Effect.Ref.new likewise allocates at execution time, so repeated runs of one
allocation action produce independent cells. A previously captured Ref remains
the same cell across repeated reads/writes.

## Boundaries and interfaces

The public facade types remain official. Private native request rows are not
added to function arrows or inferred as ambient effects. Existing Coercible,
rank-N, kind, and generic representation rules apply unchanged.

Leaf storage uses the GC language heap. Host calls retain WIT resource and
canonical ABI ownership rules. A trap is not automatic finalization; resource
bracketing, exceptions, and cancellation need their own explicit contracts.

This proposal uses the generic handler mechanism selected by DEC-19 but does not
require target stack switching, and it does not prohibit a future target option
that preserves its native API and multiplicity. Any target continuation
implementation must meet the separate control obligations in the
[algebraic-effects topic](algebraic-effects-and-handlers.md) before replacing
compiler-generated frames.

## Open questions and future work

- Prototype rank-N heterogeneous request transport and the common Global
  representation before migrating public modules. Any compiler fixes must
  also work for unrelated rank-N ADTs/records and generic closures.
- Audit the full public Effect/ST/Ref/uncurried/array API and exact target
  adapters. The current State-dependent package is not an implementation of
  this proposal, and the normal compiler lock must not move prematurely.
- Measure interpreter allocation and deep execution. Select an established
  stack-safe representation without special optimizer exemptions.
- Confirm trap lowering and the remaining generic external storage binding
  contracts. Do not rely on historical State projection declarations.

## References

- [Algebraic effects and handlers](algebraic-effects-and-handlers.md),
  [source fidelity](../../../workflow/stdlib-vendoring.md), and
  [conformance workflow](../../../workflow/stdlib-conformance.md).
- [Official ST.Internal](https://github.com/purescript/purescript-st/blob/master/src/Control/Monad/ST/Internal.purs)
  and [ST.Global](https://github.com/purescript/purescript-st/blob/master/src/Control/Monad/ST/Global.purs).
- [Official Effect](https://github.com/purescript/purescript-effect) and
  [Effect.Ref](https://github.com/purescript/purescript-refs).

The pinned local upstream ST and Ref source and JavaScript bodies were inspected
on 2026-10-11. They establish the referenced API/behavior contracts, not target
acceptance of this proposed implementation.
