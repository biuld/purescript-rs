# Parsing and Concrete Syntax

**Feature:** F-01, F-02

**Status:** Draft

**Prerequisites:** [lexing and layout](lexing-and-layout.md),
[frontend boundaries](../00-ir-boundaries.md), and recursive-descent parsing.

**Summary:** P1 consumes the layout-augmented token stream and produces a
source-oriented CST. The CST retains concrete alternatives and the ranges of
tokens needed for diagnostics and source inspection. Parsing establishes
grammar structure only; name and type meaning begins later.

## Scope

This document owns the module, declaration, expression, pattern, import/export,
and type-expression grammar as CST forms, parser recovery, and CST verification.
It does not normalize syntax-only forms ([P2](ast-lowering.md)) or resolve
references ([P3](../semantics/modules-and-resolution.md)).

## Background

The parser needs concrete alternatives such as parentheses, operator chains,
guards, grouped binders, and explicit versus layout-delimited blocks so later
diagnostics can point at source syntax. The original source remains the owner
of comments and exact text; a CST node stores token and child ranges rather
than duplicating every byte.

## Model

```text
CstModule = { header, imports, declarations, range }
CstNode   = { kind, children, range, significant_token_ranges }
ParseResult = { module: Option<CstModule>, diagnostics: [Diagnostic] }
```

The grammar includes module headers and export lists, imports and hiding,
value/type/class/instance declarations, expressions, binders, patterns, and
type expressions. A token range is half-open in its `SourceId`. The CST may
retain a recovered error node for diagnostics, but only an error-free tree is
verified and passed to P2.

## Design

The parser uses recursive descent for declarations and delimited forms, with
precedence handling that preserves unresolved operator structure. Fixity
resolution belongs after names have IDs; P1 must not guess the semantic
precedence of imported operators. A function declaration retains its equations,
guards, and `where` body. A type expression retains `forall`, application,
arrows, rows, and annotations without kind checking.

Recovery synchronizes at declaration boundaries, layout separators, and
matching delimiters to report multiple errors. Recovery never silently
manufactures a valid declaration; build mode fails if diagnostics remain.
An inspection dump may show the recovered CST with errors marked, but it is
not input to P2.

Rejected alternatives: parsing straight to AST would erase concrete forms;
resolving fixities in P1 would require the module environment; and storing
checked types on CST nodes would violate the representation boundary.

## Algorithms

```text
parse_module(stream):
    parse header and optional export list
    parse imports until first declaration
    while not EndOfFile:
        try parse_declaration
        on error: record diagnostic and synchronize at next declaration boundary
    if diagnostics are empty: verify_cst(module); return module
    return diagnostics with optional recovery tree
```

Delimited parsers consume either explicit brace tokens or balanced virtual
layout markers through a common block interface. Each node range extends from
its first to last physical token, including enclosing parentheses where the
concrete form needs them.

## Code map

`crates/psrs-cst/src/` defines `Module`, declaration, expression, pattern,
import/export, and type-expression nodes. `crates/psrs-syntax/src/parser/`
owns `parse_module(tokens: &[Token]) -> ParseResult`, with focused
`declaration/`, `expr/`, `import.rs`, and `type_expr.rs` modules. `verify_cst`
checks child ranges and structural grammar invariants. The parser depends on
tokens and CST, never AST or semantic crates.

## Invariants and verification

Every CST child lies inside its parent range; token indices and source ranges
are valid; virtual delimiters are balanced; and the module contains only
grammar-permitted forms. A successful parse has no recovery nodes or
diagnostics. Parser fixtures compare accept/reject and relevant syntax trees
with the official suite; malformed delimiters and nested layout receive
source-spanned errors.

## Worked example

`f (x) = x + 1` retains parentheses around the binder and an operator
expression for `x + 1`. It does not decide whether `+` is imported or which
declaration `x` denotes. P2 may remove the parentheses; P3 resolves the names.

## Boundaries and interfaces

P1 receives a verified token stream and returns CST plus diagnostics. `psrs
parse` exposes the concrete tree. P2 consumes only a successful CST; source
text remains available separately for slices and diagnostics.

## Open questions and future work

Lossless trivia nodes may be added for formatting tools without changing
semantic passes. Incremental parsing requires stable subtree/source mapping
and must not weaken the successful-CST verifier.

## References

- [Lexing and layout](lexing-and-layout.md),
  [AST lowering](ast-lowering.md), and
  [D-01](../../D-01-frontend-and-ir-boundaries.md).
- [Official PureScript compiler](https://github.com/purescript/purescript),
  as the syntax-compatibility oracle.
