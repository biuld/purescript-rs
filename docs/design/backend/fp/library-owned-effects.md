# Library-Owned Effects and State Dependencies

**Feature:** [F-02](../../../feature/F-02-portable-programs.md)  
**Status:** Draft design; newtype and state-dependency direction selected,
implementation migration in progress. Coverage is recorded in the
[implementation checklist](../../../implementation/backend/library-owned-effects.md).  
**Prerequisites:** [effects](effects.md), [representation and evidence](representation-and-evidence.md),
[primitive types](../../frontend/type-system/prim.md),
[foreign imports](../../frontend/semantics/foreign-imports.md),
[primitive bindings](../wasm/primitive-ffi-and-stdlib.md),
[Core optimization](../opt/core.md), [MIR](mir.md), and
[source fidelity](../../../workflow/stdlib-vendoring.md).  
**Summary:** Effect is an ordinary library newtype over a state-transforming
callable. General state primitives carry explicit sequencing dependencies
through checked Core, CC and MIR; the final encoding erases their zero-width
representation after verifying execution order. The library owns combinators,
loops, host adapters and execution policy. The runtime package owns executable
storage and termination primitives; the compiler owns their checked call and
dependency projection. A configured ordinary runner adapts
main to the command ABI without compiler recognition of Effect.

## Scope

This design replaces trusted Effect discovery, synthesized combinators, the
constant-token protocol, host suspension inferred from Effect result types,
and lexical runner restrictions. It also replaces PSRS.ST's Unit-triggered
representation with the same state protocol, preserving region safety.

The synchronous state protocol covers host I/O and mutable storage. Async
scheduling, cancellation, transactions, exception recovery and the complete
GHC runtime model are separate topics. Existing mechanisms remain implemented
until migration is validated. Neither earlier foreignWrap/foreignUnwrap
intrinsics nor the earlier Unit-thunk design are part of this repair.

## Background

Official PureScript declares `foreign import data Effect :: Type -> Type` and
implements foreign slots using JavaScript suspended functions. The selected
Wasm adaptation replaces that declaration with a library newtype and provides
foreign-slot bodies in the target library. Preserve public signatures, exports,
roles, classes, instances and ordinary pure definitions. This is an explicit
target source adaptation, not byte-identical restoration of the opaque type.

GHC represents IO as `newtype IO a = IO (State# RealWorld ->
(# State# RealWorld, a #))`. State# has a zero-width representation: it does
not contain a copy of the world or an incrementing counter. State-producing
operations and bind create a compiler-visible dependency chain. Their signatures,
optimizer rules and runtime boundaries jointly establish the contract.

Our current Core synthesis creates `[STATE_TOKEN] -> a`, passes the same token
to both actions and returns no state. CC lowers StateToken to constant zero.
That is an invocation trigger, not the selected state-dependency protocol.
PSRS.ST similarly uses `Action r a = Action (Unit -> a)` today. Both must migrate;
changing Effect alone would leave ST.Global.toEffect's unsafe conversion invalid.

## Model

### Primitive state and ordinary library types

The shared primitive declaration catalog owns the following proposed types,
identified by stable declaration IDs rather than spelling checks:

```text
Prim.State.State     :: forall k. k -> Type   -- primitive, region role nominal
Prim.State.RealWorld :: Type           -- primitive region marker
```

State has no source constructors, equality test, numeric value or safe coercion
between regions. RealWorld is a marker, not a mutable object. These are generic
low-level state facilities, not compiler-defined Effect or ST constructors.
The region kind is polymorphic so official `Region` and `Global :: Region`
declarations remain valid alongside `RealWorld :: Type`. Do not change the
official ST region kind to accommodate a target primitive.

The following library definitions are schematic. Step is an ordinary structural
record alias, so primitive signatures do not depend on a library-owned record
constructor or a synthesized source declaration:

```purescript
type Step s a = { state :: State s, value :: a }
newtype Effect a = Effect (State RealWorld -> Step RealWorld a)
type role Effect representational

newtype Action r a = Action (State r -> Step r a)
type role Action nominal representational
```

