# Frontend Design

The frontend turns source text into verified Typed Core. Its topics live under
source structure (`syntax/`, P0–P2), name and term elaboration (`semantics/`,
P3/P4/P6), and the type system (`type-system/`, P5).
[00-ir-boundaries.md](00-ir-boundaries.md) defines their stage contracts.
[D-01](../D-01-frontend-and-ir-boundaries.md) remains the P0–P11 overview;
[DEC-04](../../decision/DEC-04-official-test-suite-roadmap.md) tracks implementation coverage.

```mermaid
flowchart LR
    source[Source] --> tokens[P0 tokens and layout]
    tokens --> cst[P1 CST]
    cst --> ast[P2 AST]
    ast --> hir[P3 resolved HIR]
    hir --> desugar[P4 normalized HIR]
    desugar --> thir[P5 THIR]
    thir --> core[P6 Typed Core]
    core --> backend[P7/P8 backend]
```

## Source structure (`syntax/`)

| Document | Owns | Output |
| --- | --- | --- |
| [Lexing and layout](syntax/lexing-and-layout.md) | Tokens, virtual layout markers, lexical spans | TokenStream |
| [Parsing and CST](syntax/parsing-and-cst.md) | Grammar, recovery, concrete syntax | CST |
| [AST lowering](syntax/ast-lowering.md) | Syntax-only normalization | AST |

## Name and term elaboration (`semantics/`)

| Document | Owns | Output |
| --- | --- | --- |
| [Modules and resolution](semantics/modules-and-resolution.md) | Module graph, imports/exports, stable IDs | Resolved HIR |
| [Desugaring](semantics/desugaring.md) | Surface constructs lowered while preserving HIR | Normalized HIR |
| [Core lowering](semantics/core-lowering.md) | THIR-to-Core conversion and verification | Typed Core |
| [Functional Core](semantics/functional-core.md) | Producer-owned Core type, term, and semantic contract | P7 input |

## Type system (`type-system/`)

| Document | Owns | Output |
| --- | --- | --- |
| [Type-system index](type-system/README.md) | P5 topic order and shared rules | — |
| [Kinds](type-system/kinds.md) | Kinds, constructor application, synonym legality | Checked kinds |
| [Type inference](type-system/type-inference.md) | Schemes, unification, signatures, typed terms | THIR types |
| [Classes and evidence](type-system/classes-and-evidence.md) | Constraint solving, coherence, explicit dictionaries | THIR evidence |
| [Rows and records](type-system/rows-and-records.md) | Row unification and record typing | Typed row operations |

Each arrow is an explicit conversion or a verified same-representation pass.
No semantic ID or type is added to CST or AST. Frontend representations carry
source ranges through P6; runtime layouts and target capabilities begin below
the backend's P9 boundary.
