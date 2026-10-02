# Modules and Name Resolution

**Feature:** F-01, F-02

**Status:** Draft

**Prerequisites:** [AST lowering](../syntax/ast-lowering.md),
[frontend boundaries](../00-ir-boundaries.md), lexical scope, and module
systems.

**Summary:** P3 resolves a complete program's module graph, imports, exports,
fixities, and references into stable HIR identities. It keeps source spans and
WIT binding declarations as metadata, while leaving kinds and types for P5.
No unresolved source name crosses the P3 boundary.

## Scope

This document owns module loading order, namespace lookup, import/export
resolution, local scope, stable IDs, and HIR construction. P4 owns operator
and term desugaring; [type checking](../type-system/type-inference.md) owns
type validity; backend WIT validation owns target signatures.

## Background

Textual names are meaningful only within a module environment and lexical
scope. A value name can coincide with a type name, while constructors and
classes have their own declaration rules. Value and type operators occupy
their respective namespaces; aliases resolve to the same declaration identity
as their target and carry associativity and precedence into HIR. Imports may be
qualified, selective, or hidden, and exports can re-export imported
declarations. Stable identities let later passes refer to declarations
without repeating name lookup.

## Model

```text
ModuleKey = canonical module name
SymbolId = stable value declaration identity within a linked program
TypeId | ConstructorId | ClassId | LocalId = disjoint identity spaces
ResolveEnv = { modules, imports, value_ns, type_ns, ctor_ns, class_ns, fixities }
HIR = { declarations, resolved references, imports, exports, spans }
```

An ID is stable across passes and a deterministic build of the same program;
it is not promised to survive arbitrary source edits. A local ID is scoped to
its owning declaration. A reference records both its resolved ID and its
source range. The module environment records exported visibility separately
from declarations, so re-exports do not clone identities.

## Design

The driver loads the transitive source graph, checks duplicate module names and
cycles according to the source language's module rules, then resolves modules
in dependency order. P3 first registers declarations and their namespaces,
then resolves bodies so same-module references and recursive groups can name
their final IDs. Qualified lookup uses only the named imported module;
unqualified lookup combines local declarations and permitted imports and
rejects ambiguity.

P3 resolves every value and type fixity target in its own namespace, binds the
operator alias to the target's `SymbolId` or `TypeId`, and stores associativity
and precedence on the resolved module. Expression, constructor-pattern, and
type operator chains keep their source order through P3. P3 does not
re-associate them; P4 consumes their identities and fixities. Import and export
resolution preserves aliases as aliases of the original declaration,
including through re-exports. Unqualified fixity targets use the same ambiguity
checks as ordinary references, and qualified targets resolve through the
named import and its alias.

`foreign import` WIT binding text stays attached to the resolved declaration.
The quoted binding is a source string value: a Unicode scalar sequence
([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)). P3
accepts it only when that sequence matches the WIT `<interface>#<function>`
grammar, then stores the validated interface and function names on the
resolved external. An unpaired surrogate or any other malformed binding is
rejected; the compiler does not replace it or invent a different external
name. The declared value name remains an identifier, not a source string
value. P3 validates binding syntax and identity, but not the canonical ABI or
target capability. Type names are resolved even though kinds and type
applications remain unchecked.

Rejected alternatives: source strings in HIR would force later passes to
repeat lookup; one global namespace would mis-handle same-spelled value and
type names; and target ABI resolution in P3 would leak a backend profile into
the frontend.

## Algorithms

```text
resolve_program(ast_modules):
    index each module by canonical name; reject duplicates
    construct import graph; report missing modules and illegal cycles
    for each module in dependency order:
        register local declarations in disjoint namespaces
        compute the visible import environment and exports
        resolve fixity targets and bind value/type operator aliases
        resolve declaration bodies with lexical scope stacks
        attach resolved fixities and validated external binding names
    verify_hir(program)
```

For each binder, allocate one `LocalId` before resolving its scope. A use
checks the innermost local scope, then the permitted module environment.
Duplicate exports, hidden-name uses, and ambiguous imports point to the use or
declaration span and list the competing origins.

## Code map

The `psrs-hir` organization defines disjoint IDs, declarations, resolved
fixities, and `verify::verify_program`. The `psrs-resolve` resolver separates
program-graph planning, import/export visibility, lexical and qualified name
lookup, fixity target binding, and type-name lookup around
`resolve_program(modules: &[ast::Module]) -> Result<hir::Program,
Vec<Diagnostic>>`. Source string values and quoted row labels stay scalar
sequences; identifier text and `TextRange` remain separate representations.
`psrs-resolve` converts a WIT binding to interface and function names only
after that validation. `psrs-driver` supplies source modules and displays
diagnostics.

## Invariants and verification

Every reference points to a declaration in the correct namespace, every
imported name is exported by its source module, every export is unambiguous,
and local uses are in scope. HIR carries no checked type or runtime layout.
Source ranges remain in bounds and refer to the originating source module.
The verifier runs before P4 and P5; official-suite comparison checks resolution
accept/reject and diagnostic categories.

## Worked example

With `import Lib (value)` and `f x = value x`, P3 resolves `value` to Lib's
`SymbolId`, both occurrences of `x` to one `LocalId`, and `f` to its own
`SymbolId`. Exporting `f` exposes that same ID; it does not copy its body or
assign a backend function index.

## Boundaries and interfaces

P3 consumes verified AST modules and yields resolved HIR to P4. The driver
passes the complete module graph, not just the file under inspection. `psrs
hir` prints IDs and source ranges. P5 trusts the reference identities but
still checks kind, type, and class constraints.

## Open questions and future work

Package-qualified identity and incremental cache keys need a durable module
key policy. Orphan and overlapping instance visibility is specified with
[classes and evidence](../type-system/classes-and-evidence.md), not by value
lookup alone.

## References

- [Frontend boundaries](../00-ir-boundaries.md),
  [AST lowering](../syntax/ast-lowering.md), and
  [D-01](../../D-01-frontend-and-ir-boundaries.md).
