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

## Compiler design

The [frontend design](design/frontend/README.md) groups syntax, semantic
elaboration, and the [PureScript type system](design/frontend/type-system/README.md).
The [backend design](design/backend/README.md) groups functional lowering,
optimization, and the Wasm/WASI target. Their shared pass pipeline is
[D-01](design/D-01-frontend-and-ir-boundaries.md); [DEC-04](decision/DEC-04-official-test-suite-roadmap.md)
tracks compatibility with the official compiler.
