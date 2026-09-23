# D-14 — PSRS Explorer

**Implements:** [F-03 — PSRS Explorer](../feature/F-03-interactive-ir-explorer.md)
**Status:** Proposed

## Decision

Implement `psrs-explorer/` as a React application written in TypeScript and
built by Vite. React is appropriate because the explorer has several views
whose selections must remain synchronized: route, sample program, selected
pass, representation mode, and source/node selection. Component-local rendering
with a single, explicit explorer state avoids the mutation-heavy DOM code of a
single HTML file. Vite provides a small development server and emits static
assets for deployment; the explorer has no server-side requirement.

This is deliberately not a React-based wrapper around compiler crates. Snapshot
generation remains a Rust-owned build step and the browser reads static data.

## Information architecture

```text
/#/                       Pipeline overview
/#/passes                 Pass index
/#/passes/:passId         One pass and its representation boundary
/#/irs                    Representation index
/#/irs/:representationId  One representation and its contract
/#/trace/:exampleId       A program's synchronized source and snapshots
```

The overview is an architecture map, not a dashboard. Its primary visual is a
representation-to-representation map: representation nodes explain their role,
while pass edges explain the transformation that connects adjacent contracts.
Both nodes and edges are selectable. This guide is the learning-oriented source
for the project's IR vocabulary and pass explanations; it deliberately carries
some responsibility that would otherwise live only in prose design documents.

The representation index is the landing page for the `IRs` navigation. On
desktop it pairs a persistent representation navigator with an index or detail
panel; on narrow screens the navigator becomes a horizontal selection rail.
The IR-detail route is the primary learning page for a representation. It
states the representation's purpose, invariant, required contents, prohibited
contents, why the representation exists, and the passes that produce or consume
it. TokenStream and Structured Wasm are explicitly marked as temporary forms;
the IR detail page prevents them from being mistaken for long-lived families.
For CC IR, MIR, and the temporary structured Wasm form, a source-aligned enum
inventory lists each current variant and its purpose. The backend review beside
it states the current implementation, target design, reason for the change,
and supporting source or design record. Each finding is marked implemented or
remaining; only variants present in Rust appear in the current inventory. The
review follows D-06 and DEC-08 and is updated as backend code evolves.

The pass-detail route has a three-region desktop layout:

1. a narrow pipeline navigator with current position and adjacent passes;
2. a dominant transformation canvas showing input and output forms connected
   by the pass operation;
3. a contract panel containing guarantees, exclusions, documentation, and the
   CLI command that produces the nearest real output.

The pass index opens from the primary `Passes` navigation and lists P0–P11.
Detail routes retain that navigator at the side on desktop. On narrow screens
it becomes a horizontal selection rail, followed by the transformation and
contract. Code and tree panes do not force page-level horizontal scrolling;
each may scroll horizontally within its own labelled region.

The trace route pairs a sample-program navigator with the selected trace on
desktop; the navigator becomes a horizontal selection rail on narrow screens.
The trace content uses one primary source pane and one representation pane
side by side, stacked on narrow screens. A compact stage rail chooses the
displayed snapshot. Selecting either an annotated source range or a
representation node updates one shared selection; the inspector below the
panes explains the mapping. It must never imply a one-to-one mapping where
lowering is many-to-one or synthetic.

## Content model

The browser owns only presentation data. `src/content/passes.ts` defines each
P0–P11 record:

```ts
type Pass = {
  id: string;
  ordinal: number;
  name: string;
  input: RepresentationId;
  output: RepresentationId;
  kind: 'boundary' | 'same-representation' | 'target-encoding';
  purpose: string;
  guarantees: string[];
  exclusions: string[];
  docs: { label: string; href: string }[];
  nearestCommand?: string;
};
```

`src/content/representations.ts` defines family, lifecycle, and invariant
records. TokenStream and Structured Wasm have `lifecycle: 'temporary'`; CST,
AST, HIR, THIR, Typed Core, CC IR, and MIR have the documented long-lived
family memberships. This makes it impossible for the UI to accidentally count
temporary forms as a seventh IR family.

Each snapshot has provenance and mapping information:

```ts
type Snapshot = {
  stage: string;
  provenance: 'generated' | 'curated';
  format: 'tokens' | 'tree' | 'text' | 'graph';
  body: string | TreeNode[] | Graph;
  links: { source: Range; nodeId: string; relation: 'direct' | 'lowered' | 'synthetic' }[];
};
```

The provenance badge is mandatory. Generated snapshot files are immutable build
inputs; curated annotations may explain them but may not be rendered as dumps.

## Component boundary

The application uses a route-level `ExplorerShell` that owns the minimal shared
state: `exampleId`, `passId`, `stageId`, and `selection`. Pages receive the
state through typed props and emit typed actions. The component tree is:

```text
ExplorerShell
├── ArchitectureOverview → ArchitectureMap
├── IrDetail → RepresentationContract, ConnectionList
├── PassDetail → PassNavigator, TransformationCanvas, ContractPanel
└── TraceView → StageRail, SourcePane, RepresentationPane, MappingInspector
```

`TransformationCanvas` uses inline SVG and semantic HTML. It lays out the two
forms and their pass label deterministically; it is not a force-directed graph.
The tree and graph renderers accept only the content-model types above, which
keeps representation-specific rendering out of route components.

## Snapshot generation

A Rust-owned generator in `psrs-explorer/tools/` invokes the existing CLI for
`lex`, `layout`, `parse`, `ast`, `hir`, `check`, and `dump core|cc|mir` over the
checked-in examples. It writes normalized JSON under `src/generated/`. The
generator records compiler revision, command, source path, and output hash.
The frontend build fails if a declared generated snapshot is missing.

P4, P7, P10, and P11 have contract pages and curated transformation diagrams,
because those boundaries are not independently printed by the current CLI. The
pages explicitly say so and link to their nearest observable input or output.

## Visual and interaction rules

- Use one semantic color per pipeline region, plus a persistent selected state;
  color never conveys a family or status without a visible label.
- Render source and output in monospace panes with stable line numbers.
- Use a tree for CST through Typed Core and a basic-block graph only for MIR;
  do not present every IR as an undifferentiated pretty-printed text block.
- Show the raw compiler dump as an alternate view, never the default learning
  view when a structured renderer exists.
- Keep all controls native and keyboard reachable. Source and node selection
  expose their relation and span to assistive technology.
- Persist only user preferences (such as raw versus structured) in the URL;
  snapshot data is never modified in the browser.

## Validation

The frontend package supplies typecheck, unit-test, production-build, and
browser accessibility/route smoke-test commands. Snapshot generation is tested
against the current CLI. Rust workspace validation remains independent because
the explorer does not add a dependency to compiler crates.
