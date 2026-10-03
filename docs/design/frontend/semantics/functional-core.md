# Functional Core

**Feature:** F-02

**Status:** Stable (design)

**Prerequisites:** the call-by-value lambda calculus, algebraic data types and
pattern matching, polymorphism with higher-rank signatures, and a
reading knowledge of administrative normal form and closure conversion. Read
[D-01](../../D-01-frontend-and-ir-boundaries.md) first, then this document and
[backend IR boundaries](../../backend/00-ir-boundaries.md).

**Summary:** Functional Core is the typed, call-by-value core calculus that the
backend targets: base scalars, functions, products and records, sums, arrays,
checked polymorphism and library-defined abstract types. It is independent of
surface syntax and is the last representation that keeps source types and
resolved declaration and local identities. P8 lowers it to CC through
administrative normalization and closure conversion.

## Scope

This document owns the frontend's Typed Core output: its type and term grammar,
semantics, source information, and verifier. It also states the input guarantees
the backend may rely on. [CC IR](../../backend/fp/cc-ir.md) owns P8 lowering to an
administrative-normal-form, closure-converted representation, and
[MIR](../../backend/fp/mir.md) fixes runtime representation.

It does not own surface syntax or name/type resolution (those belong to the
frontend in [D-01](../../D-01-frontend-and-ir-boundaries.md)); evaluation order and
closures (see [CC IR](../../backend/fp/cc-ir.md)); runtime layout (see [MIR](../../backend/fp/mir.md)); the
pattern decision compilation used when a `case` is lowered (see
[pattern matching](../../backend/fp/pattern-matching.md)); scalar operator definitions (see
[scalars and primitives](../../backend/fp/scalars-and-primitives.md)); dictionary passing (see
[type classes and dictionaries](../../backend/fp/type-classes-and-dictionaries.md)); or the
representation of effects (see [effects](../../backend/fp/effects.md)).

## Background

**Call-by-value and administrative normal form.** In a call-by-value (strict)
calculus, arguments are evaluated before a function is applied, so evaluation
effects happen in a definite order. *Administrative normal form* (ANF)
restructures a term so that every intermediate computation is bound to a name
and every argument of a call is a value or a variable. ANF was introduced for
compiling with continuations by Flanagan, Sabry, Duba, and Felleisen (1993),
building on Steele's (1978) lambda-lifting work and Appel's *Compiling with
Continuations* (1992). Core is an ordinary expression tree; P8's ANF pass makes
the order of evaluation explicit. See [CC IR](../../backend/fp/cc-ir.md).

**Closure conversion.** A lambda that refers to variables from its surrounding
scope is *converted* into a top-level function plus an explicit environment of
the free variables it uses. Those free variables are the lambda's *captures*.
Core keeps nested lambdas and lexical names; closure conversion is the P8
obligation that makes captures explicit.

**Types and erasure.** Core is typed, but types are a compile-time property.
Following the type-erasure semantics studied by Crary, Weirich, and Morrisett
(*Intensional Polymorphism in Type-Erasure Semantics*), the runtime carries no
source-type test: a value's type is known from the expression that consumes it,
so no type tag is needed. Polymorphism is recovered by passing dictionaries
(Wadler and Blott, 1989), never by runtime type tags. See
[polymorphism and erasure](../../backend/fp/polymorphism-and-erasure.md).

**Effects.** Squaring a value with effects is the standard monadic translation
(Wadler). The library-defined abstract `Effect a` type represents effectful
code as ordinary monadic code; Core has no ambient side
effects. The final runtime representation is specified in [effects](../../backend/fp/effects.md),
following Wadler and, if the effect language grows, Levy's call-by-push-value.

## Model

### Types

```text
Type = Variable(TypeVariableId)
     | Constructor(TypeConstructor)
     | Application(TypeId, TypeId)
     | ForAll([TypeVariableId], TypeId)
     | Constrained([TypeId], TypeId)
     | RowEmpty
     | RowExtend(String, TypeId, TypeId)
     | TypeLevelString(String)
     | TypeLevelInt(i64)
     | Skolem(TypeVariableId)

TypeConstructor = Function | Record | Array | Row
                | Int | Number | Boolean | String | Char | Unit
                | User(HirTypeId)
```

