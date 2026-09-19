# D-03 — Type System: PureScript-Faithful Roadmap

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress — Phase 0 and Phase 1 implemented, Phase 2 started

## Purpose

Define how the compiler grows from its current monomorphic inference to the
PureScript type features needed to compile PureScript programs and, later, a
PureScript-style algebraic-effect library. [DEC-03](../decision/DEC-03-purescript-faithful-type-system.md)
fixes the direction: reproduce PureScript's type system and encode effects as a
library, not as a compiler-native effect system.

This document is the source of truth for the type representations in
`psrs-typecheck`, `psrs-thir`, and `psrs-core`, and for the order in which
features land. It does not change the pass boundaries in
[D-01](D-01-frontend-and-ir-boundaries.md).

## Scope

The roadmap reproduces PureScript type features only. The type system does not
implement first-class algebraic effects: no effect rows, no handlers, and no
delimited continuations. An algebraic-effect library is ordinary library code
written after these phases, following `purescript-run`; it is out of scope
here.

Host services are independent of this work. They move to the runtime ABI
registry and PureScript-facing library declarations described in
[D-02](D-02-wasm-lowering.md).

## Suite-driven milestones

[DEC-04](../decision/DEC-04-official-test-suite-roadmap.md) makes the official
PureScript test suite the compatibility target. The phases below map onto its
coverage layers, and progress is measured as per-file agreement with `purs`:

| Layer | Coverage | Type-system phase |
| --- | --- | --- |
| L1 parse | Full surface grammar parses | Phase 1 (types) and parser work |
| L2 resolve | Names, imports, exports resolve | Phase 2 constructors, modules |
| L3 kinds | Kind checking matches | Phase 2 |
| L4 types | Type checking matches | Phases 1–3 |
| L5 classes | Instance resolution matches | Phase 4 |
| L6 runtime | Passing programs produce expected results | Phases 3–6 and runtime |

The phases remain the implementation order; the suite decides what counts as
done at each layer. See the classifier and scoreboard harness for current
coverage.

## Current state

Phase 0 is implemented. Inference carries variable levels and type schemes,
generalizes local `let` groups and top-level strongly connected components, and
instantiates schemes at use sites. THIR and Typed Core `Type` have generic
variables, and declarations and `let` bindings carry their quantified
variables. `identity` and `let id = \x -> x in id id` type-check with
polymorphic types.

Phase 1 is implemented. Source syntax supports function arrows, `forall`, and
parenthesized types, plus a `name :: Type` signature immediately preceding its
value declaration. Types resolve in HIR to built-in constructors (`Int`,
`Boolean`, `String`, `Unit`) and type variables. The checker elaborates a
declared signature with rigid variables and unifies it with the inferred type,
so recursive uses see the declared type and a body that is too specific is
rejected with a source-spanned mismatch.

The backend does not yet erase types or pass dictionaries, so it rejects
declarations whose checked type contains quantified variables, with a
source-spanned diagnostic.

Phase 2 has started. A dedicated `psrs-kind` pass consumes resolved HIR and
infers and checks kinds for `data`, `newtype`, `type`, and `class`
declarations, including higher-kinded parameters, standalone kind signatures,
kind annotations, records and rows, and kind-polymorphic `forall` binders. It
unifies kinds with an occurs check and reports the official
`KindsDoNotUnify`, `InfiniteKind`, `PartiallyAppliedSynonym`,
`CycleInTypeSynonym`, `CycleInKindDeclaration`, and `UndefinedTypeVariable`
codes.

Steps 2.3, 2.5, and part of 2.6 are implemented. `InferType`, THIR, and Typed
Core carry type constructors and application; signature elaboration maps
`Array` and user type names to constructors, expands type synonyms by
substituting their parameters, and unification decomposes applications and
compares constructors by identity. A signature such as `Maybe Int ->
Maybe Int`, `Array a -> Array a`, or a synonym like `type Result = Array Int`
now elaborates and checks. The backend rejects aggregate and user-defined types
with a named limitation rather than lowering them.

The rest of 2.6 and later are not implemented: no saturation of built-in
constructors into runtime layouts, class constraints, or rank-N types.

Phase 3 has started on the type side. Data and newtype constructors are
registered as polymorphic values whose types are their field types followed by
the declared result type, so `Nothing :: Maybe Int` and `Just 1 :: Maybe Int`
type-check and a wrong constructor argument is rejected. Pattern matching,
constructor lowering to Core, and runtime layouts are not implemented, so the
backend still rejects aggregate types.

## Roadmap overview

