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
| [F-02: Build portable programs](feature/F-02-portable-programs.md) | [D-02: Wasm lowering](design/D-02-wasm-lowering.md) | Planned |

Decision records use the `DEC-XX` prefix. See [decision policy](decision/README.md).