`TypeId` is an index into `Module.types`. This is one uniform application spine,
matching the frontend checked-type design and official PureScript's `TypeApp`:
functions and type constructors are both applications, and no variant carries
arrow syntax or inline primitive structure. The source-level reading of the
constructors is fixed:

| Source type | Core type |
| --- | --- |
| `Int` | `Constructor(Int)` (signed 32-bit) |
| `Number` | `Constructor(Number)` (IEEE-754 binary64) |
| `Boolean` | `Constructor(Boolean)` |
| `Char` | `Constructor(Char)` (a Unicode scalar) |
| `Unit` | `Constructor(Unit)` (no payload) |
| `String` | `Constructor(String)`, a platform value supplied by the WASI boundary |
| `a -> b` | `Application(Application(Constructor(Function), a), b)` |
| records and tuples | `Application(Constructor(Record), row)`, a row of `RowEmpty`/`RowExtend`; a tuple is the closed record `{ _1, _2, ... }` |
| `Array a` | `Application(Constructor(Array), a)` |
| data types | `Constructor(User(HirTypeId))`, optionally applied to arguments |

The primitive constructors name the source primitives; their concrete runtime
representation is fixed later at MIR, not in Core.

A data type's cases are not part of its `Type`; they are `ConstructorInfo`
records naming a tag, a field count, and field types. A sum is therefore an
ordered set of cases with stable tags, not a nested pair of constructors.

`Effect a` is `Application(Constructor(User(effect_id)), a)`: an ordinary
imported abstract type constructor applied on the uniform spine. Core has no
`Effect` node, no token type, and no side table that makes this constructor
callable. After Core, one representation lowering replaces each `Effect τ`
value with a closure whose parameter list is the runtime token and whose result
is the lowering of `τ`. That closure is not a source arrow, so curried-arrow
flattening does not absorb a function or a nested effect inside `τ`. The
translation and the execution boundary are specified in
[effects](../../backend/fp/effects.md).

`Module.newtype_ids` records single-field newtypes that are represented by their
field below Core. Erasing a newtype is representation metadata, not a change to
the source type: the type still exists, but Core lowering and later stages do
not allocate a wrapper for it.

`Module.opaque_ids` records foreign data declarations. Their Core type is still
`Constructor(User(HirTypeId))`, and they have no `ConstructorInfo`. The set is
what distinguishes that constructor from an algebraic type and from
`Constructor(Int)`. It is not a runtime layout. See [foreign imports](foreign-imports.md).

### Terms

```text
Expr      = { kind: ExprKind, ty: TypeId, span: TextRange }
ExprKind  = Local(LocalId)
          | Global(SymbolId)
          | Constructor { symbol: SymbolId, arguments: [Expr] }
          | Integer(i32) | Number(String) | Boolean(bool) | String(String) | Char(char)
          | Array { elements: [Expr] }
          | Record { fields: [(String, Expr)] }
          | RecordUpdate { record: Expr, fields: [(String, Expr)] }
          | FieldAccess { record: Expr, field: String }
          | ArrayLength(Expr)
          | ArrayIndex { array: Expr, index: Expr }
          | ArrayUpdate { array: Expr, index: Expr, value: Expr }
          | Primitive { op: Primitive, left: Expr, right: Expr }
          | Application(Expr, Expr)
          | Lambda { binder: Binder, body: Expr }
          | Let { bindings: [Binding], body: Expr }
          | If { condition: Expr, then_branch: Expr, else_branch: Expr }
          | Case { scrutinee: Expr, branches: [CaseBranch] }

Primitive = Add | Sub | Mul | DivS | RemS
          | Eq | Ne | LtS | LeS | GtS | GeS
```

Every expression carries its checked `TypeId`, so a Core term is well-typed by
construction. `Primitive` is the small integer comparison/arithmetic set that
survives the surface-language `Prim` desugaring; the full scalar set is defined
in [scalars and primitives](../../backend/fp/scalars-and-primitives.md).