| Phase | Theme | Outcome |
| --- | --- | --- |
| 0 | Foundations | Infer and represent rank-1 polymorphic types |
| 1 | Type syntax and signatures | User-written types; inferred and declared types are reconciled |
| 2 | Kinds and higher-kinded types | Type constructors and type-level application (`f a`) |
| 3 | Algebraic data types | `data`/`newtype`, constructors, pattern matching |
| 4 | Type classes | Classes, instances, constraints, dictionary evidence |
| 5 | Rows | Row kinds, records, variants |
| 6 | Advanced Polymorphism | Rank-N types, subsumption, functional dependencies |

Phases are ordered by dependency. Within a phase, steps are ordered and each
step should be independently testable.

---

## Phase 0 — Type-system foundations

Deliver rank-1 polymorphism and the representation support for it. Effect
inference stays monomorphic within a recursive group, as PureScript's does.

### 0.1 Type variables with identity and levels

- **Deliverable:** `InferType::Variable` carries a unique ID and a level;
  entering a binding increases the level.
- **Representation:** inference-private only; nothing crosses into THIR.
- **Acceptance:** unit tests that level adjustment and `occurs` traverse the
  updated type form.

### 0.2 Type schemes and instantiation

- **Deliverable:** a `Scheme` pairing quantified variables with an inference
  type; a use site instantiates fresh variables.
- **Representation:** `globals` and `locals` become scheme maps.
- **Acceptance:** a scheme used twice receives independent variable sets; a
  quantified variable can never be bound by unification.

### 0.3 Generalization of `let` groups

- **Deliverable:** infer each local `let` group monomorphically, then generalize
  variables not free in the enclosing environment before inferring the body.
- **Acceptance:** one `let`-bound function is used at `Int` and `Boolean` in the
  same body; recursive `let` groups still infer monomorphically.

### 0.4 SCC analysis and top-level generalization

- **Deliverable:** build the declaration dependency graph, order it into
  strongly connected components, infer each component monomorphically, and
  generalize it before later components.
- **Acceptance:** two top-level declarations use the same polymorphic
  definition at different types; mutually recursive definitions remain
  monomorphic within their component.

### 0.5 Generic type variables in THIR and Core

- **Deliverable:** THIR and Typed Core `Type` gain generic variables; a
  declaration carries its quantified variables together with its type.
- **Representation:** type-variable IDs are unique across the module so the flat
  interner stays sound; they are not per-scheme indices.
- **Acceptance:** `identity` and `compose` produce verifiable THIR/Core
  declarations whose types show distinct quantified variables.

### 0.6 Backend rejection with clear diagnostics

- **Deliverable:** the backend rejects declarations whose checked type contains
  quantified variables or unsupported constructors, with a source-oriented
  diagnostic.
- **Acceptance:** programs that type-check but cannot yet be lowered fail at the
  backend, not in the type checker, and the message names the construct.

---

## Phase 1 — Type syntax and signatures

PureScript programs annotate types. This phase adds the surface syntax and the
resolution path for types, which later phases require.

### 1.1 Type-expression syntax

- **Deliverable:** parse type expressions in CST: names, application, function
  arrows, and `forall`; retain source spans.
- **Acceptance:** parse and AST tests for nested applications and arrows.

### 1.2 Type-name resolution

- **Deliverable:** resolve type names to stable type-name IDs in HIR; reject
  duplicate or unknown type names.
- **Representation:** HIR gains a type namespace separate from values.
- **Acceptance:** forward type references resolve; unknown names produce
  source-spanned errors.

### 1.3 Signatures and checking

- **Deliverable:** declarations and binders may carry type signatures; the
  checker unifies inferred types with declared types and reports mismatches at
  the signature span.
- **Acceptance:** a correct signature is accepted; a wrong one reports a
  mismatch; a signature enables generalization from the declaration, not only
  from inference.

### 1.4 Rank-1 `forall` and scoped type variables

- **Deliverable:** `forall` at the outermost position of a signature; explicitly
  quantified variables are rigid during checking of that signature.
- **Acceptance:** `forall a. a -> a` accepts `\x -> x`; an attempt to unify `a`
  with `Int` inside the signature is rejected.

### 1.5 Kind annotations

- **Deliverable:** optional kind annotations on type names, checked once Phase 2
  exists.
- **Acceptance:** a wrong kind annotation is rejected; omission is inferred.

---

## Phase 2 — Kinds and higher-kinded types

Enable type constructors and type-level application, the basis for `Effect a`
and `f a`.

