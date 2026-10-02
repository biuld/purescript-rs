# Classes, Constraints, and Evidence

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [type inference](type-inference.md), [kinds](kinds.md), and PureScript classes and instances.

**Summary:** P5 checks multi-parameter classes, functional dependencies, superclasses, and instance declarations, then solves wanted constraints under PureScript's instance rules. Selected evidence becomes explicit in THIR; the backend later represents it as ordinary dictionary values.

## Scope

This document owns class and instance validation, constraint entailment, functional-dependency improvement, ambiguity checks, and evidence elaboration. [Backend dictionaries](../../backend/fp/type-classes-and-dictionaries.md) own runtime representation; [rows](rows-and-records.md) owns row primitive semantics.

## Background

A constraint `C a b` requires a dictionary. A class can have several parameters, dependencies such as `a -> b`, methods, and superclasses. A given dictionary, a superclass projection, an eligible instance, or a compiler-supported primitive relation can discharge a wanted constraint. Instance chains have ordered alternatives; ordinary competing instances must remain coherent. Functional dependencies improve unknown types and affect ambiguity, rather than serving as runtime fields.

## Model

```text
Class = { id, kind_parameters, type_parameters, fundeps,
          superclasses, methods, covering_sets }
Instance = { id, class_id, head, context, chain_id, chain_position, source }
Constraint = { class_id, kind_args, type_args, source }
Evidence = Given(LocalId) | Superclass(Evidence, FieldId)
         | Instance(InstanceId, [Evidence])
         | Coercible(SourceType, TargetType) | Primitive(PrimitiveEvidence)
```

An instance chain is a contiguous, ordered group of alternatives in its
declaring module. `chain_id` is unique within that module and local
`chain_position` is contiguous from zero. Export filtering may leave gaps in a
visible subset, so surviving branches retain their original positions and
identity. Ordinary instances are singleton groups. The class environment
records imported and local instances with stable identities and visibility.
Evidence terms are checked against the instantiated constraint they prove.
Primitive classes such as `Prim.Row.Cons`, `Prim.Row.Union`, and
`Prim.Row.Lacks` are solved by dedicated rules but present ordinary constraint
interfaces to inference. `Prim.Coerce.Coercible source target` is
compiler-owned: users cannot provide instances for it, and its evidence is a
checked proof of a representation relation rather than a runtime dictionary
selected by ordinary instance search.

Instance bodies contain dictionary members, not module value declarations. A
member's optional type signature must immediately precede its first equation;
the signature applies to that consecutive equation group. Consecutive equations
for one member form one definition, while a later separated group with the
same name is a duplicate declaration. A signature without its matching member
is an orphan type declaration. Member signatures are checked against the class
method after substituting the instance head, and their unbound type variables
resolve in the instance-head scope. The implementation supports the existing
subsumption rules for these annotations; it does not yet solve a constrained
annotation merely to specialize it to a monomorphic expected method type.

## Design

Class bodies declare method signatures only; implementations belong in
instances. Source default implementations are not part of PureScript syntax.
A backend fixture placing a default closure in a dictionary does not add a
source-language feature.

Compiler-supported deriving rules are selected by resolved class identity,
including the declaring module; re-exporting a class does not change its
identity, and an unrelated user class with the same short name does not gain
the rule. Structural rules traverse the declared type's normalized field
types. `derive newtype` delegates to the wrapped class dictionary and checked
coercion boundaries instead of generating per-class wrappers. Every generated
method and underlying dictionary obligation goes through ordinary instance
checking and evidence selection, including for classes with no methods.

`Coercible` consults the role vector in the checked kind environment. Equal
types are reflexive. Matching constructors decompose arguments by role: nominal
arguments must be equal, representational arguments require recursive evidence,
and phantom arguments require none. A newtype may be unwrapped only when its
constructor is visible in the current module; nested visible newtypes may be
unwrapped recursively. Foreign data is nominal unless an explicit trusted role
signature says otherwise. Given `Coercible a b` constraints may justify the
same relation in either direction. User instance declarations for the compiler
class are rejected.

The source value `Safe.Coerce.coerce` elaborates to a typed coercion expression
carrying `Coercible` evidence and explicit source/target types. THIR verifies
that the proof class and its boundary match the value and result. Core records
that boundary as `RepresentationCast`; P8 must implement it with the existing
typed representation-conversion protocol. The proof authorizes a type-level
relation, not a raw Wasm cast: array elements, functions, records, and ADT
payloads follow their established conversion plans, and unsupported conversion
shapes fail lowering.

