# Typed Core Optimization

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [functional core](../../frontend/semantics/functional-core.md),
[IR boundaries](../00-ir-boundaries.md), and the basics of call-by-value
semantics, substitution, and rank-1 polymorphism.

**Summary:** P7 transforms verified Typed Core into verified Typed Core before
ANF and closure conversion. It simplifies and specializes while source types,
constructor identities, dictionary evidence, and lexical scopes are available.
The pass preserves observable call order and keeps the unspecialized path for
polymorphic or separately compiled uses.

## Scope

This document owns the legal P7 transformations, their profitability limits,
the effect-aware substitution rule, and P7 verification. It does not choose
runtime layouts ([MIR](../fp/mir.md)), perform closure conversion
([CC IR](../fp/cc-ir.md)), or structure Wasm control flow
([control flow](../fp/control-flow-and-tail-calls.md)).

## Background

Substituting a value into a call-by-value expression can duplicate or remove
the computation that produced it. Beta reduction is therefore safe only after
accounting for the evaluation of the callee and argument. A dictionary is an
ordinary Core record of methods, so a known dictionary can expose a known
method without introducing a runtime type test. Rank-1 polymorphic functions
retain their generic implementation because a call site need not know all
other instantiations.

## Model

```text
CorePass = Module -> Result<Module, Vec<VerifyError>>
EffectSummary = { may_call: bool, may_trap: bool }
Cost = { nodes, duplication, recursion_depth }
```

An expression is *inert* when both flags are false. `may_call` includes every
direct or closure call without a trusted purity proof,
including imported calls and calls through dictionaries. `may_trap` covers
partial primitives, failed casts, bounds checks, and any other trapping term.
The summary is conservative: unknown sets both flags. Allocation may
be removed only when it cannot be observed through identity or finalization;
the Core language has neither feature for ordinary immutable products.

P7 keeps each declaration's checked type, quantified variables, stable symbol,
and source range. New expressions carry the span of the operation they replace;
cloned expressions retain an origin span and receive a distinct local identity.

## Design

The default pass order is: local simplification, bounded inlining, known
dictionary projection, then dead-binding elimination. Each pass reads and
writes Core and runs the Core verifier. A fixed iteration limit permits
newly exposed simplifications without creating an optimizer that can diverge.

- Fold total scalar operations only when the result exactly matches the source
  semantics, including `NaN`, signed zero, 32-bit wrapping, and character
  validity. Do not fold a primitive whose execution would trap.
- Reduce `if` with a known Boolean and `case` with a known constructor or
  record only when the selected pattern is decidable without a runtime cast.
  Preserve the first-match rule and required evaluation of the condition or
  scrutinee.
- Inline a small nonrecursive body by capture-avoiding substitution. Bind
  arguments once, in source order, before substituting their values.
- Project a method from a statically known dictionary when doing so preserves
  construction and call behavior. Keep the generic dictionary-passing body.
- Remove a dead binding only when its initializer is inert; retain an
  initializer with an observable call or possible trap.

P7 may create a specialized Core declaration for a concrete call site, with a
size and count budget. It never makes specialization necessary for correctness,
changes a public declaration's type, or relies on target layout. Erased
polymorphism and adapters remain the fallback
([polymorphism](../fp/polymorphism-and-erasure.md)).

Rejected alternatives: unrestricted beta reduction can duplicate effects;
whole-program monomorphization makes separately compiled and unknown uses
unsound; and source-type-aware optimization in MIR would require restoring
source semantics after erasure.

## Algorithms

```text
optimize_core(module, budget):
    verify_core(module)
    repeat at most budget.iterations:
        next = simplify(module)
        next = inline_small(next, budget)
        next = project_known_dictionaries(next)
        next = eliminate_dead_inert_bindings(next)
        verify_core(next)
        if next is structurally unchanged: break
        module = next
    return module

inline_call(callee, arguments):
    evaluate callee and arguments once, left to right, into fresh bindings
    alpha_rename the body and substitute only those bound values
    keep the call if the body is recursive or exceeds the budget
```

The local simplifier folds a `case` only when its scrutinee is a known
constructor or record value and every earlier branch is either disproved by
that known outer shape or can be matched without testing a nested erased
field. An unknown local or global scrutinee always remains a case, even when a
later wildcard branch exists.

External declarations and binding metadata are validated independently of
reachability; P7 cannot hide a malformed declaration by dropping its last use.
The final binding side table is checked at the P8 boundary.

## Code map

