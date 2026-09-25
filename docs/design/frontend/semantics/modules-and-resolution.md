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
classes have their own declaration rules. Imports may be qualified, selective,
or hidden, and exports can re-export imported declarations. Stable identities
let later passes refer to declarations without repeating name lookup.

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

`foreign import` WIT binding text stays attached to the resolved declaration.
P3 validates binding syntax and identity, but not the canonical ABI or target
capability. Fixity declarations attach to resolved operator IDs; P4 consumes
them. Type names are resolved even though kinds and type applications remain
unchecked.

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
        resolve declaration bodies with lexical scope stacks
        attach resolved fixities and external binding text
    verify_hir(program)
```

For each binder, allocate one `LocalId` before resolving its scope. A use
checks the innermost local scope, then the permitted module environment.
Duplicate exports, hidden-name uses, and ambiguous imports point to the use or
declaration span and list the competing origins.

## Code map

`crates/psrs-hir/src/` defines disjoint IDs, declarations, expressions, and
`verify::verify_program`. `crates/psrs-resolve/src/resolver/` owns
`resolve_program(modules: &[ast::Module]) -> Result<hir::Program,
Vec<Diagnostic>>`. `program.rs` handles the graph, `exports.rs` visibility,
`names.rs` lexical and qualified lookup, and `type_resolution.rs` type-name
lookup. `psrs-driver` supplies source modules and displays diagnostics.

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