Effect belongs to the Effect module, with its constructor hidden from public
exports and no added public Newtype instance. PSRS.ST owns Action. Ordinary
newtype checking and lowering own identity, field templates, constructor scope
and result-role coercions. The State region role remains nominal through
substitution and physical erasure. Constructor-hidden Coercible scope must be
checked, not assumed from export syntax.

The callable representation contract retains the complete source field and its
constructor variables, the canonical storage field's owner, all call parameters
and its complete result type. Structured domains such as `State r` and structural
Step results must not be reduced to a fixed/argument-only parameter list or a
guessed final constructor argument. Transparent newtype storage keeps the declared
field's erased convention; a constructor application does not specialize its
physical parameter slots. Checked adapters relate concrete use types to that
convention. Nested newtypes retain their outer field application as evidence
while naming the declaration that owns the final canonical callable field.
Ordinary ADT field storage and recovery retain a checked instantiation from
the declared field template to its use type, scoped by the owning constructor's
parameters. Consumers use the Core-owned type relation; they do not infer
higher-kinded constructor bindings from an erased storage shape.

Abstract callable-constructor transport uses the same registered physical
protocol as other consumers. For a State call, payload erasure preserves the
Step's successor field and erases only its value field; recovery converts only
that value field. The shared State call projection owns this split. Treating
the whole Step as an ordinary erased record would box or recover State and
destroy the dependency contract. Missing signatures and malformed State call
shapes reject during protocol planning.

Source arrays use one erased-element storage representation at both concrete and
polymorphic use types. Construction and writes convert the checked element into
its payload storage protocol; indexing and patterns recover the checked element.
Passing an array through a bare variable, constructor field, callable boundary
or abstract Array constructor retains the existing storage reference. A pure
array update still copies before writing. Private text codec buffers cross an
explicit value conversion boundary and do not define source array identity.
Canonical WIT list adaptation retains the projected semantic element alongside
its erased storage, using the same payload owner protocol for records and nested
aggregates. Physical erasure must not remove the layout requirements carried by
that projection, including String storage used only by external results.

Bare polymorphic callable storage uses the shared payload conversion protocol.
Ordinary arguments pass through erased unary curry segments. A terminal State
argument retains its logical State shape and returns a complete Step whose
payload alone uses erased storage. State is never boxed or captured by a curry
segment. The shared State call projection owns successor/payload field positions
and labels; layout planning registers terminal signatures before publishing the
immutable representation table. Recovery uses the same protocol and preserves
the checked source region and complete result, including callable payloads.

Conversion sequences and product field conversions are expanded into CC
assignments before dependency evidence is published. Each adapter factory call
in these conversions therefore has an explicit operand and destination available
to invocation correspondence checks.
Conversion output shapes are shared with the verifier; computing a declared
output does not validate a conversion's input or semantic evidence. Array element
conversion calls still require their full loop correspondence contract.

State-aware host bindings use the same canonical ABI adapter as ordinary WIT
bindings. Their logical CC signature retains State and the complete Step;
the guest projection describes only ordinary arguments and the Step payload.
Canonical adapter planning produces an immutable instruction subgraph before
physical emission, anchored to the source owner, call identity, destination,
ordinary operands, and checked binding. The dependency verifier checks its
actual instructions, temporary types, internal edges, and canonical import.
Only that complete checked subgraph may be contracted to one logical invocation
for source ordering and control correspondence. Its exit remains the enclosing
source continuation. Added calls, altered adaptation or cleanup, changed imports,
and entry into an adapter through an unchecked source edge must reject.
This adapter contract does not authorize a source-level loop or multiway State
control flow without its own dependency correspondence.

A state value denotes a dependency point. An observable state operation consumes
its incoming dependency and yields a successor only on normal completion. A
pure action may return the incoming state unchanged. Successor identity is not
an observable source value and is not derived from different token bits.

State threading is not a claim that PureScript has linear types. Constructor
privacy and rank-N regions do not alone prove unique use. Safe library code
threads one current state per executed path. General dependency verification
must establish the operation order and preserve it under transformation;
unchecked library coercions and unsafe execution remain explicit trust boundaries.

`Effect (a -> b)` returns a function after one action; `Effect (Effect a)`
returns a second action after one action. Neither payload becomes extra
parameters of the outer state-transforming callable.

### General primitive contracts