### 2.1 Kind representation

- **Deliverable:** kinds `Type`, `Type -> Kind`, and later a row kind; a kind
  table for type names and built-in constructors.
- **Acceptance:** kind pretty-printing and equality tests.

### 2.2 Type-constructor registry

- **Deliverable:** a registry of type constructors with declared kinds and
  arities; built-ins include function, `Effect`, `Array`, and `Maybe` (values
  for data constructors arrive in Phase 3).
- **Acceptance:** registry lookups report arity and kind.

### 2.3 Type-level application in inference

- **Deliverable:** `InferType` gains constructors and application; unification
  decomposes applications and unifies constructors by identity.
- **Acceptance:** `f a` unifies with `Maybe Int` and `Array String` at the right
  constructor; mismatched constructors are rejected.

### 2.4 Kind checking and arity diagnostics

- **Deliverable:** kind unification over type expressions; report arity mismatch
  and kind mismatch with source spans.
- **Acceptance:** `Maybe` used with two arguments is rejected; a `Type -> Type`
  constructor applied to a `Type` is accepted; a partially applied constructor
  is accepted only where a higher kind is expected.

### 2.5 THIR and Core constructor types

- **Deliverable:** THIR and Core `Type` gain constructors and application;
  interning and verifiers handle them.
- **Acceptance:** THIR/Core verification accepts and reports malformed type
  trees.

### 2.6 Saturating built-in constructors during lowering

- **Deliverable:** lowering resolves saturated built-in constructors used by
  values; kind-correct but unlifted types are rejected by the backend clearly.
- **Acceptance:** `Array Int` values either lower or fail with a named
  limitation; no inference rule depends on the backend.

---

## Phase 3 — Algebraic data types

Introduce user-defined data, the value side of Phase 2.

### 3.1 `data` and `newtype` declarations

- **Deliverable:** declaration syntax, name resolution, kind/arity checks,
  constructor registration with stable constructor IDs.
- **Acceptance:** recursive and mutually recursive types resolve; duplicate
  constructors are rejected.

### 3.2 Constructor typing and application

- **Deliverable:** constructors are values with function types; constructor
  application type-checks against the declared result type.
- **Acceptance:** wrong constructor argument types are rejected with spans.

### 3.3 Pattern matching

- **Deliverable:** `case` and binder patterns; type checking; basic
  exhaustiveness and redundancy diagnostics.
- **Acceptance:** non-exhaustive matches warn or error as specified; impossible
  patterns are reported.

### 3.4 THIR and Core cases

- **Deliverable:** THIR and Core gain case expressions and constructor
  application; verifiers check constructor arity and pattern coverage.
- **Acceptance:** THIR/Core verifier tests for arity and coverage.

### 3.5 Backend tagged layouts

- **Deliverable:** MIR lowers constructors to tagged layouts per
  [D-02](D-02-wasm-lowering.md); pattern matching lowers to tag tests and
  projections.
- **Acceptance:** execution tests compare observable results under a WASI
  runtime.

### 3.6 `newtype` erasure

- **Deliverable:** `newtype` constructors and coercions erase at runtime.
- **Acceptance:** a `newtype`-wrapped value behaves identically to its inner
  value.

---

## Phase 4 — Type classes

### 4.1 Class and instance declarations

- **Deliverable:** class and instance syntax, name resolution, kind checks on
  class parameters, and stable class/instance IDs.
- **Acceptance:** duplicate instances for the same head are rejected.

### 4.2 Constraints in schemes

- **Deliverable:** schemes carry constraints; constraint solving integrates with
  generalization and instantiation.
- **Acceptance:** `Eq a => a -> Boolean` types an equality use; unsolved
  constraints produce source-spanned errors.

### 4.3 Instance resolution

- **Deliverable:** resolve constraints against instances, including
  superclasses and recursive instance contexts.
- **Acceptance:** overlapping or missing instances are rejected with the
  offending constraint; recursive dictionaries terminate by construction.

### 4.4 Dictionary evidence in THIR and Core

- **Deliverable:** constraints become explicit dictionary parameters and values
  in THIR and Core, per [D-01](D-01-frontend-and-ir-boundaries.md); class methods
  become field selections.
- **Acceptance:** verifier checks dictionary arity and method indices.

### 4.5 Backend dictionary passing

- **Deliverable:** MIR lowers dictionaries to records and passes them as
  arguments; specialization is an optimization, not a requirement.
- **Acceptance:** execution tests for at least `Eq`, `Show`, and `Functor`.

---

## Phase 5 — Rows

