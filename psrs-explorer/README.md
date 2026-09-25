# PSRS Explorer

This directory will contain the React, TypeScript, and Vite implementation of
the interactive compiler-representation explorer. Its product and technical
design are [F-03](../docs/feature/F-03-interactive-ir-explorer.md) and
[D-14](../docs/design/D-14-interactive-ir-explorer.md).

The explorer is intentionally separate from compiler crates. A Rust-owned
snapshot generator records real CLI output for the checked-in examples; the
browser consumes that static data and never pretends curated teaching content is
live compiler output.

## Development

```sh
npm install
npm run dev
npm run test
npm run build
```

`npm run build` runs TypeScript checking before producing the static `dist/`
directory. The current application deliberately labels its compact prototype
snapshots as curated; generated CLI snapshots are the next implementation step.

Open the address printed by `npm run dev`; do not open the source
`index.html` directly. The application uses hash URLs such as
`/#/passes/p3-resolve`, so navigation also works from a static build without
server-side route rewrites.