### Bindings and declarations

```text
Declaration = { symbol: SymbolId, name: String, name_span: TextRange,
                quantified: [TypeVariableId], ty: TypeId,
                value: Expr, span: TextRange }
Binding     = { binder: Binder, quantified: [TypeVariableId],
                value: Expr, span: TextRange }
Binder      = { id: LocalId, name: String, ty: TypeId, span: TextRange }
```

`Declaration` is a top-level binding; `Global(SymbolId)` refers to one.
Top-level recursion is expressed by a `Global` reference to a declaration in
the same module, so a recursive function is an ordinary function whose body
mentions its own symbol. A `Binding` in `Let` may similarly be recursive. A
binding's `quantified` variables are the type variables generalized at that
binding site; Core keeps them for lowering and diagnostics but the runtime does
not carry them.

### Patterns

```text
Pattern     = { kind: PatternKind, ty: TypeId, span: TextRange }
PatternKind = Wildcard
            | Var { id: LocalId, ty: TypeId }
            | Literal { value: Literal }
            | Array { elements: [Pattern] }
            | Named { id: LocalId, pattern: Pattern }
            | Constructor { symbol: SymbolId, arguments: [Pattern] }
            | Record { fields: [(String, Pattern)] }

Literal     = Integer(i32) | Number(String) | String(String) | Char(char)
            | Boolean(bool)
```

Patterns are nested and source-oriented; compiling them into tests, and checking
exhaustiveness and redundancy, happens at the Core-to-CC boundary
([pattern matching](../../backend/fp/pattern-matching.md)).
Typed source patterns have already been checked and are removed before Core.
The Core verifier checks that literal payloads agree with their pattern type,
Number text parses to a finite value, array elements have the array's element
type, and a named binder has a unique local ID in the branch scope. A named
pattern binds its current scrutinee before checking its nested pattern.
P5 consumes the HIR record-pattern mode: source record patterns require their
named fields and can retain an open row tail, while tuple and generated product
patterns require an exact closed row. Core needs no separate mode because the
checked `Pattern.ty` and the scrutinee type carry that record shape.

### Semantics

- **Strict.** Functions evaluate their argument before the call; constructors,
  records, and arrays evaluate their elements before allocation.
- **Left-to-right evaluation.** Core expression trees have a defined evaluation
  order: evaluate the callee before its argument, and operands, constructor
  fields, record fields, array elements, and `let` bindings in source order.
  `If` and `Case` evaluate only the selected branch after their condition or
  scrutinee. P8 records this order in ANF; it does not choose a new order.
- **Types are erased.** No source type is inspected at runtime. Polymorphism is
  dictionary passing ([type classes and dictionaries](../../backend/fp/type-classes-and-dictionaries.md));
  the erased representation is [polymorphism and erasure](../../backend/fp/polymorphism-and-erasure.md).
- **Recursion is the only iteration.** There is no loop term. Iteration is
  tail recursion, made cheap by control-flow lowering
  ([control flow and tail calls](../../backend/fp/control-flow-and-tail-calls.md)).
- **Patterns are exhaustive.** Coverage and redundancy are established when the
  Core `case` is produced and again by the decision compiler at the boundary.
- **`arrayUpdate` is pure.** It returns an updated array without mutating the
  input or any alias; repeated updates from the same input are independent.

## Design

### The chosen shape

Core is a compact System F(C)-style calculus: first-class functions, products,
sums, arrays, and elaborated polymorphism, with no dedicated loop, class, or effect
node. This shape is chosen because every later stage has a meaning-preserving
counterpart:

| Core construct | Lowered by P8 to |
| --- | --- |
| `Lambda` / free variables | a lifted function plus explicit captures ([CC IR](../../backend/fp/cc-ir.md)) |
| `Application` | a direct call (callee is a `Global`) or an indirect closure call |
| `Let` and nested expressions | ordered ANF `Assignment`s |
| `Constructor` / `Case` | variant and product operations, then a pattern decision |
| `Record*` | product construction and field projection |
| `Array*` | array operations |
| polymorphism and constraints | erased type arguments and explicit dictionary arguments |
| `Effect a` | a representation closure produced after Core; not a source arrow |

