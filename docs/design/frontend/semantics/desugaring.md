# Resolved HIR Desugaring

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [modules and resolution](modules-and-resolution.md),
[frontend boundaries](../00-ir-boundaries.md), and source evaluation order.

**Summary:** P4 rewrites surface constructs into a smaller resolved HIR without
changing semantic identities. It owns fixity application, sections, `do` and
`ado`, guards, multiple equations, and `where` scope. The rewrite preserves
source order and source origins for P5 diagnostics.

## Scope

This document owns same-representation, pre-typecheck term normalization. It
does not infer types, resolve new source names, choose pattern decision trees,
or introduce runtime representations. P5 checks the result; P6 removes the
remaining high-level forms on the way to Core.

## Background

Surface notation can describe the same operation in many forms. Fixity is
known only after P3 resolves operators. `do` describes ordered binds, and
guarded equations describe ordered alternatives. Lowering these before type
inference gives the checker fewer term forms while retaining the user's
declaration and subexpression ranges.

## Model

```text
P4 : ResolvedHIR -> ResolvedHIR
Origin = { source: SourceId, range: TextRange, generated_from: NodeId }
```

The output uses the same HIR IDs and type-expression forms. Generated local
binders receive fresh IDs in the proper scope and an origin range. The
normalized subset contains applications, lambdas, `let`, conditionals, cases,
records, and primitive declarations, with operator and sequencing sugar
expanded. Pattern syntax may remain for P5 and P6.

## Design

P4 applies resolved fixities to operator chains, then expands sections and
other syntactic operators into applications of their resolved symbol. It
lowers `do` to `bind`/`pure` applications and `ado` to its applicative form
using the resolved library identities, never matching a name's spelling.
It converts multiple equations and guarded right-hand sides to ordered cases
and conditions with explicit fallthrough, and makes `where` bindings explicit
in their original lexical scope.

Every rewrite evaluates source operands in the order defined by the language.
A failed guard proceeds to the next guard without evaluating that guard's body.
A generated temporary binds an expression once when duplication would change
evaluation. P4 preserves the source span of every retained user expression;
generated scaffolding points to the construct that introduced it.

Rejected alternatives: desugaring operators in P2 cannot respect imported
fixities; waiting until MIR would discard useful source types and spans; and
duplicating a scrutinee in each equation can duplicate effectful calls.

## Algorithms

```text
desugar(program):
    verify_resolved_hir(program)
    for each declaration in source order:
        resolve operator chain using its bound fixities
        expand sections and sequencing forms
        compile equations/guards to ordered HIR cases
        turn where bindings into scoped lets
    verify_normalized_hir(program)
    return program
```

Fresh IDs are allocated monotonically per declaration. The verifier checks
generated references and that no eliminated surface form remains. A rewrite
that would need unavailable library evidence is diagnosed at its source span.

## Code map

`crates/psrs-desugar/src/` owns
`desugar(program: hir::Program) -> Result<hir::Program, Vec<Diagnostic>>`.
`fixity.rs` handles operator precedence; `sections.rs` operator sections;
`sequencing.rs` `do`/`ado`; `equations.rs` guards and function equations;
`where_bindings.rs` local scope. `psrs-hir/src/verify.rs` exposes both the
resolved and normalized profile checks. The desugar crate depends on HIR and
source utilities, never THIR or backend types.

## Invariants and verification

Existing IDs keep their meaning, new local IDs are unique and scoped, and
source-origin ranges remain valid. Every output has the same observable
evaluation order as its input; tests cover ordered guards and single
evaluation of scrutinees. The normalized verifier rejects operator chains,
sections, `do`/`ado`, guarded equations, and `where` nodes after P4.

## Worked example

```purescript
f x | x > 0 = x
    | otherwise = 0
```

P4 resolves `>` and `otherwise`, then produces an ordered conditional in a
single equation body. `x` keeps its `LocalId`; the comparison and each guard
keep source origins. The second body runs only if the first guard fails.

## Boundaries and interfaces

P4 receives verified resolved HIR and returns verified normalized HIR to P5.
Because it preserves the HIR representation, P5 does not need a new ID space.
P4's generated binders and library calls participate in ordinary type
checking. Core and CC never see the eliminated surface forms.

## Open questions and future work

The exact ordering of `ado` dependency groups and all official syntax forms
must be compared with the official suite. Additional syntax sugar should be
added here only when its lowering needs resolved identities but not checked
types.

## References

- [Modules and resolution](modules-and-resolution.md),
  [type inference](../type-system/type-inference.md), and
  [D-01](../../D-01-frontend-and-ir-boundaries.md).