`crates/psrs-core/src/opt/` owns P7. `mod.rs` defines
`optimize(module: Module, budget: Budget) -> Result<Module, Vec<VerifyError>>` and
the ordered pass driver. `effects.rs` computes conservative evaluation
summaries. `simplify.rs` owns constants, branch reduction, and statically
known record and dictionary projections; `inline/` owns bounded lambda and
named-global inlining. Its local reducer and global call-graph analysis are
separate from capture-avoiding cloning and call-site construction.
`specialize/` owns the bounded specialization cache, call redirection, and
structural type substitution. `dead.rs` owns dead-binding analysis; `util.rs`
owns scope-aware substitution, node counting, and fresh-ID support. These
modules consume only Core types and source utilities, never CC, MIR, or Wasm
types.

## Invariants and verification

The Core verifier runs before and after every pass and checks types, scopes,
stable references, and spans. Pass-specific checks assert that an initializer
with either effect flag is not dropped or duplicated, that inlining binds
arguments once in order, and that specialization retains the generic entry.
The optimized and unoptimized paths must agree on results, traps, and ordered
external-call traces for representative programs.

## Worked example

```text
let x = trace(1) in (\y -> y + y) x
```

The inliner first binds `x` once, then substitutes the value identity twice in
`y + y`; it never copies `trace(1)`. Dead-binding elimination retains the
binding because `trace` may call externally. The result is observationally
equivalent even if `trace` records each invocation.

## Boundaries and interfaces

P7 receives verified, linked Typed Core from P6 and returns the same type to
P8. P8 may rely on preserved types, IDs, spans, and evaluation order and still
performs ANF and closure conversion. The external-binding projection remains a
separate input and is validated even for unused declarations. P7 never reads
`TargetCapabilities` or emits representation requirements.

## Open questions and future work

The cost model can be calibrated using code size and execution benchmarks.
More precise purity summaries require a sound interprocedural analysis and
must keep external and unknown closure calls conservative. Cross-module
specialization needs explicit linkage and invalidation rules.

## References

- [Functional core](../../frontend/semantics/functional-core.md), [effects](../fp/effects.md),
  [type classes](../fp/type-classes-and-dictionaries.md).
- Peyton Jones and Marlow, *Secrets of the Glasgow Haskell Compiler inliner*
  (2002), for bounded inlining and call-site guidance.
- [D-01 — Frontend and IR boundaries](../../D-01-frontend-and-ir-boundaries.md).

## Implementation notes

P7 is implemented in `crates/psrs-core/src/opt/` and runs in the backend
compile path immediately before external-binding extraction and P8. The
current passes fold total `Int` operations, reduce literal conditionals and
inert known constructor or record cases whose patterns need no nested runtime
cast, expose record and array projections while sequencing their inputs,
perform bounded lambda beta reduction, and remove unused inert `let` bindings.
Dictionary methods are exposed through the same known-record projection rule;
there is no separate dictionary pass. Array operations and signed division
remain potentially trapping unless a rewrite proves the access is in range or
the division is total.

Bounded named-global inlining applies to statically available, non-polymorphic
declarations whose body is below the node limit and whose call-graph path is
nonrecursive. It currently requires a fully applied direct global call with
one matching leading lambda per argument and skips candidate bodies containing
`Case`, so P8 source-spanned redundancy warnings are not duplicated. Each
argument is bound once in source order; cloned local and pattern binders receive
fresh IDs, call-site spans are used for generated bindings, and source spans
inside the cloned body are retained. The default limits are 24 body nodes
and 128 inlining sites per optimization round. The regression compares an
effect trace and a trap before and after inlining, and checks recursive calls
remain calls.

Concrete specialization currently applies to applied direct named-global
heads within the declaration's owning module when the use-site function type
contains no unresolved type variables. It structurally deduplicates instances,
substitutes types throughout the cloned declaration, clears the clone's
quantifiers, and redirects eligible calls while retaining the original generic
declaration. Its default limits are 32 generated declarations and 1,024 copied
Core expression nodes per P7 run. Calls with unresolved type variables,
cross-module calls, and calls beyond either budget keep the generic target.
Generated specializations that become unreferenced after later optimization
rounds are removed. Regressions cover concrete and nested aggregate types,
deduplication, generic fallback, same-module eligibility, budgets, and the
generic declaration surviving inlining of a small specialization.

Unknown calls remain conservatively observable to simplification and dead
binding elimination; inlining preserves calls and traps inside a known body.
Interprocedural purity and cross-module specialization remain future work.
