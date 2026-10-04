# D-15 — Compiler Built-ins and the Intrinsic Registry

**Implements:** [F-01 — Inspect PureScript Source](../feature/F-01-source-inspection.md)  
**Status:** Draft

## Purpose

Define how the compiler identifies and lowers its built-in terms — the intrinsics
— so that each semantic fact has one owner and each pass interprets it in its own
representation. An intrinsic is a compiler-owned global term whose implementation
is irreducible: the source language cannot express it and no host capability
provides it, so the backend must emit it directly from its own representation. It
is a value, a function of fixed arity, or checked evidence; it is not a surface
operator and not an arbitrary literal. The document fixes the boundary between
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
| Intrinsic | `i32Add`, `charToInt`, `arrayAppend`, `stringToBytes`, `unit` | the compiler; emitted inline | no |
| External | `wasi:cli/stdout#get-stdout` (WIT), `psrs:effect#bind` (effect) | a WIT import or a compiler-lowered effect external | only the WIT import |
| Type-level function | `Prim.Int.Add`, `Prim.Row.Cons`, `Prim.Symbol.Append` | compile-time only | no runtime form |

Only the first kind is this document's subject. The second is
[DEC-06](../decision/DEC-06-runtime-interface-via-wit.md) and
[DEC-11](../decision/DEC-11-primitive-ffi-stdlib-wrappers.md); the third is the
`Prim.*` hierarchy. An entry belongs to exactly one kind, and the kind decides
the mechanism: intrinsics are emitted as instructions, a WIT host capability
crosses the Canonical ABI, an effect external is lowered by the compiler, and
type-level functions exist only while checking.

### What counts as an intrinsic

An intrinsic is a *term*: a global name with a stable `SymbolId` whose
implementation is irreducible, so the backend must emit a target instruction or
instruction sequence directly. That is narrower than "anything the compiler
knows":

- A literal is not an intrinsic. `123`, `'a'`, and `"abc"` are uniform encodings
  of built-in types; the compiler handles the type's representation, not each
  value. `true`, `false`, and `unit` are intrinsics only because `Boolean` and
  `Unit` are built-in types with no declaration to attach constructors to, so
  their canonical inhabitants have nowhere else to live.
- A surface operator is not an intrinsic. `+`, `==`, and `<>` are library
  declarations; the descriptor carries the compiler-internal primitive name that
  the library's class instance calls, not the operator spelling or its fixity.
- Not every intrinsic is a function. `unit` is a value, `Prim.undefined` is a
  partial value with no runtime representation, and `Coerce` is checked evidence
  with no runtime form.

### Intrinsics on the Wasm target

For the Wasm target the intrinsics are the compiler's curated vocabulary of
primitive operations at the machine layer — the operations the standard library
is allowed to rely on directly. Each is realized by the backend from one or more
Wasm instructions (scalar, GC `array`/`struct`/`ref`, or bulk memory), from a
short helper sequence, or from no instruction at all. The correspondence is
many-to-many, so the set is not literally a subset of the instruction list:

- One instruction, one intrinsic: `i32.add` for `i32Add`.
- One instruction, several intrinsics: `i32.lt_s` serves `i32LtS` and `charLt`.
- One intrinsic, several instructions: `arrayAppend` is `array.new_default` plus
  two element-copy loops; `intDiv` is `i32.div_s` with a sign-correction helper.
- No instruction: `charToInt` and `intToChar` are the identity; `true`, `false`,
  and `unit` are constants; `Coerce` has no runtime form.

The set is a project decision, not a projection of the instruction list. It holds
exactly the operations the standard library needs that the source language cannot
express and no host capability provides. That places it at the machine layer and
below the Canonical ABI, which is why it cannot be a WIT call; the encoding of
each operation belongs to the backend
([Wasm encoding and structuring](backend/wasm/encoding-and-structuring.md)), not
to this registry.

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
([F-02](../feature/F-02-portable-programs.md)), and its remaining external
mechanism, `foreign import`, is a WIT host-capability import. The JavaScript FFI
is not a precedent for WIT: a JavaScript foreign import is untyped code in the
same runtime and the same heap, so it can implement `+` and array indexing
directly, whereas a WIT call is a typed cross-component boundary that copies
canonical values and cannot see the guest's GC heap.

