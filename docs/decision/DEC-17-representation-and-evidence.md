# DEC-17 — Unified Runtime Representation and Checked-Boundary Model

**Status:** Proposed
**Date:** 2026-10-05

## Context and constraints

A source type does not determine a runtime representation. Representation is
representation-polymorphic wherever a type variable, an abstract constructor, a
class dictionary, an effect, or a newtype is involved. The frontend already
produces explicit evidence for these cases — class dictionaries and selected
instances, `Coercible` proofs, instantiation and subsumption — and the backend
already owns layout ([CC IR](../design/backend/fp/cc-ir.md),
[MIR](../design/backend/fp/mir.md)). What was missing is a single contract for
how the two meet.

Without that contract, the abstract-constructor transport work grew private,
per-feature mechanisms: a HIR-`TypeId`-keyed constructor-protocol table inside
CC, a callable-protocol signature derived by enumerating signatures and taking
parameter prefixes, an effect token lowered to the constant `i32` `0`, and a
driver that skips the compiler-provided `Safe.Coerce` module to avoid a
recursive vendored body. Each is a local rule where a shared one is required,
each keys a representation decision on information its owner does not have, and
together they make erasure, dictionaries, effects, coercion, and partial
application agree only by coincidence.

Established practice treats these as one problem. GHC represents a value of kind
`TYPE r` through its `RuntimeRep` `r`, carries that representation at a coercion
site, uses `Any` as the uniform boxed inhabitant, represents `IO` as a state
token, and exposes `unsafeCoerce#` as the primitive representation coercion.
Swift gives a function value a calling convention and reintroduces a changed one
with a reabstraction thunk, and represents dictionaries as witness tables. Java
erases generic type arguments and restores call compatibility with bridge
methods. Koka elaborates class and effect operations to explicit evidence. The
constraint is to adopt that one model instead of another feature-local rule,
without adding a runtime type tag and without putting a source or HIR identity
into the CC representation.

## Decision

Adopt one runtime representation model, one checked-boundary contract, and one
conversion planner, shared by every feature:

- **RuntimeRep** (`Scalar`, `Box`, `Reference`, `Aggregate`, `Closure`,
  `Erased`) is the target-neutral representation a value has at a boundary; its
  `Layout` is the target-specific realization. `Erased` is the uniform
  anyref slot, not a loss of information.
- **RepresentationPolicy** is the `RuntimeRep` a producer chose when the value
  entered an `Erased` slot (`Boxed`, `Nominal`, `AggregateLayout`, `Callable`).
  It is compiler evidence, not a runtime tag.
- **Boundary**, **Evidence** (instantiation, subsumption, dictionary selection,
  coercion proof, effect plan), and **ConversionPlan** describe the crossing.
- **Adaptation** (`Identity`, `Box`, `Unbox`, `Cast`, `AggregateConvert`,
  `Thunk`, `Coerce`) is planned from the evidence and the two representations;
  it is never searched for.
- A **representation owner** maps a source constructor plus its checked
  instantiation arguments to a policy. There is one registry, not a special case
  per feature.

Four facts have four owners: checking owns the source relation (P5/P6); the
producer or representation owner owns the policy; the checked use owns the
consumer requirement; P9 owns the layout. The model has two levels: the
`RuntimeRep` of a value at a boundary is target-neutral, and its `Layout` is
the target-specific realization, joined by a total mapping per target
(`heap_layout` for the Wasm GC language heap, `abi_layout` for the Canonical ABI
boundary). A `RuntimeRep` a target uses must have a layout, so the target-neutral
model plus a total, verified mapping is what makes the backend complete.
`RuntimeRep` is this compiler's analogue of GHC's `RuntimeRep`; source-level
representation polymorphism (`TYPE r`) is not a PureScript feature, so it is an
internal descriptor, not a kind.
Erasure, canonical
aggregates, callables, dictionaries, effects, coercion, and partial application
are instances of this model, specified in
[representation and evidence](../design/backend/fp/representation-and-evidence.md),
not separate mechanisms.

Checked evidence and representation policies travel to P8 through an explicit
Core-to-CC side table, the same boundary discipline `ExternalBindings` uses for
WIT bindings. P8 must not read the Core type arena ambiently, must not key a
semantic decision on a HIR identity, and must not reconstruct a producer policy
from a consumer type or a declaration arity. Missing evidence is a reported
unsupported boundary.

The model adopts the established naming: GHC `RuntimeRep`/`Any`/`unsafeCoerce#`,
Swift calling conventions and reabstraction thunks with witness tables, Java
erasure with bridge methods, and Koka-style explicit evidence.

## Consequences

- The transitional mechanisms are removed, not extended: the HIR-keyed protocol
  table, the signature-prefix protocol derivation, and the effect token's
  special case are now a single Core-to-CC side table plus the one planner.
- A new constructor is added by registering one representation owner, not by
  teaching each pass a new rule.
- `Safe.Coerce.coerce` and `Unsafe.Coerce.unsafeCoerce` are compiler-provided
  primitive values. A module the compiler provides is never shadowed by a
  vendored on-disk file, so the vendored source stays faithful to upstream.
- The effect token is the opaque, compiler-owned `State# RealWorld` analogue; the
  earlier `i32` `0` placeholder is gone.
- Generic aggregate normalization and the aggregate conversion plan
  (`ProductMap`/`ArrayMap`, canonical aggregate keys) are sections of the model
  document, not a separate design; the concrete aggregate layouts and their
  lowering stay in [data representation](../design/backend/fp/data-representation.md).
- The cost is a longer-lived boundary artifact: checked evidence and policies
  must be preserved until P8 emits conversion operations, and the side table is
  validated in both directions like `ExternalBindings`. That cost is accepted
  because it is the only way a consumer can recover a value without guessing.
- CC remains target-neutral and free of source identity; MIR keeps sole
  ownership of concrete layout.

## Rejected alternatives

- **Keep the per-feature rules (HIR-keyed protocol table and signature-prefix
  derivation).** Rejected: they key a representation decision on an identity
  owned by another stage, they give the same constructor two mechanisms, and
  they cannot be extended without another local rule.
- **Recompute the checked relation inside the backend instead of carrying it.**
  Rejected: it duplicates the checker's subsumption without preserving its
  direction, scope, or substitutions, and it makes acceptance depend on the
  backend's copy.
- **Runtime type tags or type passing.** Rejected: the typed core performs no
  runtime type analysis, so a tag store and load on every polymorphic boundary
  buys nothing; dictionaries already carry the operations a class needs
  ([DEC-15](DEC-15-unified-type-representation.md),
  [polymorphism and erasure](../design/backend/fp/polymorphism-and-erasure.md)).
- **A dedicated IR node or runtime object per feature (dictionary, effect,
  coercion).** Rejected: dictionaries and effects are ordinary products and
  closures, and a dedicated node couples the backend to one library and adds a
  verifier path for no semantic gain.
- **Keying representation behavior on a source or HIR identity inside CC.**
  Rejected: representation decisions belong to stable representation indices;
  a HIR identity is not part of the CC contract.
- **Treating representation equality as nominal identity.** Rejected:
  structurally equal signatures must share one `SignatureId`, or a `ref.func`
  for one source type is not callable through a shared `call_ref`.
