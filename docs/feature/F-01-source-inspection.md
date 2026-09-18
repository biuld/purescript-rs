# F-01 — Inspect PureScript Source

**Status:** In progress  
**Design:** [D-01 — Frontend and IR Boundaries](../design/D-01-frontend-and-ir-boundaries.md)

## User need

Compiler contributors need to see how a source file is interpreted so they
can understand syntax behavior and locate problems.

## User-visible behavior

Given a PureScript source file, the user can request:

- lexical tokens;
- tokens after indentation-based block markers are inserted;
- the parsed concrete syntax tree;
- the normalized surface syntax tree;
- the resolved tree for the supported single-module subset.

The commands are:

```sh
psrs lex path/to/Module.purs
psrs layout path/to/Module.purs
psrs parse path/to/Module.purs
psrs ast path/to/Module.purs
psrs hir path/to/Module.purs
```

Output includes source locations. Invalid input reports a location and exits
with a failure status.

## Current supported subset

The current parser handles module headers, simple value declarations, names,
integer/string/character literals, application, infix operators, lambdas,
conditionals, and local `let` declarations. The normalized tree removes
redundant parentheses and represents grouped function parameters as nested
single-parameter functions. AST names remain unresolved; the HIR command
resolves local and same-module value references. Imports and cross-module
resolution are not implemented. This describes current bootstrap behavior,
not the eventual language-compatibility target.

## Acceptance criteria

- `lex`, `layout`, `parse`, and `ast` accept `examples/basic.purs` and print a
  readable result.
- `hir` accepts `examples/resolved.purs` and prints stable IDs for resolved
  declarations and local references.
- `parse` exposes concrete syntax details and source spans; `ast` exposes the
  normalized tree with source spans; `hir` exposes resolved symbol and local
  IDs while preserving source spans.
- Lexical, parse, or name-resolution errors identify the source location and
  return a non-zero process status.
- Layout blocks, declaration structure, and supported expression forms are
  visible at the appropriate stage.

## Out of scope

This feature does not promise type checking, cross-module package loading,
code generation, or complete PureScript compatibility.