So a term the source language cannot express and no host capability provides must
be owned by the compiler. That is what `Intrinsic` is for. The set is a closed
enum of machine and representation operations, canonical values, and checked
evidence — not a place for library functions that can be written in PureScript.
The current set lies entirely *below* the Canonical ABI: scalar arithmetic and
conversions are the ABI's own base case, and the array and string operations act
on the guest GC heap. A pure computation with canonical inputs and outputs — a
floating-point function the target has no instruction for, or Unicode case
mapping — is the opposite case and belongs to a WIT capability instead (see Open
questions), not to `Intrinsic`.

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
    /// The compiler-internal primitive name, not a surface operator spelling.
    pub name: &'static str,
    pub arity: u8,
    pub category: IntrinsicCategory,
    /// A non-capturing constructor, so the table can be `const`.
    pub scheme: fn() -> Type, // an HIR Type
}

pub enum IntrinsicCategory {
    Nullary,          // true, false, unit
    UnaryScalar,
    BinaryScalar,
    ArrayLength,
    ArrayIndex,
    ArrayUpdate,
    ArrayAppend,
    StringToBytes,
    BytesToString,
    Coercion,         // Safe.Coerce.coerce: checked evidence, no runtime op
    PartialValue,     // Prim.undefined
}
```

`category` is a representation-independent classification: it says what kind of
term it is, not how any one stage emits it. `scheme` is an HIR `Type`, which can
express `forall a. Array a -> Int` as
`Forall { variables: [a], body: Function { parameter: Application(Constructor(Array), Variable("a")), result: Constructor(Int) } }`.
`arity` is the number of arguments a saturated call supplies: the count of leading
arrows in `scheme`. A stage uses it to tell a saturated intrinsic application from
an unsaturated reference. `category` is the escape hatch for the one intrinsic
whose typing is not a plain scheme: `Coerce` needs a `Coercible` wanted and a
`CoerceFunction` node, so it takes a special path. `PartialValue` needs no special
case — its scheme `forall a. a` already instantiates to the fresh variable the use
decides.

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
| P5 Type check | `scheme`, instantiated | the intrinsic's `InferType`; `Coercion` takes a special path |
| P6 Core lowering | `intrinsic`, `arity`, `category` | one `ExprKind::IntrinsicCall` |
| P8/P9 Backend | its own tables only | CC `AssignmentKind` and MIR instructions |

The backend does not consult the registry: by the time it runs, Core has already
expressed the operation in its own terms, and the backend lowers those with its
own representation tables. The registry tells the front-end stages *what* the
operation is.

### Why lowering is not in the registry

`arrayAppend` lowers to `array.new_default` plus two element-copy loops; that is
code, not data. A registry that also held lowering would either depend on every
representation crate, creating the cycle above, or become a miniature IR whose
"instructions" are backend operations. The registry holds only the facts that are
independent of every representation; each pass owns its interpretation.

### The operation is explicit in Core, not left as a call

An intrinsic becomes an explicit node in Core — one `ExprKind::IntrinsicCall` —
rather than staying a generic call that the backend resolves. This is the industry
pattern: GHC makes a `PrimOp` a `Core` expression, LLVM's front ends emit `llvm.*`
intrinsics into IR, Swift uses SIL `builtin` instructions, and Go uses SSA ops;
each backend then lowers its own set. A target primitive whose optimizations matter
is made explicit in the mid-level IR so constant folding, CSE, and dead-code
elimination can see it. Leaving the operation as a call until codegen is the
pattern for host and runtime services — which is exactly the WIT path, not the
intrinsic path.

Resolving intrinsics in the backend from Core's external table, as WIT calls are
resolved in `mir/wit`, would make the front-end pipeline uniform but would push
every intrinsic optimization down to MIR and route the lowest machine operations
through CC as calls. Correctness would survive, but Core would stop folding and
verifying intrinsic nodes. The project keeps the operation explicit in Core and
keeps the per-stage dispatch.

### One generic Core node

Core represents every intrinsic application with one node,
`ExprKind::IntrinsicCall { intrinsic, arguments }`, replacing the specialized
array and string variants and the scalar `Primitive`/`UnaryPrimitive` pair. This
keeps the traversal surface stable: adding an intrinsic does not add an arm to any
`match ExprKind` in Core's inline, alpha, simplify, or verifier code.

The per-intrinsic handling lives in a per-representation intrinsic module — Core's
verifier and optimizer rules, the CC lowering, and the MIR expansion — each with
one compile-time-exhaustive match over `Intrinsic`. The registry's scheme and
`arity` let the Core verifier check an `IntrinsicCall`'s operands and result in one
rule, and let the optimizer fold `arrayLength [a, b]` through the same module, so
no bespoke `ExprKind` variant is needed per operation. The modules stay split by
representation: no single module knows Core and the backend at once.

The cost is that a traversal no longer enumerates the operations, so the compiler
cannot force each `match ExprKind` to consider a new one; that exhaustiveness
moves to the intrinsic module's match over `Intrinsic`. Structural facts are
reached through the node's `intrinsic` field rather than a dedicated variant,
which is where the intrinsic module reads them.

### Rejected alternative: one WIT component for all intrinsics

Declaring every intrinsic as a `psrs:intrinsics` WIT interface, with one
component implementing them, is rejected:

- **It has no base case.** The Canonical ABI lowering of compound values is
  written in scalar instructions (`i32.add` for an element offset, `i32.lt_u` for
  a loop bound, `i32.store` for a return area). Once an intrinsic that lowering
  needs is itself an ABI call, lowering that call reaches for the same operation,
  so the set as a whole has no consistent floor. A scalar-only call would not
  regress in isolation, but the machine layer cannot be built this way.
- **Representation operations cannot cross the boundary.** `stringToBytes`,
  `arrayAppend`, and `arrayLength` act on the same GC heap. A WIT call only
  carries canonical values, so it would force a GC array through linear memory
  and back, contradicting [DEC-09](../decision/DEC-09-gc-only-language-heap.md)
  and [DEC-16](../decision/DEC-16-scalar-strings-and-utf8-storage.md); and GC
  heaps do not cross component instances at all.
- **Some intrinsics are not functions.** `true`, `false`, and `unit` are values,
  `Prim.undefined` is a value with no runtime representation, and `Coerce` is
  compile-time evidence with no runtime form. They have no `func(...) -> ...`
  shape.
- **One world defeats capability imports.** A single `psrs:intrinsics` world
  would make every artifact import every operation, where the target profile
  prunes unused capabilities ([DEC-05](../decision/DEC-05-wasmtime-feature-set.md)).

The legitimate use of WIT for a *pure* computation that has no host and no
representation coupling — floating-point math, Unicode case mapping — is a
capability decision recorded separately, not a reason to move the machine layer.

### Alternative considered: intrinsics declared in source

Instead of a Rust registry, the standard library could declare each primitive
with a compiler-recognized `foreign import` binding —
`foreign import "psrs:intrinsic#i32Add" i32Add :: Int -> Int -> Int` — and the
backend would emit the instruction rather than call a host. This mirrors the
`purs` split, where the operation is a library declaration the optimizer
recognizes, without a JavaScript runtime.

It is viable but it does not remove the closed set: the backend still needs a
fixed table of operations it can emit, and it must reject a declaration whose
type it cannot lower, which requires the same name-and-scheme metadata. It moves
that metadata into `.purs` and adds a second kind of `foreign import` (compiler
internal rather than WIT). This document keeps the metadata in the Rust registry
for now; moving it into source is recorded under Open questions.

## Algorithms

### Adding an intrinsic

1. Add the `Intrinsic` variant and its `IntrinsicDescriptor`.
2. Add the operation to each representation's intrinsic module that must handle
   it: the Core verifier and optimizer rules, the CC lowering, and any MIR
   expansion. No `match ExprKind` in a Core traversal changes.
3. Add the focused tests, and the official-suite evidence when the operation is
   corpus-facing.

A variant added without a registry entry fails to compile through the registry's
exhaustive `descriptor` match; a representation that does not handle it fails to
compile through its intrinsic module's exhaustive match over `Intrinsic`.

## Code map

```text
crates/psrs-hir/src/intrinsic/
  mod.rs              Intrinsic enum, symbol(), and the ALL list
  registry.rs         IntrinsicCategory, IntrinsicDescriptor, names, categories, schemes