Proposed registry operations are named here by responsibility; exact Rust and
binding names must follow the single intrinsic catalog. Do not add implicit
bootstrap exports of unsafe operations.

```text
runWorld  :: forall a.
  (State RealWorld -> Step RealWorld a) -> a
runRegion :: forall a.
  (forall s. State s -> Step s a) -> a
```

runWorld starts one dynamic execution, supplies its root dependency, invokes
the supplied function once and discharges its returned state before yielding
the payload. It is an unsafe execution boundary used by the library unsafe
runner and command adapter, not a purity proof. runRegion introduces a fresh
scoped region, invokes the rank-N body once and checks that the region does
not escape in the result. No arbitrary initial State literal is exposed.
Nested and repeated executions retain distinct dynamic invocation boundaries.

Mutation and reads of mutable storage use state-aware checked primitives, for
example a private cell operation has the schematic shape:

```text
readCell  :: forall s a. Cell s a -> State s -> Step s a
writeCell :: forall s a. Cell s a -> a -> State s -> Step s Unit
```

Cell is library owned. Actual storage bindings use their existing checked
storage operands; a primitive scheme may not presume the identity or layout of
Cell. New storage allocation, mutable reads, writes and unsafe freezing all
participate in the dependency protocol. Pure array operations retain their
existing pure contracts; reading mutable backing storage must not be classified
as pure merely because immutable array indexing is pure.

Generic execution primitive descriptors and checked target binding contracts
own dependency inputs/results, observable effects and trap behavior. Source
schemes, kind/role checks, Core elaboration and verifiers consume these contracts.
Backend binding metadata keeps runtime provider/export identity, source module,
span and the checked Core scheme identity beside target-neutral CC. Extraction
must retain every runtime binding; boundary validation rejects missing,
duplicated or substituted records. Keeping a raw binding record does not mean
its call has been projected or checked against a physical instruction sequence.
Returning an ordinary record containing
the same input State is not enough to implement an observable primitive: its
checked operation must establish a successor dependency.

## Design

### Runtime execution boundary

Keep Effect and Action as ordinary library newtypes. The existing single
`psrs-runtime` crate owns executable low-level providers, including GC storage
allocation, mutable reads/writes and unconditional termination. Library pure,
bind and loops remain ordinary source definitions; they do not become runtime
opcodes or compiler-synthesized functions. Host services retain their checked
WIT providers and library adapters.

