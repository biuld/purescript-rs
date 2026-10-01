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
StringValue = sequence of Unicode scalar values
```

Physical token ranges cover exactly their source bytes. Virtual markers have
zero-width ranges at the physical token or end of file that caused them. A
layout stack stores indentation columns and the syntactic context that opened
each implicit block; explicit braces are distinct stack entries. Columns are
computed from the original source line, not from token order or UTF-8 byte
count. Trivia stays recoverable from the source between token ranges.

`StringValue` is a finite sequence of Unicode scalar values
([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)). It
contains no surrogate code point. String literals, type-level `Symbol`
literals, and quoted record labels carry this value through later
representations. Identifier spelling remains Unicode text, and `TextRange`
remains a range in the UTF-8 source bytes. Neither is converted to
`StringValue`.

## Design

Lexing and layout are consecutive operations in P0 so `psrs lex` can show
physical tokens and `psrs layout` can show the augmented stream. The lexer
validates escapes, character scalars, numeric forms, and unterminated strings
or comments. A character literal is one Unicode scalar value, written
directly or as a `\x` escape, including a supplementary scalar. An unpaired
surrogate escape is a lexical error. The compiler does not replace it with
U+FFFD. In a string, a contiguous escaped high-surrogate/low-surrogate pair
decodes as its one corresponding scalar, so `"\\xD834\\xDF06"` and
`"\\x1D306"` are the same one-scalar value. A lone `"\\xD834"` is rejected.

The same value type represents a type-level `Symbol` and a quoted record
label. Row identity uses exact scalar-sequence equality, and row equivalence
does not depend on declaration order. Source-level equality and concatenation
operate on scalar sequences and do not apply Unicode normalization. Type-level
symbol solver behavior belongs to [classes and evidence](../type-system/classes-and-evidence.md).
At the WIT boundary a source string is already UTF-8 and is copied after
validation, as specified by [the Canonical ABI boundary](../../backend/wasm/linear-memory-and-canonical-abi-boundary.md).
WIT `list<u8>` is `Array Int`, not a string.

The layout processor distinguishes implicit blocks from delimiter and masking
contexts. Commas close every exposed implicit block; case binders, guards,
record labels, and class declaration heads mask commas where they belong to
that syntax. Lambda binders mask their arrow from an enclosing guard. A
closing backtick closes its exposed implicit blocks before ending the infix
expression. Guard terminators emit layout ends for exposed `do` blocks before
removing the guard context. Record labels mask keyword behavior, including
empty-record and row-tail boundaries. Dedents and end of file close implicit
blocks with zero-width markers.

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
String values are validated Unicode scalar sequences. Because every such
sequence is well-formed Unicode text, UTF-8 text is a sufficient carrier from
CST through backend constants. That carrier does not own source ranges,
identifiers, or display text.
`crates/psrs-syntax/src/lexer/` owns
`lex(source: &SourceFile) -> Result<Vec<Token>, Vec<Diagnostic>>`;
`layout.rs` owns
`insert_layout(source: &SourceFile, tokens: &[Token]) -> Result<Vec<Token>, Vec<Diagnostic>>`.
`lib.rs` exposes both for the source-inspection commands and the parser. Neither
module imports CST, AST, HIR, or type-checker types.

## Invariants and verification

Physical ranges are ordered, in bounds, and nonoverlapping except for
documented composite tokens. Virtual markers have zero width, balanced block
starts and ends, and deterministic placement. Every diagnostic points to the
original source. Equal string values have equal scalar sequences; appending
values concatenates those sequences without normalization. A surrogate-pair
escape and its supplementary scalar compare equal. An unpaired surrogate is
rejected and is not stored as U+FFFD. Row labels and type-level symbols retain
the same value through parsing and lowering. Verify token/range consistency
before P1 and compare layout traces across explicit and implicit brace
spellings. Lone-surrogate cases in the official string corpus are intentional
differences ([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).

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

## Implementation notes

The lexer now decodes `\x` escapes as scalar values: a paired surrogate escape is
one scalar, and an unpaired surrogate escape is rejected rather than becoming
U+FFFD. A supplementary scalar is accepted as one `Char`. The backend stores GC
strings as canonical UTF-8 bytes and transcodes them at the ABI boundary. The
resulting L1 differences against `purs` are the DEC-16 intentional differences
recorded in [D-04](../../D-04-suite-roadmap.md). Parse agreement does not
verify scalar-string values.
