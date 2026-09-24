# D-08 — Generic Values and Calls on Wasm

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Context

PureScript rank-1 polymorphism is resolved at use sites. The official compiler
instantiates `forall` variables while type checking and makes type-class
dictionaries explicit values before its runtime-oriented CoreFn is emitted.
That behavior is the semantic reference for this compiler.

Wasm has no generic function type. A `call_ref` instruction names one exact
function type, including every parameter and result representation. Therefore
mapping a source type variable directly to `eqref` is not sufficient: a
concrete closure with an `i32 -> i32` code signature cannot be passed directly
to a closure call that expects an `eqref -> eqref` code signature.

## Decision

Use a representation-directed erased ABI for genuinely polymorphic values,
with explicit adapters at higher-order boundaries. Keep concrete values
specialized whenever their source type is known.

### Value representation

- A type variable in a generic value position uses the erased representation
  requirement in CC. The current Wasm GC planner realizes that requirement as
  a non-null `eqref` in MIR.
- Concrete `Int`/`Boolean` values are boxed in the existing i32 GC box;
  `Number` values are boxed in the f64 GC box; concrete GC references are cast
  to `eqref` without another allocation.
- A consumer that knows the concrete type explicitly unboxes or casts the
  erased value. The erased protocol is typed by the surrounding Core
  expression, so it does not add a dynamic source-type tag.
- Parameterized ADT fields continue to follow
  [DEC-07](../decision/DEC-07-runtime-representation-for-parameterized-adts.md).

### Function representation

- A generic function uses an erased CC signature. For example,
  `forall a. a -> a` requires an erased argument and result. Under the current
  Wasm GC planner this becomes a code signature equivalent to
  `(ref closure, eqref) -> eqref` in MIR.
- A concrete function keeps a concrete CC representation requirement. The
  current Wasm GC planner may realize it as `(ref closure, i32) -> i32` or
  `(ref closure, f64) -> f64` in MIR.
- When a concrete function value is passed to a parameter whose source type is
  a polymorphic function type, CC creates a semantic adapter closure. The
  adapter has the erased signature, captures the original closure, adapts its
  erased argument, calls the concrete closure, and adapts the result back to
  the erased representation. P9 selects the physical box, cast, and call
  operations.
- The reverse direction is recorded at a concrete consumer as a semantic
  adaptation from an erased function result. P9 realizes it as the concrete
  closure conversion and indirect call required by the selected planner.
- Distinct Core function type IDs that have the same erased requirement share
  a canonical CC `SignatureId`. P9 interns equal concrete Wasm signatures and
  allocates their MIR type IDs; nominally different but structurally equal
  source types must not create incompatible `ref.func`/`call_ref` pairs.
- Every concrete indirect call is verified against the code signature selected
  by the closure representation. No verifier relaxation may treat two
  different function signatures as compatible merely because their values are
  references.

### Type-class dictionaries

Type-class evidence is an ordinary explicit value at the Typed Core boundary,
following the official PureScript dictionary-passing model. Its runtime
representation is selected by the normal aggregate and closure rules; it is
not encoded as a special Wasm type-system feature.

## Lowering responsibilities

Typed Core retains source type IDs and instantiation results. CC is the first
stage that records erased versus concrete representation requirements and
inserts semantic representation-adaptation and function-adapter operations. P9
chooses whether those become boxes, unboxes, casts, or no-ops. MIR contains
only concrete Wasm value types and typed references to its concrete function
types. The Wasm emitter only emits those already-verified operations.

The backend currently verifies and executes an erased identity fixture for
`Int`, `Number`, and a concrete reference through the GC planner. This exercises
scalar box allocation/projection and reference adaptation without adding runtime
source-type tags. End-to-end
Core integration and the remaining higher-order acceptance cases are still
tracked by the feature matrix.

The adapter boundary must preserve evaluation order: the original function
value is evaluated once, then captured; each erased argument is unboxed only
when the adapter is invoked; the concrete result is boxed before returning.

## Acceptance tests

The implementation must cover at least:

1. `identity :: forall a. a -> a` called with `Int`, `Number`, and a GC
   reference.
2. A generic function called directly with a parameterized ADT value.
3. A concrete lambda passed to a generic higher-order function and invoked by
   `call_ref`.
4. A polymorphic function value returned from another generic function and
   then invoked at a concrete type.
5. A dictionary-passing call once type classes reach the backend.

Each case requires CC/MIR verification and Wasmtime execution. A local
regression is not enough to mark the corresponding backend matrix row
`Implemented`; the official L6/M7 gate remains required by DEC-04.

## Alternatives rejected

- **Blind `Type::Variable -> eqref`:** fails for higher-order calls because
  Wasm function references carry exact, invariant function types.
- **Whole-program monomorphization as the only representation:** provides good
  scalar performance but makes closures, separate modules, and values whose
  instantiation is not visible at the current boundary depend on a global
  specialization pass. It can be added later as an optimization of this
  erased fallback.
- **Dynamic source-type tags for every erased value:** adds runtime overhead
  and is unnecessary for type-safe Core; source type information already
  determines the unboxing operation at each consumer.
