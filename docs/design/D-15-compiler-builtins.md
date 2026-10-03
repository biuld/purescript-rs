# D-15 — Compiler Built-ins and the Intrinsic Registry

**Implements:** [F-01 — Inspect PureScript Source](../feature/F-01-source-inspection.md)  
**Status:** Draft

## Purpose

Define how the compiler identifies and lowers its built-in value operations —
the intrinsics — so that each semantic fact has one owner and each pass
interprets it in its own representation. The document fixes the boundary between
three kinds of compiler-known name and records why a single cross-layer mechanism
for all of them is rejected.

The identity list already exists as `psrs_hir::Intrinsic`, and it grows as the
standard library lands. The missing piece this document specifies is a single
metadata registry beside that identity, so that a name, an arity, a category, and
a type scheme are not re-derived in every pass.

## Scope

This document owns the intrinsic identity, its metadata registry, and the
contract each pass has for interpreting an intrinsic. It does not own:

- the lowering of any individual intrinsic — that belongs to the pass that emits
  it, and its requirements belong to that pass's design;
- the WIT host-capability path
  ([canonical ABI and WIT](backend/wasm/canonical-abi-and-wit.md),
  [WASI platform library](backend/wasm/wasi-platform-library.md));
- the type-level `Prim.*` relations
  ([primitives](frontend/type-system/prim.md)).

## Background

### Three kinds of compiler-known name

| Kind | Examples | Owner | Crosses the component boundary? |
| --- | --- | --- | --- |
| Intrinsic | `i32.add`, `charToInt`, `arrayAppend`, `stringToBytes`, `unit` | the compiler; emitted inline | no |
| Host capability | `wasi:cli/stdout#get-stdout`, `psrs:effect#bind` | a WIT import or an effect external | yes |
| Type-level function | `Prim.Int.Add`, `Prim.Row.Cons`, `Prim.Symbol.Append` | compile-time only | no runtime form |

Only the first kind is this document's subject. The second is
[DEC-06](../decision/DEC-06-runtime-interface-via-wit.md) and
[DEC-11](../decision/DEC-11-primitive-ffi-stdlib-wrappers.md); the third is the
`Prim.*` hierarchy. An operation belongs to exactly one kind, and the kind
decides the mechanism: intrinsics are emitted as instructions, host capabilities
cross the Canonical ABI, and type-level functions exist only while checking.

### Why intrinsics are compiler-owned here

The official `purs` compiler keeps only types and type-level classes in its
compiler-known `Prim` set (`Prim.Array`, `Prim.Int`, `Prim.Coerce.Coercible`,
`Prim.Row.Cons`, `Prim.Int.Add`, `Prim.Symbol.Append`, `Prim.TypeError.*`, and
the one value `Prim.undefined`). Its value-level operations — `+`, array index,
`Int` bit operations, mutable references — are ordinary `foreign import`s in
library modules, implemented by JavaScript FFI. The optimizer recognizes a fixed
list of qualified library names (`Constants/Libs.hs`) and inlines them into the
target IR's operators (`CoreImp/Optimizer/Inliner.hs`). That split works because
the JavaScript runtime already provides `+` and array indexing.

This project excludes JavaScript and Node.js FFI
([F-02](../feature/F-02-portable-programs.md)), so a value operation that the
source language cannot express and the runtime does not provide must be owned by
the compiler. That is what `Intrinsic` is for. The set is a closed enum of
machine and representation operations, not a place for library functions that can
be written in PureScript.

### How other compilers organize built-ins

Every mature compiler uses the same shape: a closed enum of built-ins, a metadata
table, and a per-stage or per-backend `case`.

| Compiler | Closed enum | Metadata | Per-backend dispatch |
| --- | --- | --- | --- |
| GHC | `GHC.Builtin.PrimOps` (generated) | `compiler/GHC/Builtin/primops.txt.pp` (arity, types, attributes) | `GHC.StgToCmm.Prim` |
| Rust | `rustc_hir::intrinsics` | `library/core/src/intrinsics` plus compiler registration | `rustc_codegen_ssa::intrinsics`, per backend |
| Zig | `BuiltinFn` | one builtin list | a per-backend `lowerBuiltin` switch |
| LLVM | `Intrinsic::ID` (generated) | `Intrinsics.td` | `IntrinsicLowering` |

No compiler puts backend lowering in the metadata table. GHC's `primops.txt.pp`
is the most table-driven: it generates the enum, the tags, and the type
signatures, and `GHC.StgToCmm.Prim` still has a `case` per primop. This document
follows that pattern.

## Model

### Identity