The full Core-to-CC contract, including the representation requirements that CC
introduces and the operations it owns, is specified in [CC IR](../../backend/fp/cc-ir.md).

### Backend consumption

Core's grammar leaves these transformations to P8; they introduce no new Core
nodes and are owned by [CC IR](../../backend/fp/cc-ir.md):

1. **Administrative normalization (ANF).** Name every non-trivial computation in
   evaluation order; every call argument becomes a value or variable.
2. **Closure conversion.** Compute each lambda's captures, lift the lambda to a
   top-level generated function with an explicit closure parameter, and emit a
   function value that pairs the code with its captures.
3. **Dictionary consumption.** Receive the explicit dictionary evidence
   elaborated by the frontend and lower it as ordinary values and projections.

`CC -> MIR` then fixes runtime representation; those steps are in
[MIR](../../backend/fp/mir.md).

### Rejected alternatives

- **Make Core a CPS or ANF language.** Rejected: ANF is a *form* of the same
  program and belongs to CC. Keeping Core as an ordinary expression tree keeps
  source diagnostics and type structure visible and lets ANF be verified
  separately ([CC IR](../../backend/fp/cc-ir.md)).
- **Represent polymorphism with runtime type tags or `Typeable`-style evidence
  in the terms.** Rejected: the typed core performs no runtime type analysis, so
  tags would add cost with no semantic need; dictionaries cover the instances
  that genuinely need runtime dispatch ([polymorphism and erasure](../../backend/fp/polymorphism-and-erasure.md)).
- **Add a dedicated loop term for recursion.** Rejected: recursion is already
  expressible and tail-recursion lowering in [control flow](../../backend/fp/control-flow-and-tail-calls.md)
  recovers looping without enlarging the core.
- **Make `Effect` a Core node.** Rejected for the same reason as a loop node:
  an effect is an abstract value in Core. One later translation produces its
  closure, so CC and MIR never match on the library type
  ([effects](../../backend/fp/effects.md)).
- **Keep surface `where`/guards/view patterns in Core.** Rejected: those are
  surface sugar and patterns; the desugaring and the decision compiler own them
  ([pattern matching](../../backend/fp/pattern-matching.md)).

## Algorithms

### Typing and generalization

Core is produced already typed by the front-end type checker; the Core pass
itself does not infer. A binding site generalizes the type variables its value
does not constrain; a use site instantiates them. Instantiation is a
capture-avoiding substitution over the type structure, including nested
`ForAll` binders, and the resulting
expression's `ty` field records the instantiated type. `quantified` records
which variables were generalized. A nested `ForAll` belongs to its value
type rather than the enclosing declaration scheme. An expression introducing
such a universal binds its type variables in the checked subtree; optimization
must preserve that scope when replacing the expression.

### Core verification

```text
verify_module(module):
    build globals = declarations -> checked type, externals -> unknown
    for each type in module.types: verify_type
    for each declaration:
        verify_type(declaration.ty)
        verify_expr(value, expected = declaration.ty, scope = {})
```