### 5.1 Row kinds and row types

- **Deliverable:** a row kind, row types with a tail variable, and row
  unification.
- **Acceptance:** open and closed rows unify correctly; missing or extra labels
  are rejected with the label named.

### 5.2 Records

- **Deliverable:** record types, literals, access, update, and row-polymorphic
  accessors.
- **Acceptance:** a function polymorphic in the row extends a closed record.

### 5.3 Variants

- **Deliverable:** variant types and injections; the basis for `VariantF`-style
  effect unions.
- **Acceptance:** variant unification and exhaustiveness follow row rules.

### 5.4 Backend record and variant layout

- **Deliverable:** MIR fixes record layouts and variant representations.
- **Acceptance:** execution tests for record field access and update.

---

## Phase 6 — Advanced polymorphism

### 6.1 Rank-N types

- **Deliverable:** `forall` under function arrows; checking and subsumption for
  higher-rank arguments.
- **Acceptance:** a handler that abstracts over its result type type-checks;
  incorrectly ranked uses are rejected.

### 6.2 Rigid variables and subsumption

- **Deliverable:** skolemization for checking polymorphic arguments and a
  defined subsumption relation for instantiating them.
- **Acceptance:** soundness tests that a monomorphic argument cannot be used
  where a polymorphic one is required.

### 6.3 Functional dependencies and multi-parameter classes

- **Deliverable:** class parameters and functional dependencies, if libraries
  require them.
- **Acceptance:** dependency-driven improvement tests.

### 6.4 Interaction tests

- **Deliverable:** cross-feature tests for rows, classes, and rank-N together.
- **Acceptance:** representative PureScript library signatures type-check.

---

## After the roadmap

The type features above make a PureScript-style algebraic-effect library
possible. That library is ordinary PureScript-level code; it is not part of the
type system and needs no new compiler rules. Host services continue to use the
runtime ABI registry from [D-02](D-02-wasm-lowering.md).

## Dependency graph

```text
0.1 -> 0.2 -> 0.3 -> 0.4 -> 0.5 -> 0.6
0.5 -> 1.1 -> 1.2 -> 1.3 -> 1.4 -> 1.5
1.1 -> 2.1 -> 2.2 -> 2.3 -> 2.4 -> 2.5 -> 2.6
2.5 -> 3.1 -> 3.2 -> 3.3 -> 3.4 -> 3.5 -> 3.6
3.4 -> 4.1 -> 4.2 -> 4.3 -> 4.4 -> 4.5
2.5 -> 5.1 -> 5.2 -> 5.3 -> 5.4
4.5 -> 6.1 -> 6.2 -> 6.3 -> 6.4
```

## Cross-cutting representation contracts

### Inference types

`InferType` is private to `psrs-typecheck` and never crosses into THIR or lower
representations. It may carry levels, substitutions, and constraint worklists.

### Type schemes

A scheme pairs quantified type variables and constraints with an inference type.
Generalization and instantiation operate on schemes only. A quantified variable
must be instantiated before it participates in unification; unification never
binds a quantified variable directly.

### THIR and Core types

THIR and Typed Core `Type` gains generic variables, then constructors and
application. Declarations carry their quantified variables and constraints so a
polymorphic type is fully described. Type-variable IDs are unique across a
module so the flat type interner can deduplicate them soundly.

Kinds and inference state are checked before THIR construction and do not
appear in THIR or lower representations.

### Backend interaction

The backend learns nothing about inference. Until it implements type erasure and
dictionary passing, it rejects declarations whose checked type contains
quantified variables or unsupported constructors, with a source-oriented
diagnostic. Rejecting there is a bootstrap limitation and must not leak into the
type checker's rules.

## Invariants

- A pass consumes one representation and produces the next; no inference state
  or kind information enters a long-lived IR.
- Source spans remain on type errors and on the constructs that introduce
  binders, type names, and constraints.
- Generalization is sound: a variable is quantified only when it is not free in
  the enclosing environment and not mentioned in unsolved constraints.
- Recursive binding groups are inferred monomorphically.
- The Wasm emitter never special-cases a language symbol; it reads the runtime
  ABI registry.

## Validation

- Unit-test each step with accept and reject cases, including kind, arity, and
  constraint errors.
- Update the existing `UnconstrainedType` tests when 0.3 and 0.4 intentionally
  accept `identity`-style programs.
- Verify THIR and Core after every lowering, as they already do.
- Compare accepted source with the official `purs` compiler as coverage grows.
- Add execution tests under a WASI runtime for each backend-visible phase.