The physical storage ABI passes a non-null mutable GC array of nullable eqref
payloads, signed Int indices/lengths and checked boxed values. Fill returns a
new array; read returns the stored reference; write mutates the original array
and returns no physical payload. Negative sizes and invalid indices trap.
Array identity and element reference identity survive the provider boundary.
The runtime owns a checked source/raw call projection contract. Each ordinary
operand pairs its source storage role with its declared physical ABI type;
result projection distinguishes a raw value, normal void-to-Unit adaptation,
and non-returning execution. Source/ABI arity, operand roles, payload types and
normal-return properties must agree before source checking or lowering consumes
the projection. This contract contains no compiler IR and does not itself
authorize State erasure, boxing, or invocation correspondence. A void write
produces no raw payload, and a trap produces neither a normal successor nor a
payload; lowering must represent those facts explicitly.
The target library may wrap that primitive as an ordinary suspended failure
action through its hidden newtype representation boundary. Constructing the
wrapper must not terminate or manufacture a State token. Executing it follows
the checked non-returning runtime contract; no Effect-specific intrinsic or
normal successor may be introduced to make the wrapper type-check.
At the CC boundary, the ordinary external signature must retain exactly one
final State parameter and a checked Step result. Its ordinary operand count and
payload role must agree with the runtime-owned projection. Array operands and
results require a non-null concrete array representation with canonical erased
element storage. Core checks nominal regions and source element equality before
CC selects that storage protocol. Source declarations may use concrete element
types. Only catalog Element operands and Element payloads adopt the shared
erased payload protocol; indices, arrays, Unit and non-returning results retain
their declared roles. Ordinary checked CC conversions box and recover scalars,
records and callable payloads before MIR layout planning. Callable payloads use
the same curry protocol on entry and recovery. Array operands retain the
original mutable storage reference, including nested array elements; adapting
a concrete source declaration must not copy it through an ArrayMap.
Signature validation does not establish call-site ordering or
authorize raw-call emission; checked instruction and CFG correspondence must
still account for operand conversion, payload recovery and termination.
Raw import signatures use the application's planned GC type arena. Array type
IDs come from that arena's concrete representation mapping; validating the
logical array shape alone is insufficient. Its target definition must be a
mutable array of nullable eqref. Raw element parameters and results are nullable
eqref even when the checked source payload is non-null or scalar. Normal void
and non-returning exports both have no raw result, but retain distinct result
projection contracts. Import signature planning does not replace their caller
adaptations or the checked provider identity at linking.
MIR raw runtime imports retain explicit provider/export identity alongside the
physical signature and ordinary call symbol. They do not acquire reserved
intrinsic identities. Provider/export lookup and storage-specific ABI checking
belong to external binding adaptation and target linking, not MIR semantics.
The binding adapter publishes a physical signature and a general result
convention (value, normal void-to-Unit, or non-returning). MIR verifies ordinary
call typing and immutable source-to-instruction correspondence, including the
exact checked import retained by each source invocation. It does not reconstruct
storage operations from provider names. The backend converts the application's actual GC
type arena to linker-owned closed reference contracts, retaining complete
recursion groups; provider selection compares those contracts with the offered
runtime unit. Substituting the catalog signature for the consumer's actual
types would hide ABI drift and is forbidden.
Invocation adaptation retains the checked source assignment as its anchor.
Raw operands must correspond to the original ordinary operand IDs, with explicit
checked conversions where the ABI type differs. A raw read's payload recovery
must consume that call's actual result. A normal void write must execute its
matching call before producing Unit; a Unit constant cannot stand in for the
operation. These per-invocation guarantees do not discharge whole-function
dependency and CFG correspondence or non-returning execution.
A checked non-returning ABI refines the source's abstract normal-return path at
its exact invocation. The immutable source assignment and checked import must
anchor that refinement. Emit the actual raw call followed by a bottom-value
projection and a terminating block, not Unit recovery. No ordinary continuation
instruction or normal successor may follow it. Subsequent source assignments on
that path become unreachable; sibling branches and their normal joins remain.
MIR correspondence must validate the refinement from the frozen return contract
rather than infer non-returning behavior from a function name or a void result.
The caller validates payloads at their checked use types before boxing/erasure;
the runtime cannot recover source types from eqref. An array of another physical
element type requires a representation-compatible storage design, not a copied
array passed to an in-place operation. This ABI does not require every ordinary
immutable array to use mutable storage's representation.

The GC provider is an independent core module encoded by the runtime package,
with its own type and function index spaces and no linear memory, private
globals, start function or world-token allocation. Application and provider GC
reference types must match structurally under the checked linking contract.
GC ABI metadata is separate from the existing scalar/UTF-8 artifact metadata;
do not disguise a GC reference as an i32 handle or advertise a scalar signature
for it. The linker must validate reference types and compose the actual provider
before a source storage call can be accepted as executable.

Storage is an ordinary explicit runtime binding, not a separate HIR intrinsic
for each export. For example:

```purescript
foreign import "psrs:runtime-storage#array_read" readArray
  :: forall s a. Array a -> Int -> State s -> Step s a
```

HIR retains provider/export identity and the source signature. Runtime metadata
owns physical signatures and source operand/payload relations; target linking
checks every declaration, including unused ones, before erasure. Checked state
signature evidence remains attached to the call conversion. Unknown providers,
exports and malformed contracts are errors. Do not manufacture primitive IDs
for storage operations or send runtime bindings through the WIT registry.

State parameters and successors belong to the logical compiler contract, not
the provider's physical ABI. Core/CC/MIR keep each storage or host call observable
and ordered; only normal completion produces its successor. Runtime termination
has no normal result or successor. Runtime code cannot repair a dependency that
was erased before optimization or a call that the compiler removed as pure.

runWorld and runRegion introduce and discharge generic compiler dependencies
around ordinary callable invocation. They need no runtime world object or
Effect-aware dispatcher. Executable helpers may live in the runtime when the
shared callable ABI requires them, but their representation must follow that
ABI. Do not force an arbitrary closure through the scalar runtime ABI.

