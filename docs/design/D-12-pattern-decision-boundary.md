# D-12 — Pattern Decision Boundary

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Boundary

Typed Core keeps source-oriented patterns so diagnostics and source ranges are
preserved. At the Core-to-CC boundary, `PatternDecision` compiles the ordered
alternatives into branch indices and target-neutral matcher kinds. The decision
step owns first-match ordering and the distinction between constructor, record,
and default alternatives.

CC consumes that decision to build conditional assignments. It owns the target
representation work that follows a selected alternative: representation tests
and casts, field projections, local bindings, and lowering of the branch
expression. The scrutinee is already a CC value and is evaluated once before
the decision is consumed.

## Allowed and forbidden forms

| Boundary | Allowed | Forbidden |
| --- | --- | --- |
| Typed Core | Nested constructors, records, variables, and wildcards with source spans | CC representation IDs or Wasm layouts |
| Pattern decision | Ordered branch indices and constructor/record/default matchers | Field projections, runtime reference types, or branch expressions |
| CC lowering | Representation tests/casts, projections, bindings, and conditional assignments | Reordering source alternatives or re-evaluating the scrutinee |

The decision is an internal boundary object rather than a public long-lived IR.
It is intentionally discarded after CC assignments are produced. This keeps
pattern semantics separate from closure conversion without adding another
serialized compiler representation.

## Invariants

- Alternatives retain source order; the first matching branch wins.
- A default alternative is selected only after earlier alternatives fail.
- Nested pattern checks are emitted inside the selected branch's conditional
  assignments, so their projections cannot evaluate the original scrutinee
  again.
- A branch index always refers to the original Core branch and retains its
  source span for diagnostics.

Regression tests cover ordered decision compilation, nested constructor and
record patterns, and execution through the GC representation planner.
