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
Instance = { id, class_id, head, context, chain_id?, chain_position?, source }
Constraint = { class_id, kind_args, type_args, source }
Evidence = Given(LocalId) | Superclass(Evidence, FieldId)
         | Instance(InstanceId, [Evidence]) | Primitive(PrimitiveEvidence)
```

An instance chain is an ordered group of alternatives with its own selection rules. The class environment records imported and local instances with stable identities and visibility. Evidence terms are checked against the instantiated constraint they prove. Primitive classes such as `Prim.Row.Cons`, `Prim.Row.Union`, and `Prim.Row.Lacks` are solved by dedicated rules but present ordinary constraint interfaces to inference.

## Design

Check class parameter kinds, dependency indices, superclass cycles, method signatures, instance heads and contexts, and coherence conditions before solving uses. Build a searchable instance environment respecting module visibility and the official orphan and instance-chain rules. Search givens first, then superclass paths and candidate instances. Apply functional dependencies to improve unknowns; repeat until stable. Select an instance-chain branch according to source order and its apartness conditions, then solve its context recursively. Reject ambiguous alternatives and unresolved obligations with source-oriented diagnostics. Memoize and bound search to prevent cycles.

Elaboration turns a constrained binding into explicit evidence parameters and inserts evidence at overloaded uses. A method selection projects from its dictionary; a superclass selection follows a dictionary field. The frontend proves and records the selected path. Backend optimization may specialize dictionaries but cannot change which instance was selected.

Rejected alternatives: a global ban on overlapping heads would reject valid instance-chain programs; choosing the first ordinary candidate is incoherent; and postponing instance choice to runtime changes PureScript semantics.

## Algorithms

```text
solve(wanted, givens, instances):
    normalize wanted; improve unknowns using class fundeps and givens
    if matching given exists: return Given
    if a superclass path from a given proves wanted: return Superclass
    if wanted is a primitive relation: apply its checked solver
    collect visible candidate instances and chains
    select only a coherent candidate under PureScript chain/overlap rules
    recursively solve its context with cycle and work limits
    return Instance(candidate, context_evidence)
```

Generalization retains constraints permitted by the inferred scheme. Check that every remaining variable is determined by the result type and dependencies; otherwise report ambiguity. Keep constraint origins through improvement and search.

## Code map

`crates/psrs-typecheck/src/typecheck/classes/` owns `environment.rs` (`ClassEnv`, `InstanceEnv`), `validate.rs`, `fundeps.rs`, `entailment.rs`, and `evidence.rs`. Its key entry points are `check_class_declarations(&hir::Program, &CheckedKindEnv) -> Result<ClassEnv, Vec<Diagnostic>>` and `solve_constraint(&Constraint, &ClassEnv, &GivenEnv) -> Result<Evidence, Diagnostic>`. THIR owns the final evidence syntax and verifier; Core lowering converts it to dictionary parameters, applications, and projections.

## Invariants and verification

Every selected evidence term proves exactly its checked constraint; ordinary instance lookup is coherent; chain order and visibility are respected; improvement never assigns a rigid variable; and search terminates or reports a bounded cycle. Verify superclass paths, recursive contexts, fundep improvement, ambiguity, instance chains, overlap errors, and primitive constraints against official `purs` accept/reject cases.

## Worked example

For `class Convert a b | a -> b`, a wanted `Convert Int x` can improve `x` from the matching instance head. If `convert :: forall a b. Convert a b => a -> b`, a use at `Int` receives the selected dictionary as an explicit argument. A superclass method instead receives a projection from an available subclass dictionary.

## Boundaries and interfaces

P5 consumes resolved class and instance declarations plus kind-checked types. It emits THIR evidence or quantified constraints. [Core lowering](../semantics/core-lowering.md) makes evidence operational; the backend receives only verified dictionary values and calls.

## Open questions and future work

Track the official compiler's exact orphan, instance-chain apartness, and primitive-class rules as executable compatibility cases. Implementation coverage belongs in [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md).

## References

- [PureScript entailment](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Entailment.hs), [class desugaring](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Sugar/TypeClasses.hs), and [class environment](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Environment.hs).
- [Backend dictionaries](../../backend/fp/type-classes-and-dictionaries.md).
