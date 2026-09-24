# Project Documentation

Project documentation is written in English and grouped by purpose:

- `feature/` describes user-visible behavior without implementation details.
- `design/` specifies how a feature is implemented and how compiler
  representations relate. Each design document names its matching feature.
- `decision/` records only major choices that would be costly or confusing to
  reverse. Routine technical details belong in design documents.

## Feature-to-design map

| Feature | Design | Status |
| --- | --- | --- |
| [F-01: Source inspection](feature/F-01-source-inspection.md) | [Frontend and IR boundaries](design/D-01-frontend-and-ir-boundaries.md) | In progress |
| [F-02: Build portable programs](feature/F-02-portable-programs.md) | [Backend design](design/backend/README.md) | In progress |
| [F-03: PSRS Explorer](feature/F-03-interactive-ir-explorer.md) | [PSRS Explorer](design/D-14-interactive-ir-explorer.md) | In progress |

Decision records use the `DEC-XX` prefix. See [decision policy](decision/README.md).

## The backend has two concerns

The backend does exactly two jobs, and every backend document serves one of
them. The backend design lives under [`design/backend/`](design/backend/README.md).

1. **Functional semantics** (`design/backend/fp/`). Represent and execute a
   typed functional core: functions and closures, algebraic data types and
   pattern matching, records and arrays, polymorphism and its erasure, recursion
   and control flow, and later type classes and effects. The theoretical
   backbone is the standard functional pipeline — System F(C) to administrative
   normal form, closure conversion, and an SSA/CFG low IR.
2. **The Wasm/WASI target** (`design/backend/wasm/`). Lower those representations
   to Wasm GC and the WASI 0.2 Component Model: encoding and structuring, the
   capability profile, the canonical ABI and WIT, the linear-memory boundary,
   and the WASI platform library.

The cross-cutting contract is
[Backend IR boundaries](design/backend/00-ir-boundaries.md);
[D-01](design/D-01-frontend-and-ir-boundaries.md) defines the pass pipeline and
[DEC-04](decision/DEC-04-official-test-suite-roadmap.md) tracks compatibility.
Control-flow lowering is the foundation the functional concern builds on.
