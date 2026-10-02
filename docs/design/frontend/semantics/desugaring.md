# Resolved HIR Desugaring

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [modules and resolution](modules-and-resolution.md),
[frontend boundaries](../00-ir-boundaries.md), and source evaluation order.

**Summary:** P4 rewrites surface constructs into a smaller resolved HIR without
changing semantic identities. P2 retains equation and guard forms while
normalizing case arity: multiple scrutinees become one closed record product
with `_1`, `_2`, and subsequent fields, and `_` scrutinees become generated
function inputs. P3 assigns those names stable IDs. P4 owns fixity application,
sections, `do` and `ado`, guarded equations and alternatives, and `where`
scope. The rewrite preserves source order and source origins for P5 diagnostics.

## Scope

This document owns same-representation, pre-typecheck term normalization. It
does not infer types, resolve new source names, choose pattern decision trees,
or introduce runtime representations. P5 checks the result; P6 removes the
remaining high-level forms on the way to Core.

## Background

Surface notation can describe the same operation in many forms. Fixity is
known only after P3 resolves operators. P2 converts case arity to a record
product without introducing a library tuple dependency; each source scrutinee
is a field expression in its original order. P2 retains guards and equation
alternatives for name resolution. `do` describes ordered binds, and guarded
equations describe ordered alternatives. Lowering these before type inference
gives the checker fewer term forms while retaining the user's declaration and
subexpression ranges.

## Model

```text
P4 : ResolvedHIR -> ResolvedHIR
Origin = { source: SourceId, range: TextRange, generated_from: NodeId }
```

The output uses the same HIR IDs and type-expression forms. Generated local
binders receive fresh IDs in the proper scope and an origin range. P4 consumes
resolved value, constructor-pattern, and type operator chains, applying their
associated fixity before lowering them to applications or constructor
patterns. It expands both operator sections to lambdas and applications of
the resolved operator. The normalized subset contains applications, lambdas,
`let`, conditionals, cases, records, and primitive declarations, with operator
and sequencing sugar expanded. Pattern syntax may remain for P5 and P6.

## Design

P4 applies resolved fixities to expression, constructor-pattern, and type
operator chains. It reassociates each chain by precedence and associativity;
type operators become type applications headed by the resolved `TypeId`.
Sections become lambdas whose bodies apply the resolved operator to the saved
operand and the new parameter in source order. Other syntactic operators
expand to applications of their resolved identity. P4 lowers `do` to
`bind`/`pure` applications and `ado` to its applicative form using the
resolved library identities, never matching a name's spelling.
If one unparenthesized chain uses operators of the same precedence with mixed
associativity, or repeats non-associative operators at that precedence, P4
reports the ambiguity at an operator span and requires parentheses. Parentheses
form separate chains, so an inner group is validated independently.
It converts multiple equations and guarded right-hand sides to ordered cases
and conditions with explicit fallthrough, and makes `where` bindings explicit
in their original lexical scope. Boolean `true` guards are unconditional only
when their resolved symbol is a compiler Boolean-true intrinsic or a
whole-program declaration proven to be a transparent alias of that intrinsic.
The proof follows resolved symbol identity through typed expressions and local
aliases; it does not infer truth from an import path or the spelling
`otherwise`.

P2 lowers integer literal patterns to a generated binder plus a compiler-owned
integer equality node. P3 resolves that node to `Intrinsic::I32Eq`, so a
source-level `==` binding cannot change pattern matching. Local `let` and
guarded `where` declaration annotations are retained as resolved `Typed`
expressions for P5 rather than discarded during name resolution.

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
        reject mixed associativity and repeated non-associative operators
        reassociate expression, pattern, and type operator chains by fixity
        expand sections using the resolved operator and retained operand side
        expand sequencing forms
        compile equations/guards to ordered HIR cases
        turn where bindings into scoped lets
    verify_normalized_hir(program)
    return program
```

Fresh IDs are allocated monotonically per declaration. The verifier checks
generated references and that no eliminated surface form remains. A rewrite
that would need unavailable library evidence is diagnosed at its source span.

## Code map

The `psrs-desugar` organization separates fixity reassociation and type
normalization from section, sequencing, equation, and `where` lowering. The
fixity logic handles value chains, constructor-pattern chains, and type chains;
type normalization traverses signatures and declaration types. `crates/psrs-ast/src/expr/guards.rs` owns P2 case-arity and literal-pattern
normalization. `crates/psrs-resolve/src/resolver/names/patterns.rs` assigns local
IDs and resolves local annotations. `crates/psrs-desugar/src/expr.rs` owns P4
case and equation lowering; `guards.rs`, `boolean_case.rs`, and
`boolean_product_case.rs` expand guard and Boolean paths; `case_helpers.rs`
contains coverage predicates and shared product helpers; `alpha.rs` assigns
fresh IDs to duplicated continuations; and `constant_truth.rs` proves
transparent Boolean-true aliases over the complete resolved program. P4 type
normalization runs after operator fixity reassociation. `psrs-hir::verify`
exposes resolved and normalized profile checks. The desugar crate depends on
HIR and source utilities, never THIR or backend types.

## Invariants and verification

Existing IDs keep their meaning, new local IDs are unique and scoped, and
source-origin ranges remain valid. Every output has the same observable
evaluation order as its input; tests cover fixity reassociation, ambiguity
diagnostics, sections, ordered guards, and single evaluation of scrutinees.
Multi-scrutinee records are bound once before pattern tests, preserving field
evaluation order. Generated continuation branches use fresh local IDs and
generated coverage provenance. The normalized verifier rejects expression,
pattern, and type operator chains, sections, `do`/`ado`, guarded equations,
`where` nodes, and any Boolean pattern nested inside a remaining case after P4.

## Worked example

```purescript
f x | x > 0 = x
    | otherwise = 0
```

P4 resolves `>` and uses the resolved, verified definition of `otherwise` only
if that declaration is a transparent alias of Boolean `true`. It then produces
an ordered conditional in a single equation body. `x` keeps its `LocalId`; the
comparison and each guard keep source origins. The second body runs only if the
first guard fails.

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
