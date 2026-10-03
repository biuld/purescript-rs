# Rows, Records, and Variants

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [kinds](kinds.md), [type inference](type-inference.md), and [classes](classes-and-evidence.md).

**Summary:** P5 implements PureScript's extensible rows for records and row-based library types. It unifies open and closed rows by label, solves the primitive row relations, and checks record operations without fixing runtime field layout.

## Scope

This document owns row types, unification, row constraints, record literals/access/updates, and typing of row-based library abstractions. Runtime record and variant layouts belong to [backend data representation](../../backend/fp/data-representation.md). No compiler-native effect row is introduced.

## Background

`{ x :: Int | r }` abbreviates `Record (x :: Int | r)` with `r :: Row Type`. A closed row ends in the empty row; an open row ends in a variable. Row entries are matched by label rather than textual order. The primitive `Prim.Row` classes (`Cons`, `Lacks`, `Union`, `Nub`) expose row relationships through type-class constraints and functional dependencies. `Variant` is a library type constructor over a row, rather than a separate built-in row kind.

## Model

```text
Row(k) = Empty(k) | Extend(label: Symbol, field: k, tail: Row(k))
RecordType(row: Row Type) = Record row
RowEquation = Equal(Row, Row) | Cons(label, field, tail, whole)
            | Lacks(label, row) | Union(left, right, union) | Nub(row, nubbed)
NormalizedRow = { fields: [(Symbol, Type)], tail: Closed | Open(variable) }
RowNormalization = Ok(NormalizedRow) | InvalidShape(TextRange)
```

A row is a value of the shared type spine, not a side structure: `RowEmpty` and
`RowExtend` are ordinary type nodes, `Row k` is the kind `App(Builtin(Row), k)`, and
a row-polymorphic variable is an ordinary variable whose recorded kind is `Row k`.
Rows therefore gain nothing from a representation of their own, and the kind of a row
is maintained by the shared kind solver rather than by a row-local table.

Rows preserve source label ranges but compare modulo label order. A label is a
type-level `Symbol` value: a sequence of Unicode scalar values, distinct from
identifier text, with no Unicode normalization
([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)). An
unpaired surrogate is not a label. Matching, duplicate detection, and canonical
label ordering use that exact scalar sequence. A rigid row variable cannot be extended by
unification; an inference unknown may be solved subject to occurs and kind
checks. A duplicate label is handled according to the source operation and
primitive relation, rather than silently collapsed by a map. Record field types
always have kind `Type`; generic primitive row relations can range over `Row k`.

## Design

Normalize rows for comparison into labelled entries and a tail while retaining original ranges. Align labels by exact scalar-sequence equality and unify their field types. Sort only for canonical row equality or lookup; field declaration order does not affect a row's meaning. Distribute unmatched entries into open unknown tails with a fresh shared remainder when both sides are open. Reject unmatched entries against a closed or rigid tail. Check record literals against closed or expected open rows, access against a row containing the field, and updates against the official record-update typing rules. Row-based library operations produce or consume class constraints; their primitive solvers share the same row normalizer.

Row equality does not depend on the head a row sits under. `Checker::unify` reaches the row normalizer for a `Record`-to-`Record` pair and for any pair where either side is a row, so `Record (x :: Int | r)` and a bare `Row k` argument are one equality: the alternative is that a row-polymorphic declaration's own use reports two identical rows as a mismatch, and that a rule which decides a row cannot bind it.

Normalization has one owner and one result type. `normalize_row` returns the fields and tail it actually found, or `InvalidShape` with the offending range when the value is not a row. It never converts a shape it does not understand into a closed row: a value that resolved to a non-row is a kind error, and reporting it as a closed row instead loses the fields already collected and leaves the caller with no evidence of the original mistake. Every consumer — row unification, the `Prim.Row` primitive relations, record literals, field access, and record update — goes through that one function, so two of them cannot disagree about a row's tail.

A row's kind comes from the shared kind state. A field has the kind its value type has, a row tail has kind `Row k`, and a record is `Record : Row Type -> Type` applied to a row of kind `Row Type`; those facts are read through the one denotation and the one primitive kind table that [kinds](kinds.md) owns. A record literal and an equivalent constructor application therefore reach the same row normalizer with the same kinds.

The primitive row relations are dispatched by resolved class identity to the rules that own them, and those rules use this normalizer and the shared kind solver. [Primitives](prim.md) owns which `Prim.Row` and `Prim.RowList` members exist and what each rule decides; this document owns the structure they decide over. A rule builds or matches an extension, proves a label absent, merges two rows, canonicalizes one, or converts a row to a list through this normalizer, and none of them keeps a private notion of what a row's kind is.

Record subsumption also checks missing or additional fields when one side is closed. Preserve label-specific diagnostics and keep type-level `Symbol` values distinct from runtime strings. Type checking does not choose memory offsets or reorder record expressions for codegen.

Rejected alternatives: positional row equality makes field order observable in types; silently discarding duplicate labels breaks row relations; treating row polymorphism as an effect system conflicts with PureScript's ordinary record and library model; and normalizing an unrecognized shape into a closed row hides the error that produced it.

