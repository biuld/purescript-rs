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
| [F-01: Source inspection](feature/F-01-source-inspection.md) | [D-01: Frontend and IR boundaries](design/D-01-frontend-and-ir-boundaries.md) | In progress |
| [F-02: Build portable programs](feature/F-02-portable-programs.md) | [D-02: Wasm lowering](design/D-02-wasm-lowering.md), [D-05: Backend capability](design/D-05-backend-capability.md), [D-06: Backend IR boundaries](design/D-06-low-level-ir-and-wasm-types.md), [D-07: WIT imports and std](design/D-07-wit-imports-and-std.md), [D-08: Generic Wasm representation](design/D-08-generic-wasm-representation.md), [D-09: Scalar and numeric lowering](design/D-09-scalar-and-numeric-lowering.md), [D-10: Canonical ABI boundary](design/D-10-linear-memory-representation.md), [D-11: GC representation and evidence](design/D-11-gc-representation-and-evidence.md) | In progress |
| [F-03: PSRS Explorer](feature/F-03-interactive-ir-explorer.md) | [D-14: PSRS Explorer](design/D-14-interactive-ir-explorer.md) | In progress |

Decision records use the `DEC-XX` prefix. See [decision policy](decision/README.md).
