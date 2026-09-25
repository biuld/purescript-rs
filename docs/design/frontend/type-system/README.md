# Frontend Type System

P5 consumes normalized resolved HIR and produces verified THIR. It has four
separate but ordered concerns:

| Document | Owns | Dependency |
| --- | --- | --- |
| [Kinds](kinds.md) | Constructor kinds, synonym expansion, kind legality | resolved HIR |
| [Type inference](type-inference.md) | Schemes, signatures, unification, typed terms | kinds |
| [Classes and evidence](classes-and-evidence.md) | Constraints, instances, dictionaries | type inference |
| [Rows and records](rows-and-records.md) | Row kinds and row unification | kinds and type inference |

The design follows the official PureScript type checker: polymorphic kinds,
higher-rank `forall`, multi-parameter classes with functional dependencies,
instance chains, and row-polymorphic records. `Effect` is a library type, not a
compiler-native effect row. The P5 driver checks kinds before type inference, solves row and class
constraints during inference, and constructs THIR only after all required
evidence has been selected. Inference variables, worklists, and partial
solutions remain private to the checker; THIR contains zonked checked types,
stable resolved IDs, explicit evidence, and source ranges.

[DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md) tracks
implementation coverage. These topic documents define the intended complete behavior. The
[frontend boundary contract](../00-ir-boundaries.md) owns the P4/P5 and P5/P6
handoffs; [Functional Core](../semantics/functional-core.md) owns the later
Core type and term model.