An operation-node interpreter would require another action representation,
allocations and a runtime dispatch protocol. It is outside this function-newtype
design. Async scheduling and cancellation remain separate work rather than
implicit guarantees of these synchronous primitives.

### Ordinary library combinators and host adapters

```purescript
pureE value = Effect (\state -> { state: state, value: value })
bindE (Effect first) next = Effect (\state0 ->
  case first state0 of
    { state: state1, value: value } ->
      case next value of
        Effect second -> second state1)
```

pureE and bindE retain their official signatures and names. Operations, loops
and callbacks are ordinary source definitions; no pure/bind/loop intrinsic
is introduced. Official instances stay with their type in Effect. Prelude
keeps its official surface; check the resulting dependency graph for cycles.

Construction of an action is inert. Arguments already evaluated while creating
an action remain strict. On execution, bind completes the first action, invokes
the continuation, then executes the returned action with the returned state.
Loops feed the last iteration's state to the next, including condition and
body transitions. Mutable reads and writes obey the same sequence.

A private host binding has a state-aware source signature, for example:

```text
hostNow :: State RealWorld -> Step RealWorld Int
```

A target library action adapter wraps this function in Effect. Because the
public constructor is hidden, the defining module may provide a package-internal
adapter using the existing Unsafe.Coerce, specialized to the exact newtype field
type. This is an audited unsafe library implementation boundary. The safe
public exports stay unchanged; no additional compiler visibility mode or
foreignWrap intrinsic is introduced. If implemented as a helper module, its
unsafe status and selected callable representation must be explicit.

Effect.Unsafe.unsafePerformEffect similarly unwraps the selected representation
at this audited boundary and calls runWorld. It does not cast State to Int,
fabricate a token or bypass dependency verification. Common unsafe callable
transport must preserve the logical signature and execute correctly.

### Checked dependencies across representations

Typed Core preserves source State applications, roles and primitive contracts.
Its state-operation verifier checks input/output shapes, region identity,
producer provenance, normal-return state and dependency scope. Ordinary functions
are checked from their bodies; an external or abstract callable needs a checked
state signature contract. MIR retains checked source requirements in an immutable
module inventory independent of mutable per-function evidence. Every retained
function must satisfy its source requirement; deleting an optional witness must
not disable verification. Unused whole functions may be pruned without changing
the requirements of surviving functions. Missing required evidence is an error, not a fresh
state or assumed-pure operation.

Core body evidence is tied to its immutable source arena. It derives transitions
from actual invocation nodes and tracks their argument and returned-field
provenance through ordinary aliases, record patterns and control-flow joins.
Verification checks invocation coverage and order, region/payload contracts and
branch placement in addition to standalone graph validity. A proven pure body
may pass its incoming dependency through unchanged; an unknown invocation stays
conservatively observable. Pure joins preserve aliases already evaluated before
the join. Core transformations invalidate old body evidence and recheck the
resulting source representation before later projection can consume it.

CC makes the dependency chain explicit alongside ordinary evaluation order.
Each observable state operation defines an output dependency referring to its
input. Calls retain checked state inputs/results through dictionaries, erased
closures and higher-kinded instantiation. Conditional joins select the dependency
from the executed branch; loops carry it as a loop parameter. The returned
state must be downstream of the observable operations executed on that path.
Unknown calls and unsafe execution remain conservatively observable.

Verification does not claim a general linear source type system. It rejects
malformed chains, region mixing and an unsupported fork that would discard or
replay a predecessor around observable operations. Branch-exclusive uses and
pure state pass-through are valid. A library closure may execute repeatedly;
each invocation threads the state supplied for that invocation. Support for a
raw-state transport shape must be checked across its actual fields, captures,
products and calls; unsupported shapes fail explicitly rather than materializing
a placeholder token or erasing the dependency.

MIR fixes physical representation and keeps a checked dependency projection
for instructions, calls, block joins and loop edges. Dependency identities are
not Wasm value types or integer locals. Their definitions/uses are explicit MIR
structure, not a name-based side table reconstructed by the encoder. MIR
verification establishes dominance, region agreement, call contract consistency,
and control-flow preservation of the dependency chain.