## Algorithms

```text
normalize_row(row) -> RowNormalization:
    follow solved row variables, collecting (label, field type) in order
    on Empty: return Ok(fields, Closed)
    on a variable: return Ok(fields, Open(variable))
    on anything else: return InvalidShape(range)   # not a row

unify_rows(left, right):
    normalize both sides; report InvalidShape at its own range
    align common labels and unify their field types
    if both remainders empty: unify tails
    if one tail is an unknown: solve it with opposite remainder and tail
    if both tails are distinct unknowns:
        create fresh tail; solve each with the other's remainder and fresh tail
    otherwise reject unmatched labels at their source ranges

solve_row_rule(constraint):                       # one rule per Prim.Row member
    normalize the rows the constraint names
    build, split, merge, canonicalize, or convert through normalize_row
    return Solved, Deferred with the remainder as an obligation,
        Undecided, or Failed                          # see primitives
```

Occurs checks traverse field types and tails. A solver must not bind a rigid row variable or invent a duplicate field to satisfy a constraint. Every row equation also runs the shared kind check on the tails it solves, so a tail cannot acquire a kind its row does not admit.

## Code map

`crates/psrs-typecheck/src/typecheck/rows/` owns `row.rs` (`RowView`, `RowEntry`, `normalize_row`), `unify.rs`, `records.rs`, and the structure the `Prim.Row` rules operate on. The per-member rules themselves live with the primitive table in [primitives](prim.md). Row labels use the shared scalar-string value; `psrs-span` continues to own their source ranges, and identifier spelling remains text. Key entry points are `normalize_row(&InferType) -> RowNormalization`, `unify_rows(&InferType, &InferType, &mut InferState) -> Result<(), Diagnostic>`, and `check_record(&hir::Expr, &ExpectedType) -> Result<thir::Expr, Diagnostic>`. Row kinds are read through the kind owner in [kinds](kinds.md); the row module defines no kind of its own. THIR stores typed record operations and checked row types; MIR later fixes layouts.

## Invariants and verification

Row equality is independent of source label order; matched fields have equal checked types; tails have the correct `Row k` kind; no unknown or duplicate introduced by the solver escapes THIR. An invalid row shape is reported at the range where the shape was found and never silently becomes a closed row, so a caller can still see the fields collected before the failure. Record syntax and an equivalent constructor application reach the same normalizer with the same kinds. Verify closed/open unification, shared-tail cases, rigid tails, duplicate labels, access/update, row-polymorphic library signatures, and the invalid-shape diagnostic against official `purs`; [primitives](prim.md) owns the `Cons`/`Lacks`/`Union`/`Nub` and `RowToList` cases.

## Worked example

`getX :: forall r. { x :: Int | r } -> Int` accepts `{ x: 1, y: true }` by solving `r` as `(y :: Boolean)`. Calling it with `{ y: true }` fails at label `x`. Reordering the literal's fields does not alter its type. A `Variant (left :: Int | r)` uses the same `Row Type` machinery, while the `Variant` constructor and its operations come from a library.

## Boundaries and interfaces

P5 receives kind-checked row expressions and resolved record operations from HIR. It supplies type equations and evidence to [type inference](type-inference.md) and emits verified THIR. Row normalization and the primitive row relations are internal to P5 and share the kind and evidence contracts of [kinds](kinds.md) and [classes](classes-and-evidence.md); a later stage receives only checked row types. The backend chooses record and variant representation only after Core lowering.

## Open questions and future work

Use official tests to pin down duplicate-label diagnostics, record-update edge cases, and primitive row improvement order; [primitives](prim.md) owns the rules those tests exercise. [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md) tracks implementation coverage.

## References

- [PureScript row unification](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Unify.hs), [row primitives](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Environment.hs), and [entailment](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Entailment.hs).
- [Type inference](type-inference.md), [primitives](prim.md), and [backend data representation](../../backend/fp/data-representation.md).

## Implementation notes

`normalize_row` is the one normalizer, and it returns the fields and tail it
found or a `RowShapeError` carrying the entries it had already collected; a row
that resolved to a non-row value is a diagnostic, never a closed row. Because
`InferType` holds no source ranges, the error carries the range of the operation
that reached the shape rather than the shape's own construction site. Record
literals no longer have a dedicated checking path: record syntax and an explicit
`Record` application reach one `Application(Constructor(Record), row)`, so both
reach this normalizer with the same kinds. A row equation solves its tails through
`bind_variable`, which now checks the kind as well, so a tail keeps the kind its
row admits and a tail that would acquire another kind is a kind diagnostic at the
binding. The primitive table also owns the per-member `Prim.Row` rules. `Lacks`
and `Union` may solve a known row prefix and re-enter solving on a residual
obligation for an open tail; a known present label makes `Lacks` fail even while
that tail remains open. This uses the same normalizer and kind solver as row
unification. Rigid-tail row unification still has known implementation bugs;
the complete rigid-tail behavior above remains the contract, not a claim of full
coverage. See [primitives](prim.md) for per-relation outcomes and [D-04](../../../decision/DEC-04-official-test-suite-roadmap.md)
for measured coverage.
