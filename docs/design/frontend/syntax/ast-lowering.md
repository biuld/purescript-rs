# CST to AST Lowering

**Feature:** F-01, F-02

**Status:** Draft

**Prerequisites:** [parsing and CST](parsing-and-cst.md),
[frontend boundaries](../00-ir-boundaries.md), and lexical scope basics.

**Summary:** P2 converts verified CST into a separate, normalized surface AST.
It removes punctuation and parser-only distinctions while preserving source
ranges, unresolved names, declaration order, and evaluation order. Semantic
desugaring and name lookup remain later passes.

## Scope

This document owns CST-to-AST conversion and AST verification. It does not
resolve modules or fixities ([P3](../semantics/modules-and-resolution.md))
or lower `do`, equations, and guards
([P4](../semantics/desugaring.md)).

## Background

CST mirrors how source was written; AST captures what later source-level
analysis needs. Removing a parenthesis is safe because it only groups syntax.
Turning `\x y -> body` into nested lambdas is safe when binder order and spans
are retained. Operator identity and precedence are semantic, so the AST keeps
an unresolved operator chain for later resolution.

## Model

```text
AstModule = { name: TextName, imports, exports, fixities, declarations, span }
AstExpr   = { kind: AstExprKind, span: TextRange }
AstName   = { text: String, span: TextRange }
```

AST nodes have no token indices or punctuation fields. They keep unresolved
value, type, constructor, class, and module names. Declaration and field
orders are stable, as are ranges for names, binders, patterns, and operators.
When one CST node produces several AST nodes, each gets a source origin.
Fixity declarations retain their namespace, associativity, precedence, target,
alias, and spans. Expression and type operator chains retain source order, and
operator sections retain which side supplies their operand using explicit
anonymous arguments such as `(_ + 1)` and `(1 + _)`. Parenthesized unary
negation remains a negation expression; it is not interpreted as a section.
Backticked value sections such as ``(_ `eq` value)`` retain the same section
side and unresolved value identity, including local names.
Constructor operator patterns remain chains as well. None of these forms binds
an operator name or applies precedence in P2.

## Design

P2 maps each successful CST node to one or more AST nodes through an explicit
conversion. It removes redundant parentheses and layout delimiters; it
normalizes grouped function parameters to nested single-binder functions;
and it keeps all forms whose meaning depends on imports, types, or the
language's sequencing rules. The AST is a separate type, not a view or alias
of CST.

Record constructors with immediate anonymous field arguments become lambdas
in written field order: `{ z: _, a: _ }` becomes `\z a -> { z, a }`,
independently of canonical type or runtime field ordering. Record updaters
use the same rule, with an anonymous base record as the first argument.
Anonymous leaves of nested update paths belong to the enclosing updater;
an explicit field expression or nested record literal introduces its own
scope. P2 gives generated binders source-inexpressible names and preserves
each underscore's span. An immediate `_` in an `if` condition, then branch,
or else branch is a lambda parameter in that written order, as a `case`
scrutinee already is. An underscore in any other expression position
stays an unresolved name. These rules follow the official compiler's
`Sugar.ObjectWildcards` conversion and require no resolved names or types.
AST update fields explicitly distinguish an expression from a nested path.
P3 projects a nested path from its enclosing base, while resolving explicit
expressions in lexical scope: `field = field { value = 7 }` must use the
local `field` value rather than the enclosing record's `field` member.

Pattern normalization removes grouping parentheses, maps tuples to closed
records, and turns record puns into field-variable patterns. Record patterns
carry a match mode: source `{ field }` patterns are partial and accept other
fields, while tuple patterns and compiler-created products are exact closed
records. Literal, array, named (`name@pattern`), typed (`pattern :: Type`),
constructor, and unresolved operator-chain patterns remain explicit AST forms
with their original spans.
P2 does not replace a literal pattern with a generated equality expression or
discard a typed pattern's annotation; P3 resolves binders and operators, and P5
checks pattern types, enforces exact versus partial record rows, and consumes
typed annotations. The record mode ends at P5: the checked pattern type carries
the resulting row shape into Typed Core.

The conversion is total for every verified CST form. An unsupported construct
is a diagnostic at its own span, never a placeholder AST node. P2 does not
sort record fields when their expressions can evaluate in source order.

Rejected alternatives: reusing CST types would make later passes depend on
punctuation; doing full desugaring here would require name and type information
not yet established; and dropping origin ranges would impair diagnostics.

## Algorithms

```text
lower_expr(cst):
    Paren(inner)        -> lower_expr(inner) with enclosing origin retained
    Lambda([x, y], b)  -> Lambda(x, Lambda(y, lower_expr(b)))
    OperatorChain(xs)  -> AstOperatorChain(lower each operand/operator in order)
    OperatorSection   -> AstOperatorSection(lower operand, retain side/name)
    other               -> convert children in source order

lower_module(cst):
    lower header, imports, exports, and declarations in source order
    verify_ast(result)
```

An origin map or node span retains enough information to relate normalized
nodes to the concrete tokens that introduced them.

## Code map

The `psrs-ast` module organization separates declarations, expressions,
patterns, types, and fixity declarations around the conversion entry point
`lower_module(cst: &psrs_cst::Module) -> Result<Module, Vec<Diagnostic>>`.
`verify_ast` checks spans, well-formed binders, and absence of CST nodes. This
crate depends on `psrs-cst` and `psrs-span`, not HIR or the type checker.

## Invariants and verification

Names remain textual and source-spanned; no resolved ID, checked type, or
backend field appears. Parentheses and virtual layout tokens do not survive
as AST nodes. Grouped binders have the same order and scope after nesting.
Property tests compare parenthesized and unparenthesized equivalents, while
source inspection checks that operator and binder spans still point to the
original text.

## Worked example

```purescript
f = \x y -> (x + y)
```

P1 keeps the grouped binders and parentheses. P2 produces two nested lambda
nodes and an unresolved `+` chain, retaining the ranges of `x`, `y`, and `+`.
P2 leaves the operator, precedence, and section side explicit; P3 binds the
operator to a declaration and attaches its fixity; P4 reassociates the chain
and expands the section.

## Boundaries and interfaces

P2 consumes only verified CST and produces verified AST for P3. `psrs ast`
prints this tree. Source text remains available for diagnostic rendering;
the AST carries ranges rather than token objects.

## Open questions and future work

Additional syntax forms extend CST and this conversion together. A source
formatter may consume CST directly; AST should stay focused on semantic
analysis rather than formatting fidelity.

## References

- [Parsing and CST](parsing-and-cst.md),
  [modules and resolution](../semantics/modules-and-resolution.md), and
  [D-01](../../D-01-frontend-and-ir-boundaries.md).
