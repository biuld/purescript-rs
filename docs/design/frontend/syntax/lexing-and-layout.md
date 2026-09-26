# Lexing and Layout

**Feature:** F-01, F-02

**Status:** Draft

**Prerequisites:** [frontend boundaries](../00-ir-boundaries.md), UTF-8 byte
ranges, lexical analysis, and indentation-sensitive parsing.

**Summary:** P0 converts source bytes to positioned tokens and inserts virtual
layout markers for indentation-based blocks. It preserves physical spelling
and trivia through source ranges, reports lexical errors locally, and gives P1
a deterministic token stream without resolving names or types.

## Scope

This document owns tokenization, comments and literals, newline/indentation
tracking, virtual layout markers, and lexical diagnostics. The grammar that
consumes those markers belongs to [P1](parsing-and-cst.md); CST node shapes
and semantic names are outside P0.

## Background

PureScript permits declaration and expression blocks without braces. A lexer
recognizes physical tokens; a layout processor makes block boundaries explicit
to the parser. Physical source positions must remain authoritative because
virtual tokens have no text and comments can affect indentation without
becoming expressions.

## Model

```text
Token = { kind, range: TextRange, source: SourceId }
Kind  = Identifier | Operator | Keyword | Literal | Punctuation
      | LayoutStart | LayoutSep | LayoutEnd | EndOfFile
TokenStream = { source: SourceId, tokens: [Token], diagnostics: [Diagnostic] }
```

Physical token ranges cover exactly their source bytes. Virtual markers have
zero-width ranges at the physical token or end of file that caused them. A
layout stack stores indentation columns and the syntactic context that opened
each implicit block; explicit braces are distinct stack entries. Columns are
computed from the original source line, not from token order or UTF-8 byte
count. Trivia stays recoverable from the source between token ranges.

## Design

Lexing and layout are consecutive operations in P0 so `psrs lex` can show
physical tokens and `psrs layout` can show the augmented stream. The lexer
validates escapes, character scalars, numeric forms, and unterminated strings
or comments without inventing a semantic value. A character literal is one
Unicode scalar, including the astral range U+10000 through U+10FFFF, written
either as the character or as a `\x` escape of one to six hexadecimal digits.
Surrogate code points are not scalars and are not character literals. The layout processor inserts
markers only at grammar-defined layout introducers, honors explicit braces,
and closes implicit blocks before an outer dedent and at end of file.

A malformed token produces a diagnostic with its physical range. P0 may
continue scanning to report more lexical errors, but no verified token stream
is passed to P1 while errors remain. Source text and line maps are shared
utilities, never copied into each token.

Rejected alternatives: using indentation directly in parser branches would
duplicate offside rules; treating virtual markers as physical source would
produce misleading ranges; and normalizing identifiers in P0 would lose
source spelling and preempt later namespace rules.

## Algorithms

```text
lex(source):
    scan UTF-8 bytes in order
    emit physical token(kind, exact byte range)
    retain line starts and lexical diagnostics

layout(tokens):
    stack = [top-level context]
    for each physical token t:
        close implicit contexts whose indentation exceeds t's column
        emit LayoutSep at a same-column item boundary where grammar permits
        open an implicit context after a layout introducer without explicit '{'
        emit t
    close remaining implicit contexts at EndOfFile
```

The actual opener/separator decision uses token context, not indentation
alone: a continuation line inside an expression is not a new declaration.
Tests compare the resulting markers with the official compiler on nested
`let`, `where`, `do`, `ado`, `case`, explicit braces, comments, and empty blocks.

## Code map

`crates/psrs-span/src/` owns `SourceId`, `TextRange`, and line/column mapping.
`crates/psrs-syntax/src/lexer.rs` owns
`lex(source: &SourceFile) -> Result<Vec<Token>, Vec<Diagnostic>>`;
`layout.rs` owns
`insert_layout(source: &SourceFile, tokens: &[Token]) -> Result<Vec<Token>, Vec<Diagnostic>>`.
`lib.rs` exposes both for the source-inspection commands and the parser. Neither
module imports CST, AST, HIR, or type-checker types.

## Invariants and verification

Physical ranges are ordered, in bounds, and nonoverlapping except for
documented composite tokens. Virtual markers have zero width, balanced block
starts and ends, and deterministic placement. Every diagnostic points to the
original source. Verify token/range consistency before P1 and compare layout
traces across explicit and implicit brace spellings.

## Worked example

```purescript
let x = 1
    y = 2
in x + y
```

The physical stream contains `let`, the names, literals, `in`, and operator.
Layout inserts a start before `x`, a separator before `y`, and an end before
`in`; those three markers have zero-width source ranges. P1 sees one explicit
block structure regardless of whether the programmer wrote braces.

## Boundaries and interfaces

P0 receives `SourceFile` bytes and returns a verified `TokenStream` for P1.
`psrs lex` and `psrs layout` print its two views with physical source
positions. P0 never allocates declaration IDs, checks types, or chooses AST
normalization.

## Open questions and future work

Incremental lexing may reuse unchanged line segments only if layout context
at the restart boundary is known. Error recovery can collect additional
diagnostics without weakening the verified-output contract.

## References

- [Frontend boundaries](../00-ir-boundaries.md) and
  [F-01 source inspection](../../../feature/F-01-source-inspection.md).
- [Official PureScript compiler](https://github.com/purescript/purescript),
  as the grammar and layout compatibility oracle.