`psrs_hir::Intrinsic` is the closed enum, and `Intrinsic::symbol()` gives a
stable `SymbolId` in the reserved intrinsic module. It is the one authoritative
identity; no stage keeps a second name table for the same operation.

### The registry descriptor

```rust
pub struct IntrinsicDescriptor {
    pub intrinsic: Intrinsic,
    pub name: &'static str,
    pub arity: u8,
    pub category: IntrinsicCategory,
    pub scheme: &'static dyn Fn() -> Type, // an HIR Type
}

pub enum IntrinsicCategory {
    Nullary,          // true, false, unit
    UnaryScalar,
    BinaryScalar,
    ArrayLength,
    ArrayAppend,
    StringToBytes,
    BytesToString,
    Coercion,         // Safe.Coerce.coerce: checked evidence, no runtime op
    PartialValue,     // Prim.undefined
}
```

`category` is a representation-independent classification: it says what kind of
operation it is, not how any one stage emits it. `scheme` is an HIR `Type`, which
can express `forall a. Array a -> Int` as
`Forall { variables: [a], body: Function { parameter: Application(Constructor(Array), Variable("a")), result: Constructor(Int) } }`.
An intrinsic whose typing is not a plain scheme — `Coerce` needs a `Coercible`
wanted, `Undefined` is a fresh variable — carries a `category` that tells the
type checker to take its special path.

### Layer boundary

The registry lives in `psrs-hir` (or a future `psrs-intrinsics` crate that
depends only on `psrs-hir`). Every pass already depends on `psrs-hir`, so no pass
gains a dependency and no cycle is possible. The registry must not depend on
`psrs-core` or the backend: if it held Core lowering, `core` would depend on the
registry and the registry on `core`.

## Design

### One registry, per-stage interpretation

| Stage | Reads from the registry | Produces |
| --- | --- | --- |
| P3 Resolve | `name`, `scheme` | `ExternalSymbol { signature: Some(scheme) }` |
| P5 Type check | `scheme`, instantiated | the intrinsic's `InferType`; `Coercion`/`PartialValue` take a special path |
| P6 Core lowering | `category` | a specialized `ExprKind` or `Primitive`/`UnaryPrimitive` |
| P8/P9 Backend | `category`, plus its own tables | CC `AssignmentKind` and MIR instructions |

The backend does not consult the registry to decide *how* to lower an operation;
it uses its own representation tables. The registry tells every stage *what* the
operation is.

### Why lowering is not in the registry

`arrayAppend` lowers to `array.new_default` plus two element-copy loops; that is
code, not data. A registry that also held lowering would either depend on every
representation crate, creating the cycle above, or become a miniature IR whose
"instructions" are backend operations. The registry holds only the facts that are
independent of every representation; each pass owns its interpretation.

### Specialized Core nodes

Core gives a distinct `ExprKind` to the operations the verifier and the optimizer
reason about structurally — `ArrayLength`, `ArrayAppend`, `StringToBytes`,
`BytesToString` — and uses `Primitive`/`UnaryPrimitive` for the scalar set. This
keeps structural facts structural: `simplify` can fold `arrayLength [a, b]`, and
the verifier checks each node's operand and result types.

The cost is that a new specialized node ripples through every `match ExprKind`
in the Core traversals. A generic `ExprKind::IntrinsicCall { intrinsic, arguments }`
would stop that ripple, but it would move structural reasoning into
per-intrinsic code and lose the fact that a node *is* `array.length`. That
trade-off is recorded under Open questions rather than decided here.

### Rejected alternative: one WIT component for all intrinsics

Declaring every intrinsic as a `psrs:intrinsics` WIT interface, with one
component implementing them, is rejected:

- **It has no base case.** The Canonical ABI lowering is itself written in
  scalar instructions (`i32.add` for an element size, `i32.lt_u` for a loop,
  `i32.store` for a return area). If `i32.add` were an ABI call, lowering the
  call to `i32.add` would need `i32.add`.
- **Representation operations cannot cross the boundary.** `stringToBytes`,
  `arrayAppend`, and `arrayLength` act on the same GC heap. A WIT call only
  carries canonical values, so it would force a GC array through linear memory
  and back, contradicting [DEC-09](../decision/DEC-09-gc-only-language-heap.md)
  and [DEC-16](../decision/DEC-16-scalar-strings-and-utf8-storage.md); and GC
  heaps do not cross component instances at all.
- **Some intrinsics are not functions.** `unit` is a value, `Prim.undefined` is
  a value, and `Coerce` is compile-time evidence with no runtime form. They have
  no `func(...) -> ...` shape.