Type-level `Symbol` values use the same Unicode scalar sequence as source
strings ([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
`IsSymbol` evidence and `Reflectable` preserve or produce that sequence.
`SymbolCompare` orders by Unicode scalar value, and `SymbolAppend` concatenates
scalar sequences. There is no lone-surrogate `Symbol`: an unpaired surrogate
literal is rejected before solving. `SymbolCons` splits or builds one scalar
head and a scalar tail. These solver rules do not change value-level string
storage.

Check class parameter kinds, dependency indices, superclass cycles, method signatures, instance heads and contexts, and coherence conditions before solving uses. Build a searchable instance environment respecting module visibility and the official orphan and instance-chain rules. Search givens first, then superclass paths and candidate instances. Matching a given unifies flexible wanted arguments with the given's arguments transactionally; it never assigns a rigid given variable, and a failed candidate leaves no substitutions behind. Apply functional dependencies to improve unknowns using only the selected branch in each chain; repeat until stable. Compare every class argument in an instance head. Functional dependencies contribute the transitive closure of already matched positions, while arguments outside that closure can still prove a candidate apart. Within each visible chain, continue only when a branch is provably apart. A matching branch commits before its context is solved. An unknown non-final branch blocks later alternatives in that chain; unknown singleton and final branches are ignored. Unknown branches do not create an overlap with one definite match from an unrelated chain. Failure to solve a selected context does not fall through. Unrelated ordinary candidates must remain coherent; overlapping or unresolved obligations receive source-oriented diagnostics. Memoize and bound search to prevent cycles.

Elaboration turns a constrained binding into explicit evidence parameters and inserts evidence at overloaded uses. A method selection projects from its dictionary; a superclass selection follows a dictionary field. The frontend proves and records the selected path. Backend optimization may specialize dictionaries but cannot change which instance was selected.

Rejected alternatives: a global ban on overlapping heads would reject valid instance-chain programs; choosing the first ordinary candidate is incoherent; and postponing instance choice to runtime changes PureScript semantics.

## Algorithms

```text
solve(wanted, givens, instances):
    normalize wanted; improve unknowns using class fundeps and givens
    if a given can unify with wanted without changing rigid variables: return Given
    if a superclass path from a given proves wanted: return Superclass
    if wanted is Coercible: prove it from checked roles, equalities, givens,
        visible newtype constructors, and structural row rules; emit Coercible
        evidence or report an unsatisfied constraint
    if wanted is another primitive relation: apply its checked solver
    collect visible candidate instances and chains
    for each chain in stable module and chain order:
        continue after a branch only when its head is Apart from wanted
        stop at Match and commit to that branch
        stop at Unknown; block later branches only when this is non-final
    select the unique definite match even if another chain is unresolved
    diagnose multiple definite unrelated matches under coherence rules
    if no definite match exists, report no instance with chain ambiguity hints
    freshen the selected instance variables and unify every head position
    recursively solve the selected branch context with cycle and work limits
    do not try another branch if context solving fails
    return Instance(candidate, context_evidence)
```

Head comparison distinguishes `Match`, `Apart`, and `Unknown`. Repeated head
variables compare all wanted types they capture after dependency coverage is
known. A structural occurs conflict such as `a` against `Array a` is Apart;
distinct unresolved variables remain Unknown. Functional dependencies compute
the transitive closure of matched positions, so a branch can improve
determined positions only when its determining positions identify that branch.

Generalization retains constraints permitted by the inferred scheme. Check that every remaining variable is determined by the result type and dependencies; otherwise report ambiguity. Keep constraint origins through improvement and search.

## Code map

The frontend design is organized around these contracts:

- `ClassEnv` owns checked class parameters, superclasses, methods, and
  functional dependencies. `InstanceEnv` owns visible instance identities,
  module visibility, and ordered chain membership.
- `match_instance_head(head, wanted, fundeps) -> Match | Apart | Unknown`
  compares all class arguments, applies transitive fundep coverage, and checks
  repeated-variable substitutions. `select_instance_groups(wanted)` processes
  each ordered chain independently and returns definite candidates plus any
  ambiguity hints for the no-match diagnostic.
- `improve_constraints(wanted, givens, selected_instances)` reaches a fixed
  point without using a later fallback or assigning rigid variables.
  `solve_constraint(wanted, givens, InstanceEnv) -> Evidence | Diagnostic`
  commits the selected branch, freshens and unifies its complete head, then
  solves its context without fallback.
- `validate_coherence(InstanceEnv)` checks visible ordinary instances and
  chain boundaries. `elaborate_evidence(Constraint, Evidence) -> TypedCore`
  emits checked dictionary parameters, applications, and superclass
  projections; the Typed Core verifier checks the evidence boundary.
- `solve_coercible(source, target, CheckedKindEnv, Visibility) -> CoercibleProof`
  is a dedicated solver and never consults user instances. `coerce(value,
  proof) -> TypedExpr` records source and target types in the checked expression;
  its evidence remains explicit until Core lowers the proof to a
  `RepresentationCast` boundary.

AST lowering assigns module-local chain identity and source position; name
resolution carries those fields into HIR, whose verifier enforces contiguous
ordered branches and stable class identity. These are implementation
responsibilities for the contracts above, not additional semantic owners.

## Invariants and verification

Every selected evidence term proves exactly its checked constraint; ordinary instance lookup is coherent; chain order and visibility are respected; an unknown non-final branch blocks only its own chain; a unique definite match is not turned into an overlap by an unresolved chain; context failure cannot change a selected branch; improvement never assigns a rigid variable; and search terminates or reports a bounded cycle. A `Coercible` proof is generated only from a checked role, equality, given, or visible newtype decomposition; it cannot be forged by an instance, and THIR rejects a mismatched evidence boundary. Verify superclass paths, recursive contexts, fundep closure and independent-argument apartness, repeated-variable occurs checks, ambiguity, instance chains, overlap errors, and primitive constraints against official `purs` accept/reject cases.

## Worked example

For `class Convert a b | a -> b`, a wanted `Convert Int x` can improve `x` from the matching instance head. If `convert :: forall a b. Convert a b => a -> b`, a use at `Int` receives the selected dictionary as an explicit argument. A superclass method instead receives a projection from an available subclass dictionary. For `newtype Age = Age Int`, `coerce :: Age -> Int` is accepted while its constructor is visible; the resulting checked representation boundary reaches the backend conversion planner.

## Boundaries and interfaces

P5 consumes resolved class and instance declarations plus the checked kind-and-role environment. It emits THIR dictionary evidence or a `Coercible` proof boundary. [Core lowering](../semantics/core-lowering.md) makes dictionary evidence operational and preserves coercion source/target types; the backend receives only verified values and proof-authorized conversions. [Polymorphism and erasure](../../backend/fp/polymorphism-and-erasure.md) owns the concrete adaptation plan.

## Open questions and future work

Track the official compiler's exact orphan, instance-chain apartness, and primitive-class rules as executable compatibility cases. The current source subset covers role-aware higher-kinded given rewriting, checked kind compatibility, canonical open-row alignment, structural `Eq`/`Ord`, covariant `Functor.map`, `Bifunctor.bimap`, `Contravariant.cmap` through `Profunctor.lcmap`, and `derive newtype`. Class method `forall` signatures, quantified method parameters, and method-local constraints are checked with independent instantiation and scoped dictionary evidence; their verification is tracked in the [rank-N acceptance record](../../../implementation/frontend/rank-n.md). The remaining class-specific deriving traversals remain open. The function-based `Contravariant` case still reaches a backend closure-capture limit, and open-row runtime conversion remains outside the current CC layout. Implementation coverage belongs in [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md) and the [roles and coercions acceptance record](../../../implementation/frontend/roles-and-coercions.md).

## References

- [PureScript entailment](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Entailment.hs), [class desugaring](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Sugar/TypeClasses.hs), and [class environment](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Environment.hs).
- [PureScript `Coercible` entailment](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Entailment/Coercible.hs) and [safe-coerce source API](https://github.com/purescript/purescript-safe-coerce).
- [Backend dictionaries](../../backend/fp/type-classes-and-dictionaries.md).

## Implementation notes

The current bounded coercion solver is a dedicated checker helper, while the
design target exposes it as a separate `solve_coercible` service. Implemented
source cases cover higher-kinded application-head rewrites, kind compatibility,
role-aware canonical-given interactions, and aligned open rows; open-row values
still lack a runtime layout. Structural `Eq`/`Ord`, nested `Functor.map`,
`Bifunctor.bimap`, and checked newtype-derived methods execute for the covered
method signatures. Function-result mapping and `Contravariant` through a
profunctor dictionary match the upstream source rules; the function-based
Contravariant case still lacks Wasmtime evidence because of closure capture.
Method-local constraints and the remaining upstream deriving classes keep
FE-16 partial.