`verify_expr` walks the tree, maintaining a `LocalId -> TypeId` scope and
checking each node's `ty` against its operands and its expected type. See
[Invariants and verification](#invariants-and-verification).

## Code map

The functional core is a frontend representation, so its code lives in the
frontend crates, not in `psrs-backend`. The type checker elaborates the surface
language into the typed IR, `psrs-thir` defines that typed IR, and `psrs-core`
defines Typed Core and the Core-to-CC input boundary. The core representation is
independent of surface syntax: it keeps checked types, resolved `SymbolId`s and
`LocalId`s, and source spans, but no CST or AST node may appear in it.
`psrs-backend` consumes only `psrs-core` types and must not depend on any
frontend CST or AST type.

The required modules and the values they provide are:

- `psrs-thir` owns the typed IR that the type checker produces and that Typed
  Core is elaborated from. It provides `Type`, `TypeId`, `TypeConstructor`,
  `Expr`, `ExprKind`, `Primitive`, `ConstructorInfo`, `Declaration`, `Binding`,
  and `Binder`.
- `psrs-typecheck` checks `Effect` through its imported kind and declarations,
  using the same type rules as for other abstract type constructors.
- `psrs-core` owns Typed Core and the P7 boundary. It provides:
  - `Module`, `Type`, `Expr`, `ExprKind`, `Primitive`, `ConstructorInfo`,
    `Declaration`, `Binding`, and `Binder` at the crate root;
  - `Pattern` and `PatternKind` for the source-oriented pattern form;
  - `Module::verify(&self) -> Result<(), Vec<VerifyError>>`, the P8 input
    verifier, with `VerifyError` retaining a source span;
  - `link(modules: Vec<Module>) -> Module` and
    `prune_unreachable(module: &mut Module, root: SymbolId)`, which produce the
    linked, pruned Core module and its entry `SymbolId`;
  - the Core-to-CC lowering input type consumed by `cc::lower_module` and
    `cc::lower_module_with_bindings`.

## Invariants and verification

The Core verifier (`Module::verify`) checks that a produced module is internally
consistent before P8 consumes it:

- every `TypeId` is inside `Module.types`, and every referenced `TypeId` is
  valid;
- `Local` references are in scope, and `Global` references name a declaration
  or external;
- `Array*` expressions have an array type, `FieldAccess`, `Record*` and record
  patterns have a record type, and fields exist in that type;
- record construction has exactly the fields its type declares, and record
  update names only declared fields;
- constructor applications are saturating, have the constructor's field count,
  and produce the constructor's parent type;
- pattern constructors are declared, have matching arity, and belong to the
  scrutinee's type; record patterns name declared fields; literals match their
  primitive type; array patterns match an array and each element pattern has
  its element type; named binders are unique and scoped to their branch;
- `Application` targets a function type, `Lambda` has a function type, and both
  branches of `If` match the expression's type.

Verification failure is a compiler bug or an unsupported program; it is reported
with a source span rather than being passed to P8.

## Worked example

Take a rank-1 polymorphic function over a sum with a nested payload:

```purescript
data Maybe a = Nothing | Just a

fromMaybe :: forall a. a -> Maybe a -> a
fromMaybe = \d m -> case m of
  Nothing -> d
  Just x  -> x

main :: Int
main = fromMaybe 0 (Just 42)
```

The `Maybe` type is `Constructor(User(id))` with two `ConstructorInfo` records:
`Nothing` (tag `0`, no fields) and `Just` (tag `1`, one field of
`Variable(a)`). `Maybe a` is `Application(Constructor(User(id)), Variable(a))`.
`fromMaybe`'s Core type is the application spine

```text
Application(
  Application(Constructor(Function), Variable(a)),
  Application(
    Application(Constructor(Function), Application(Constructor(User(id)), Variable(a))),
    Variable(a)))
```

with `quantified = [a]`. Its body is a `Lambda` over `d`, a `Lambda` over `m`,
and a `Case` on `m` with two branches; the `Just` branch binds `x` and returns
it.

`main` is `Application(Application(Global(fromMaybe), Integer(0)), Constructor { symbol = Just, arguments = [Integer(42)] })`.
Its `ty` is `Constructor(Int)`, the instantiation of `a` to `Int`.

P8 then peels the two `Lambda`s of `fromMaybe` into function parameters whose
shapes are the erased `a` and the aggregate `Maybe a`, with an erased result.
Because `fromMaybe`'s type is polymorphic, that signature is the erased one; at
`main`'s call, which is fully applied and statically instantiated at `Int`, P8
boxes `0` into the erased representation, passes the `Maybe Int` value as an
aggregate, and unboxes the erased result back to an integer. The `Case` in
`fromMaybe` is lowered through the pattern decision compiler. Dictionary
elaboration is vacuous here because `fromMaybe` is polymorphic but not
constrained. The assignment shapes and the erased-adaptation operations used at
such a call are specified in [CC IR](../../backend/fp/cc-ir.md).

## Boundaries and interfaces

- **Input.** P6 elaborates checked THIR into Typed Core. The driver links and
  prunes declarations and selects the entry `SymbolId`
  ([D-01](../../D-01-frontend-and-ir-boundaries.md)).
- **Output.** A verified Core module for P7 with an explicit checked type on every
  expression, `quantified` variables at binding sites, source spans, external
  `symbols`, and newtype metadata. P7 preserves this contract for P8.
- **To P8 (CC IR).** Core fixes evaluation order and carries explicit dictionary
  evidence. It leaves captures and runtime requirements to
  [CC IR](../../backend/fp/cc-ir.md).
- **To P9 (MIR).** Core types are not runtime types. Only the erased
  representation requirements that CC derives from them survive
  ([polymorphism and erasure](../../backend/fp/polymorphism-and-erasure.md)).

## Open questions and future work

- **General effects.** Core keeps the abstract `Effect` application. The
  representation closure and token are specified in
  [effects](../../backend/fp/effects.md) and are not a Core type.
- **Higher-rank and constraints.** Nested quantifiers and elaborated
  dictionaries retain the checking rules specified by the frontend.
  Remaining official-suite compatibility work is tracked by FE-18
  ([classes and evidence](../type-system/classes-and-evidence.md)).
- **Open rows.** Source row polymorphism is checked by P5. Core preserves the
  checked record type and evidence; P9 fixes concrete record layouts at each
  reachable use without changing the term grammar.
- **Pattern forms not accepted by this slice.** Views and or-patterns remain
  outside the Core pattern grammar. Guards are elaborated by the frontend into
  branch coverage and fallthrough structure before Core.

## References

- Appel, *Compiling with Continuations* (1992).
- Flanagan, Sabry, Duba, and Felleisen, *The Essence of Compiling with
  Continuations* (1993).
- Steele, *Rabbit: A Compiler for Scheme* (1978).
- Wadler and Blott, *How to Make Ad-hoc Polymorphism Less Ad Hoc* (1989).
- Wadler, *The Essence of Functional Programming* (1992).
- Levy, *Call-by-Push-Value* (2003).
- Crary, Weirich, and Morrisett, *Intensional Polymorphism in Type-Erasure
  Semantics* (2002).
- Augustsson, *Compiling Pattern Matching* (1985); Maranget, *Warnings for
  Pattern Matching* (2007) and *Compiling Pattern Matching to Good Decision
  Trees* (2008).
- [D-01 Frontend and IR Boundaries](../../D-01-frontend-and-ir-boundaries.md),
  [DEC-01](../../../decision/DEC-01-distinct-ir-boundaries.md).

## Implementation notes

The current frontend produces nested `ForAll` types, quantified binding
schemes, and dictionary applications. Core lowering, linking, verification,
and optimization preserve these scopes and each reference's checked
instantiation. Source and malformed-IR coverage is recorded in the
[rank-N acceptance record](../../../implementation/frontend/rank-n.md);
[D-04](../../D-04-suite-roadmap.md) owns the wider implementation matrix.

The uniform application spine in the Model is implemented for the nodes the
current frontend produces: functions, primitives, records, rows, arrays, and
user data types all use `Constructor`/`Application` with `RowEmpty`/`RowExtend`
rows, and the ad-hoc `Function`, `Record`, `OpenRecord`, and inline primitive
variants are gone ([DEC-15](../../../decision/DEC-15-unified-type-representation.md)).
An open row is `Application(Constructor(Record), Variable)`, so closure
conversion still rejects it when it cannot choose a field layout. `Constrained`
is discharged into explicit dictionary arguments before THIR, which is why it is
absent from this Model even though the frontend inference model includes it.
The frontend spine's `TypeLevel*` and `Skolem` nodes remain part of the
semantic design. PureScript source syntax has no explicit kind-application
form, though its kind checker inserts internal `KindApp` nodes when ordinary
type application implicitly instantiates a polymorphic kind. This compiler's
kind solver performs that instantiation without retaining a
`KindApplication` in the source type spine, so the omission is not a source
compatibility gap. Current nested constraints elaborate to dictionary arrows
beneath `ForAll`, and solver skolems leave the frontend as scoped quantified
variables.