- **One world defeats capability imports.** A single `psrs:intrinsics` world
  would make every artifact import every operation, where the target profile
  prunes unused capabilities ([DEC-05](../decision/DEC-05-wasmtime-feature-set.md)).

The legitimate use of WIT for a *pure* computation that has no host and no
representation coupling — floating-point math, Unicode case mapping — is a
capability decision recorded separately, not a reason to move the machine layer.

## Algorithms

### Adding an intrinsic

1. Add the `Intrinsic` variant and its `IntrinsicDescriptor`.
2. If the operation needs a specialized Core node, add the `ExprKind` and its
   recognition in `core::lower`; otherwise classify it under `Primitive` or
   `UnaryPrimitive` through its `category`.
3. Add the backend lowering: a CC `AssignmentKind` and, if the existing MIR
   instruction set cannot express it, a MIR instruction with its verifier.
4. Add the focused tests, and the official-suite evidence when the operation is
   corpus-facing.

A variant added without a registry entry, or with a `category` a stage does not
handle, fails to compile through the stage's exhaustive match.

## Code map

```text
crates/psrs-hir/src/
  intrinsic.rs        Intrinsic enum and symbol()
  intrinsics.rs       the registry: descriptors, names, categories, schemes
crates/psrs-resolve/src/resolver/bootstrap.rs
                      builds the ExternalSymbols from the registry
crates/psrs-typecheck/src/typecheck/infer/intrinsics.rs
                      instantiates a descriptor's scheme
crates/psrs-core/src/lower/…
                      recognizes a saturated application into its Core node
crates/psrs-backend/src/cc/…, mir/…
                      per-operation lowering and verification
```

The registry is data. Its consumers are the pass entry points named above.

## Invariants and verification

- One name table and one type scheme per intrinsic. `ExternalSymbol.signature` is
  truthful for an intrinsic, not `None` with the type re-derived in type checking.
- Every stage's match over `Intrinsic` or `IntrinsicCategory` is exhaustive, so a
  new variant cannot be silently unhandled.
- The registry does not depend on any representation crate other than `psrs-hir`.
- Moving the type and category into the registry is behavior-preserving: the L1–L6
  scoreboards must be identical before and after.

## Open questions and future work

- **Generic Core node.** Whether to replace the specialized `ExprKind` array and
  string nodes with one `ExprKind::IntrinsicCall`, trading structural
  verification and optimization for a stable traversal surface.
- **First-class intrinsic references.** An unsaturated reference to an intrinsic
  (for example using `arrayAppend` as a function value) is not lowerable today;
  the library eta-expands instead. Whether the backend should support an
  intrinsic as a closure is open.
- **Pure computations as capabilities.** Which operations — floating-point math,
  Unicode — become a `psrs:math`-style WIT capability rather than intrinsics.
- **Partial application of intrinsic types.** `Undefined` and `Coerce` show that
  a plain scheme is not always enough; the descriptor's `category` is the escape
  hatch, and its scope should stay minimal.

## Implementation notes

The identity list exists (`psrs_hir::Intrinsic`) and the name table exists
(`psrs-resolve`'s `bootstrap_externals`). The type is still hardcoded in
`psrs-typecheck`'s `infer/intrinsics.rs`, and the scalar classification is still
written twice — in Core's `Primitive::from_intrinsic`/`UnaryPrimitive::from_intrinsic`
and in the type checker's branch order. The registry above is the target that
replaces those; this document records the design before that change lands.

## References

- [D-01 — Frontend and IR Boundaries](D-01-frontend-and-ir-boundaries.md).
- [DEC-06 — Runtime Interface via WASI and the Component Model](../decision/DEC-06-runtime-interface-via-wit.md),
  [DEC-11 — Primitive foreign imports and standard-library wrappers](../decision/DEC-11-primitive-ffi-stdlib-wrappers.md),
  [DEC-16 — Scalar Strings and UTF-8 Storage](../decision/DEC-16-scalar-strings-and-utf8-storage.md).
- GHC `compiler/GHC/Builtin/primops.txt.pp`, `GHC.Builtin.PrimOps`,
  `GHC.StgToCmm.Prim`.
- Rust `rustc_hir::intrinsics`, `rustc_codegen_ssa::intrinsics`.
- Zig `BuiltinFn` and its per-backend `lowerBuiltin`.
- LLVM `llvm/IR/Intrinsics.td` and `IntrinsicLowering`.
- `purs`: `src/Language/PureScript/Constants/Prim.hs`,
  `src/Language/PureScript/Constants/Libs.hs`,
  `src/Language/PureScript/CoreImp/Optimizer/Inliner.hs`.
