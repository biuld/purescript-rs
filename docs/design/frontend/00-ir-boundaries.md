# Frontend IR Boundaries

**Feature:** F-01, F-02

**Status:** Draft

**Prerequisites:** [D-01](../D-01-frontend-and-ir-boundaries.md), source ranges,
syntax trees, lexical scope, and typed intermediate representations.

**Summary:** P0–P6 are explicit transformations from source text to Typed Core.
Each persistent representation has one owner and verifier, and no pass writes
semantic fields into an earlier syntax representation. This contract assigns
source information, identifiers, errors, and boundary checks to their stages.

## Scope

This document owns frontend stage order, persistent representation boundaries,
source-range propagation, and validation handoffs. Individual syntax and
semantic algorithms live in the topic documents in this directory. P7 Core
optimization is specified in [backend optimization](../backend/opt/core.md),
and P8 onward in [backend IR boundaries](../backend/00-ir-boundaries.md).

## Background

Concrete syntax serves source tools; normalized AST serves syntax-independent
analysis; resolved HIR identifies binders; THIR records checked types and
evidence; Core removes surface forms. Combining them would let a pass rely on
fields that have not yet been established or discard source structure needed by
an earlier tool. A boundary verifier turns the representation's promise into a
checkable condition.

## Model

| Pass | Input | Output | Required new invariant |
| --- | --- | --- | --- |
| P0 | Source bytes | TokenStream | Tokens and layout markers have ordered source ranges. |
| P1 | TokenStream | CST | Parsed concrete forms retain syntax spans. |
| P2 | CST | AST | Syntax-only distinctions are normalized; names remain textual. |
| P3 | AST + module environment | Resolved HIR | Every value, type, constructor, class, and module reference has a stable ID. |
| P4 | Resolved HIR | Resolved HIR | Surface sugar is removed without changing IDs or binding scope. |
| P5 | Resolved HIR | THIR | Kinds and types are checked; overloads carry explicit evidence. |
| P6 | THIR | Typed Core | Only the small Core grammar remains; checked types and spans survive. |

`SourceId` identifies a source file; `TextRange` is a half-open byte interval in
that file. Synthetic nodes retain an origin range and a reason for synthesis.
The token stream is a parser input, not an IR family. P4 is a same-type pass;
P3 and P5 create new representations by conversion.

## Design

Each representation is a distinct type with a directed dependency on source
utilities and prior representations. CST does not import AST, HIR, or type
checker types. AST does not import resolved IDs. HIR has IDs and unresolved
type expressions only where the next pass must still check them; THIR has
checked types and evidence but no Wasm fields. Core has no CST or AST node.

Source spelling and trivia stay in the source file, with CST ranges for tools
that need them. AST and later stages preserve ranges for names, binders,
declarations, calls, patterns, and diagnostics. A generated node records its
origin; diagnostic rendering uses the origin rather than a fabricated offset.

Errors stop the current boundary. Recoverable parsing may collect multiple
diagnostics but may not present a partially recovered CST as verified output.
No later pass guesses a missing identifier, type, evidence value, or range.

Rejected alternatives: one mutable tree with optional semantic fields has no
single verifier contract; letting P5 repair unresolved HIR conceals P3 bugs;
and throwing away spans at AST lowering prevents source-oriented diagnostics.

## Algorithms

```text
run_frontend(source_set):
    for each source: tokens = P0(source); verify_tokens(tokens)
    for each tokens: cst = P1(tokens); verify_cst(cst)
    for each cst: ast = P2(cst); verify_ast(ast)
    program = P3(all ast modules); verify_hir(program)
    program = P4(program); verify_normalized_hir(program)
    thir = P5(program); verify_thir(thir)
    core = P6(thir); verify_core(core)
    return core
```

The driver preserves deterministic module and declaration order for dumps and
diagnostics. P3 checks the full import/export graph before P5. P5 checks all
declared signatures before producing THIR, and P6 refuses incomplete evidence.

## Code map

`psrs-span` owns source files, byte ranges, and line mapping. `psrs-syntax`
owns P0/P1 over `psrs-cst`; `psrs-ast` owns P2; `psrs-resolve` owns P3 over
`psrs-hir`; `psrs-desugar` owns P4; `psrs-kind` and `psrs-typecheck` own P5 and
produce `psrs-thir`; `psrs-core` owns P6 and Core verification. `psrs-driver`
orchestrates pass entry points and diagnostics. Each topic's Code map specifies
the modules and signatures within that owner.

## Invariants and verification

Every output verifier checks ranges are within its source file, IDs index the
correct arena and namespace, and no forbidden later-stage field appears.
Round-trip or source-slice checks validate CST ranges; scope checks validate
HIR IDs; type and evidence checks validate THIR; Core checks its smaller term
set. Debug dumps name the representation and do not silently skip a stage.

## Worked example

For `let x = 1 in x`, P0 records tokens and a source range for each `x`; P1
keeps the `let` punctuation and layout; P2 retains a normalized `Let` with
textual `x`; P3 gives binder and use the same `LocalId`; P5 assigns `Int` to
both; P6 keeps a typed `Let` and the original binder/use ranges. None of these
stages chooses whether `Int` is a Wasm `i32`.

## Boundaries and interfaces

The source-inspection commands expose P0–P3 dumps as specified by
[F-01](../../feature/F-01-source-inspection.md). The build driver consumes
verified Core at the P6/P7 boundary and passes an external-binding side table
to P8. Target capabilities, canonical ABI signatures, and layout choices are
absent from P0–P6; source WIT binding text is retained as declaration metadata
until the backend validates it.

## Open questions and future work

Incremental recompilation may cache verified stage outputs by source and
dependency fingerprints; cache invalidation must preserve ID and span validity.
The official suite coverage and implementation phases are tracked in
[frontend type-system design](type-system/README.md) and [DEC-04](../../decision/DEC-04-official-test-suite-roadmap.md),
not in the representation contract.

## References

- [D-01 — Frontend and IR boundaries](../D-01-frontend-and-ir-boundaries.md).
- [Functional Core](semantics/functional-core.md).
- [Backend IR boundaries](../backend/00-ir-boundaries.md).