The existing representation planner owns the source/logical-to-physical mapping.
State maps to zero payload slots; Step's state field contributes no storage.
At a known state-call boundary, the result projects to the payload and requires
no Step record allocation. Common aggregate and erased transport may use an
ordinary payload container when required; it must not lose logical dependency
evidence or silently change callable conventions. No full GHC unboxed-tuple
syntax or global allocation-free claim is introduced.

Physical parameter removal occurs only through checked signature projection,
shared by producers, consumers, partial applications and adapters. State
parameters are zero-width while their logical invocation and dependency remain.
Core owns call-boundary extraction for arrows, fixed-arity closures and quantified
results. The representation planner consumes that boundary and the checked State
signature, retaining its source arena and region while projecting only payload
types into physical slots. A signature plan alone cannot discharge invocation dependencies.
For ordinary direct and indirect calls, MIR correspondence preserves every
non-State operand identity, order and occurrence from immutable CC. It removes
only operands whose checked CC declarations have State shape. Changing an
operand to another value of the same physical type still invalidates the
certificate. ABI conversions require their own checked correspondence rather
than relaxing this ordinary-call invariant.
A nullary physical host call stays a call inside the action body, not a global
initializer. Immutable checked source Core is not mutated into a physical type
model; the lowering carries its explicit relation to CC/MIR.

### Optimization and final erasure

Core and MIR transformations preserve dependency edges, dynamic multiplicity,
branch/loop placement and observable call/trap behavior. Unused payloads do not
permit removal of their operation. State pass-through for a pure action is
simplifiable; a state-producing host/mutation operation is never replaced by
its input State. Inlining preserves argument order and reconnects dependencies
using fresh local identities. CSE, code motion, dead-code elimination and loop
optimizations require evidence covering both dependencies and observable effects.

Zero-width representation does not make a state operation pure. Observable-effect
metadata remains necessary after physical projection and for calls outside the
state protocol. Dependencies do not promise rollback, thread synchronization or
replay prevention for arbitrary unsafe source code.

After the last MIR optimizer, verify the full dependency graph and the ordered
instruction/CFG projection. The encoder may erase logical state edges only when
that projection discharges them. It preserves instruction/control order and
performs no subsequent effect-moving optimization. No State constant zero,
state counter, token object, dedicated world heap or runtime version comparison
is emitted. Verified state ordering survives as executable calls and control flow.

### WIT boundary and generic command entry

The ABI owner recognizes the generic checked state-binding shape using primitive
State identity and the structural Step contract. It projects the state parameter
to zero canonical arguments, flattens only ordinary host arguments/results,
and attaches the successor dependency to normal host return. Traps have no
normal successor. Dependencies cover actual host invocation and required ABI
reads/writes/cleanup so these cannot move across neighboring observable actions.
State is not a WIT resource or a canonical field.

Parameter/result checking validates the complete logical and physical mapping,
including aggregates, handles and return pointers. Wrong regions, wrong state
result, missing state, incompatible WIT shapes and missing dependency contracts
are explicit errors. Do not drop parameters until arities happen to match.
The earlier Unit-triggered ABI extension is no longer a prerequisite for Effect.
Existing ordinary raw bindings retain their checked ABI and conservative effects;
they cannot be inserted into a state action without an explicit checked adapter.

The package configures an ordinary runner such as
PSRS.Command.run :: Effect Unit -> Int. It invokes the unsafe library runner
once, returns zero on normal completion and propagates traps. The compiler
checks a configured monomorphic runner `T -> Int` against main's type T;
it does not inspect whether T is Effect. Load its module as an explicit root,
resolve stable identity once and build checked ordinary Core `runner main`
before pruning. Preserve Main.main precedence, unique-main fallback and Int
entries. Missing configuration, ambiguity and mismatches are explicit errors;
check-only compilation needs no executable runner.
Core entry normalization consumes resolved source identities and checked type
schemes, validates the monomorphic runner and selected entry, and constructs an
ordinary application. The generated declaration gets a fresh symbol in the
entry's source namespace and preserves its source range. Validation failure
must leave the input Core unchanged. Run this conversion before reachability
pruning so the runner, its dependencies and original main remain live. Source
loading and manifest selection belong to the driver; Core does not read package
configuration or rediscover a runner from library/type names.

