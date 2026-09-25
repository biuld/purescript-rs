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
```

Rows preserve source label ranges but compare modulo label order. A rigid row variable cannot be extended by unification; an inference unknown may be solved subject to occurs and kind checks. A duplicate label is handled according to the source operation and primitive relation, rather than silently collapsed by a map. Record field types always have kind `Type`; generic primitive row relations can range over `Row k`.

## Design

Normalize rows for comparison into labelled entries and a tail while retaining original ranges. Align common labels and unify their field types. Distribute unmatched entries into open unknown tails with a fresh shared remainder when both sides are open. Reject unmatched entries against a closed or rigid tail. Check record literals against closed or expected open rows, access against a row containing the field, and updates against the official record-update typing rules. Row-based library operations produce or consume class constraints; their primitive solvers share the same row normalizer.

Record subsumption also checks missing or additional fields when one side is closed. Preserve label-specific diagnostics and keep type-level `Symbol` values distinct from runtime strings. Type checking does not choose memory offsets or reorder record expressions for codegen.

Rejected alternatives: positional row equality makes field order observable in types; silently discarding duplicate labels breaks row relations; and treating row polymorphism as an effect system conflicts with PureScript's ordinary record and library model.

## Algorithms

```text
unify_rows(left, right):
    align common labels and unify their field types
    if both remainders empty: unify tails
    if one tail is an unknown: solve it with opposite remainder and tail
    if both tails are distinct unknowns:
        create fresh tail; solve each with the other's remainder and fresh tail
    otherwise reject unmatched labels at their source ranges

solve_primitive_row_constraint(constraint):
    normalize known rows and apply Cons/Lacks/Union/Nub relation
    improve unknowns according to the relation's functional dependencies
    defer only obligations permitted by class entailment
```

Occurs checks traverse field types and tails. A solver must not bind a rigid row variable or invent a duplicate field to satisfy a constraint.

## Code map

`crates/psrs-typecheck/src/typecheck/rows/` owns `row.rs` (`RowView`, `RowEntry`), `unify.rs`, `primitives.rs`, and `records.rs`. Key entry points are `unify_rows(&Type, &Type, &mut InferState) -> Result<(), Diagnostic>`, `solve_row_constraint(&Constraint, &mut InferState) -> Result<Evidence, Diagnostic>`, and `check_record(&hir::Expr, &ExpectedType) -> Result<thir::Expr, Diagnostic>`. THIR stores typed record operations and checked row types; MIR later fixes layouts.

## Invariants and verification

Row equality is independent of source label order; matched fields have equal checked types; tails have the correct `Row k` kind; no unknown or duplicate introduced by the solver escapes THIR. Verify closed/open unification, shared-tail cases, rigid tails, duplicate labels, access/update, `Cons`/`Lacks`/`Union`/`Nub`, and row-based library signatures against official `purs`.

## Worked example

`getX :: forall r. { x :: Int | r } -> Int` accepts `{ x: 1, y: true }` by solving `r` as `(y :: Boolean)`. Calling it with `{ y: true }` fails at label `x`. Reordering the literal's fields does not alter its type. A `Variant (left :: Int | r)` uses the same `Row Type` machinery, while the `Variant` constructor and its operations come from a library.

## Boundaries and interfaces

P5 receives kind-checked row expressions and resolved record operations from HIR. It supplies type equations and evidence to [type inference](type-inference.md) and emits verified THIR. The backend chooses record and variant representation only after Core lowering.

## Open questions and future work

Use official tests to pin down duplicate-label diagnostics, record-update edge cases, and primitive row improvement order. [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md) tracks implementation coverage.

## References

- [PureScript row unification](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Unify.hs), [row primitives](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Environment.hs), and [entailment](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Entailment.hs).
- [Type inference](type-inference.md) and [backend data representation](../../backend/fp/data-representation.md).
