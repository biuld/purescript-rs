# Modules and Name Resolution

**Feature:** F-01, F-02

**Status:** Draft

**Prerequisites:** [AST lowering](../syntax/ast-lowering.md),
[frontend boundaries](../00-ir-boundaries.md), lexical scope, and module
systems.

**Summary:** P3 resolves a complete program's module graph, imports, exports,
fixities, and references into stable HIR identities. Compiler-provided `Prim`
interfaces expose existing built-in type identities without source modules or
duplicate type equations. P3 keeps source spans and WIT binding declarations
as metadata, while leaving kinds and types for P5; no unresolved source name
crosses the P3 boundary.

## Scope

This document owns module loading order, namespace lookup, import/export
resolution, local scope, stable IDs, compiler-provided `Prim` interfaces, and
HIR construction. P4 owns operator and term desugaring; [type
checking](../type-system/type-inference.md) owns type validity; backend WIT
validation owns target signatures.

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
TypeReference = Builtin(BuiltinType) | Named(TypeId)
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

`Prim` is a virtual module family. Its root interface maps built-in type names
to the existing `BuiltinType` identities, and `Prim.Coerce` exposes the
compiler-owned `Coercible` class identity. Other official `Prim.*` module names
are recognized by the loader. Import and export interfaces carry a
`TypeReference`, so a built-in name keeps the same identity when it is
qualified, imported, or re-exported. A source module named `Prim` or beginning
with `Prim.` is rejected; source cannot replace a compiler interface.

The virtual interfaces do not synthesize declarations for primitive types or
classes that have no representation in the current shared type spine. Those
names become available when their owning type or class representation is
implemented; the resolver must not imitate them with private equations.

`foreign import` WIT binding text stays attached to the resolved declaration.
The quoted binding is a source string value: a Unicode scalar sequence
([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)). P3
accepts it only when that sequence matches the WIT `<interface>#<function>`
grammar, then stores the validated interface and function names on the
resolved external. An unpaired surrogate or any other malformed binding is
rejected; the compiler does not replace it or invent a different external
name. The declared value name remains an identifier, not a source string
value. P3 validates binding syntax and identity, but not the
canonical ABI or target capability. Fixity declarations attach to resolved
operator IDs; P4 consumes them. Type names are resolved even though kinds and
type applications remain unchecked.

Rejected alternatives: source strings in HIR would force later passes to
repeat lookup; one global namespace would mis-handle same-spelled value and
type names; and target ABI resolution in P3 would leak a backend profile into
the frontend.

## Algorithms

```text
resolve_program(ast_modules):
    reject source modules in the reserved Prim namespace
    index each module by canonical name; reject duplicates
    construct import graph; report missing modules and illegal cycles
    for each module in dependency order:
        register local declarations in disjoint namespaces
        combine source interfaces with virtual compiler interfaces
        compute the visible import environment and exports
        resolve declaration bodies with lexical scope stacks
        attach resolved fixities and validated external binding names
    verify_hir(program)
```

For each binder, allocate one `LocalId` before resolving its scope. A use
checks the innermost local scope, then the permitted module environment.
Duplicate exports, hidden-name uses, and ambiguous imports point to the use or
declaration span and list the competing origins. Modules have implicit
unqualified and qualified access to the root `Prim` interface. Any explicit
`Prim` import suppresses the implicit unqualified access: a selective import
provides only its listed names, and an aliased import is qualified-only. The
implicit `Prim` qualifier remains available unless the source explicitly binds
that qualifier to its own import.

## Code map

`crates/psrs-hir/src/` defines disjoint IDs, `TypeReference`, declarations,
expressions, and `verify::verify_program`. `crates/psrs-resolve/src/resolver/`
owns `resolve_program(modules: &[ast::Module]) -> Result<hir::Program,
Vec<Diagnostic>>`. `program.rs` handles the graph, `program/interface.rs` builds
source and virtual interfaces, `exports.rs` resolves visibility, `names/`
handles lexical and qualified lookup, and `type_resolution.rs` resolves type
references. Source string values and quoted row labels stay scalar sequences;
identifier text and `TextRange` remain separate representations.
`psrs-resolve` converts a WIT binding to interface and function names only
after that validation. `psrs-driver` supplies source modules and displays
diagnostics.

## Invariants and verification

Every reference points to a declaration or existing built-in identity in the
correct namespace, every imported name is exported by its source or virtual
interface, every export is unambiguous, and local uses are in scope. HIR carries
no checked type or runtime layout. Source ranges remain in bounds and refer to
the originating source module. The verifier runs before P4 and P5;
official-suite comparison checks resolution accept/reject and diagnostic
categories.

## Worked example

With `import Lib (value)` and `f x = value x`, P3 resolves `value` to Lib's
`SymbolId`, both occurrences of `x` to one `LocalId`, and `f` to its own
`SymbolId`. Exporting `f` exposes that same ID; it does not copy its body or
assign a backend function index.

With `import Prim as P` and `value :: P.Number`, P3 resolves `P.Number` to the
existing `BuiltinType::Number`. Re-exporting `module P` carries that same
reference through the facade interface.

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

The complete transitive-export rule also checks inferred public value types.
P3 currently checks explicit signatures and constructor results it can identify
directly; general inferred result dependencies need checked type information.

## References

- [Frontend boundaries](../00-ir-boundaries.md),
  [AST lowering](../syntax/ast-lowering.md), and
  [D-01](../../D-01-frontend-and-ir-boundaries.md).