The package manifest records the runner in
`compiler_contract.command_runner`, an object containing exactly `module` and
`function` string fields. Both names must be nonempty and have no surrounding
whitespace. The manifest participates in the package content fingerprint.
Executable compilation loads the runner module from the trusted inventory as
an explicit dependency root, together with its import closure. Resolution may
select only a source declaration in that trusted prefix. Normal compilation,
IR-report compilation and traced diagnosis use the same selection path.
Check-only compilation follows the user's import closure without adding the
executable runner root; it still validates the package manifest and inventory.

During package migration, absence of this field retains the locked package's
legacy entry path. A present but malformed or unresolved configuration is an
error and must never fall back to legacy handling. A configured runner bypasses
the old trusted Effect contract and lexical execution restrictions. This
compatibility mode is temporary; deletion of the legacy path remains part of
the Effect/ST migration.

### ST, source adaptation and rejected alternatives

PSRS.ST.Action migrates to State r -> Step r a with nominal region and
representational payload. Keep Cell's nominal region and existing rank-N
public run rule; discharge execution through runRegion. ST.Global.toEffect
uses the shared physical protocol only for the checked RealWorld region.
Validate the existing unsafe conversion and any explicit target adapter with
mutation and returned-value observations. Never coerce an arbitrary region
into RealWorld or let a region escape because State has no physical bits.

The user selected this direction on 2026-10-09. The independent package records
the exact upstream declaration, diff, Wasm representation rationale, preserved
public API and changed newtype/coercion semantics in its adaptation manifest.
Do not rewrite official pure definitions to compensate for compiler defects
or report exact opaque-declaration fidelity.

Rejected: Effect-specific type/instance rules; synthesized combinators;
name-based compiler recognition; inferred host suspension; a constant trigger
masquerading as a state dependency; a numeric world counter; ordinary record
threading without checked primitive successors; and premature state erasure.

## Algorithms

1. Keep a comparable compile diagnosis and inspect the owning-stage baseline.
2. Add shared primitive state declarations and checked operation/run contracts;
   test region, kind, signature and provenance rejection before publishing them.
3. Extend source-to-CC/MIR conversion with explicit logical dependencies and
   zero-width physical projection, including erased and higher-order calls.
4. Implement runtime-owned GC storage/termination providers and checked linking
   contracts. Implement state-aware host/storage binding plans and verify their
   operation successors, ABI effects, normal returns and traps.
5. Migrate Effect and ST library representations and operations together;
   preserve official exports, instances, signatures and ordinary pure definitions.
6. Implement generic runner loading/type checking/Core entry normalization.
   Remove old trusted Effect synthesis, suspension, context and token APIs.
7. Audit every optimizer and final encoding boundary, execute the acceptance
   cases below, then update the normal stdlib lock and content fingerprint.

## Code map

| Owner | Responsibility |
| --- | --- |
| Shared primitive catalog / source checker | State and RealWorld identities, nominal region roles, checked primitive/run schemes and rank-N scope. |
| Primitive registry / checked Core | Generic execution-boundary dependency/effect contracts; validated elaboration and source state signatures. |
| CC / common representation planner | Explicit logical state dependencies, callable/newtype transport and checked zero-width projection. |
| MIR / verification / optimizers | Dependency graph and CFG checks, transformation legality and ordered physical projection. |
| ABI planner / final encoder | Checked state-aware host mappings; ordered invocation and verified final dependency erasure. |
| psrs-stdlib Effect / PSRS.ST / target modules | Ordinary newtypes, combinators, loops, private host/storage adapters and unsafe execution. |
| psrs-runtime | Executable GC storage and termination providers, source operand/payload relations, physical operation contracts and reproducible module encoding; no Effect type or combinator synthesis. |
| Runtime linker / provider planner | GC reference ABI matching, provider resolution and module composition without linear-memory handles or array copies. |
| Driver package / entry modules | Generic configured runner and ordinary Core entry normalization. |

## Invariants and verification