crates/psrs-resolve/src/resolver/bootstrap.rs
                      builds the ExternalSymbols from the registry
crates/psrs-typecheck/src/typecheck/infer/intrinsics.rs
                      instantiates a descriptor's scheme
crates/psrs-core/src/lower/…
                      recognizes a saturated application into `IntrinsicCall`
crates/psrs-core/src/opt, verify/…
                      the Core intrinsic module: folding and type checks
crates/psrs-backend/src/cc/…, mir/…
                      the backend intrinsic modules: lowering and expansion
```

The registry is data. Its consumers are the pass entry points named above.

## Invariants and verification

- One name table and one type scheme per intrinsic. `ExternalSymbol.signature` is
  truthful for an intrinsic, not `None` with the type re-derived in type checking.
- The descriptor names the compiler-internal operation. A surface operator
  spelling and its fixity belong to the library, not to the registry.
- `arity` equals the leading-arrow count of `scheme`; a test over every
  descriptor rejects one where the two disagree.
- Every stage's match over `Intrinsic` or `IntrinsicCategory` is exhaustive, so a
  new variant cannot be silently unhandled.
- The registry does not depend on any representation crate other than `psrs-hir`.
- Moving the type and category into the registry is behavior-preserving: the L1–L6
  scoreboards must be identical before and after.

## Open questions and future work

- **First-class intrinsic references.** An unsaturated reference to an intrinsic
  (for example using `arrayAppend` as a function value) is not lowerable today;
  the library eta-expands instead. Whether the backend should support an
  intrinsic as a closure is open.
- **Pure computations as capabilities.** Which operations — floating-point math,
  Unicode — become a `psrs:math`-style WIT capability rather than intrinsics.
- **Partial application of intrinsic types.** `Coerce` shows that a plain scheme
  is not always enough; the descriptor's `category` is the escape hatch, and its
  scope should stay minimal.
- **Metadata in source.** Whether to declare the primitives in a trusted library
  module with a compiler-recognized `foreign import` binding instead of a Rust
  registry, so intrinsic and WIT externals share one declaration mechanism.

## Implementation notes

The registry and the one generic `ExprKind::IntrinsicCall` are landed. The
identity, name, arity, category, and type scheme have one owner in `psrs-hir`'s
intrinsic module; resolve builds the externals from it, type checking instantiates
its schemes, and each representation's intrinsic module owns the per-operation
verification, folding, and lowering. Core has no per-operation expression node and
no scalar `Primitive`/`UnaryPrimitive` enum.

The surface operators `+`, `*`, `==`, `/=`, `<`, `<=`, `>`, and `>=` are now
library classes — `Data.Semiring`, `Data.Eq`, `Data.Ord` — over internal
primitives, and `Prelude` re-exports them, matching official PureScript. A source
that uses them imports `Prelude`. Each instance eta-expands its intrinsic
(`eq x y = intEq x y`) because a first-class intrinsic reference is not lowerable.

`-` is the `Data.Ring` operator and `/` is the `Data.EuclideanRing` operator,
both re-exported from `Prelude`. The primitives under them are `intSub`
(wrapping subtraction) and `intQuot` (truncating division). `intDiv` and
`intMod` stay the Euclidean pair the `Int` instance calls. `%` is still the
truncating remainder primitive; the library spells that operation `mod`.

`Show` is a library class in `Data.Show`, re-exported from `Prelude`, over the
same primitives. It does not add an intrinsic: integer, character, and string
rendering are written in the source language, and `Number` rendering is too.
That `Number` spelling is not a correctly rounded ECMAScript conversion. A pure
numeric formatter with canonical inputs and outputs is the open capability
question above, not a new `Intrinsic`.

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