| Requirement | Required evidence |
| --- | --- |
| Source/API adaptation | Upstream diff and package manifest; unchanged public signatures, exports, roles, instances and ordinary pure definitions. |
| Primitive state contract | Positive unrelated state-transformer library; reject fabricated roots, wrong regions, malformed schemes/results, missing successors and unrestricted unsafe primitive exposure. |
| Newtype and region safety | Hidden-constructor coercion rejection, payload coercions, rank-N escape negatives and nominal region coercion negatives before/after erasure. |
| Complete dependency chain | Core/CC/MIR fixtures for calls, joins, loops and normal returns; reject missing evidence, stale predecessor use and unsupported forks. Verify transactional failure and source origins. |
| Common conversion | Execute higher-kinded dictionaries, erased payloads, fields, captures, arrays, returned functions and nested actions with value assertions; unrelated newtypes use the same planner. |
| Suspension and order | JS oracle versus mandatory Wasmtime: inert construction, strict factory arguments, repeated execution, ignored payloads, continuation/callback order, loop state transitions and traps. |
| Optimizer integration | Compare operation traces and values across Core/MIR passes; unused results, inlining, CSE/code motion candidates, branch joins and loops must preserve dependencies and multiplicity. |
| Zero-width projection | Inspect checked MIR and emitted signatures: no token object/integer or state field; known Step returns need no record allocation. Physical nullary calls remain dynamically repeated. |
| Host/storage boundary | Repeated zero-host-argument calls, mutable read-after-write, fresh allocation, freeze alias behavior, aggregate/handle mappings and malformed-binding rejection. |
| Entry independence | Int entry, configured Effect entry and unrelated command type; helper unsafe execution, nested runners, selection errors and trap propagation. |
| Official composition / ST | Unchanged Effect.Uncurried checking; cross-module Semigroup/Monoid execution; ST.Global.toEffect mutation and repeat tests; region and missing-instance negatives. |
| Integrated validation | Format, full workspace tests, strict clippy, relevant official differential cases and comparable full-import diagnosis. Require Wasmtime for runtime acceptance. |

No requirement is verified by this design change. Import-only compilation is
compile acceptance, not runtime proof. Existing roadmap measurements stay unchanged.

## Worked example

Constructing a clock action captures its state-aware host adapter without running
it. The command runner invokes runWorld, which introduces the execution root s0.
Calling the first clock action consumes s0, performs its checked host call and
returns { state: s1, value: t1 }. bind invokes its continuation with t1 and
passes s1 to the next action. That action produces s2 and its payload.

CC and MIR preserve s0 -> s1 -> s2 and both calls' observable effects, even if
t1 or the final payload is unused. Verification establishes ordered calls in
the actual control flow. Final encoding emits those calls in that order, with
no token arguments at the host ABI and no runtime state storage. No pass
recognizes Effect's name or synthesizes bind.

## Boundaries and interfaces

The package supplies library code and runner configuration under its lock.
Primitive declarations and contracts own state semantics. Source typing owns
ordinary newtype and region evidence; dependency verification owns checked
ordering; representation lowering owns zero-width projection; ABI plans own
host invocation; final encoding consumes the verified projection. No later
consumer guesses a lost state fact from arity, type spelling or layout.

## Open questions and future work

Exact registry names, Rust storage and package configuration fields follow the
owner audit; their semantic contracts above are fixed. Dependency transport in
unsupported shapes must fail explicitly. Async effects, exception recovery and
fine-grained independent-state optimization require separate designs. Intermediate
work uses explicit development packages; normal locked consumption must not
retain scratch paths or partially migrated Effect/ST representations.

## References

- [Pinned PureScript Effect](https://github.com/purescript/purescript-effect/blob/a192ddb923027d426d6ea3d8deb030c9aa7c7dda/src/Effect.purs)
  and [JavaScript implementation](https://github.com/purescript/purescript-effect/blob/a192ddb923027d426d6ea3d8deb030c9aa7c7dda/src/Effect.js).
- [GHC IO implementation and runtime obligations](https://downloads.haskell.org/~ghc/latest/docs/libraries/ghc-internal-9.1401.0-555c/src/GHC.Internal.IO.html)
  and [State's ZeroBitType representation](https://hackage-content.haskell.org/package/base-4.20.2.0/docs/GHC-Base.html).
- [Polymorphism and erasure](polymorphism-and-erasure.md),
  [CC](cc-ir.md), [MIR optimization](../opt/mir.md),
  [canonical ABI](../wasm/canonical-abi-and-wit.md),
  [stdlib boundary](../../D-17-stdlib-and-conformance-boundaries.md).
