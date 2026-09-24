# Functional Core

**Feature:** F-02  
**Status:** Stable (design)  
**Prerequisites:** the call-by-value lambda calculus, algebraic data types and
pattern matching, rank-1 polymorphism (Hindley–Milner/System F style), and a
reading knowledge of administrative normal form and closure conversion. Read
[IR boundaries](../00-ir-boundaries.md) first, then this document, [CC IR](cc-ir.md),
and [MIR](mir.md).  
**Summary:** Functional Core is the typed, call-by-value core calculus that the
backend targets: base scalars, functions, products and records, sums, arrays,
rank-1 polymorphism, and a distinguished effect type. It is independent of
surface syntax and is the last representation that keeps source types and
unresolved names; P8 lowers it to CC through administrative normalization,
closure conversion, and dictionary elaboration.

## Scope

This document owns the Functional Core model — its type and term grammar, its
semantics, the invariants the Core verifier checks, and the responsibilities of
the Core-to-CC lowering at the Core boundary. It is the anchor of the
functional concern: [CC IR](cc-ir.md) is the administrative-normal-form and
closure-converted form of these terms, and [MIR](mir.md) fixes their runtime
representation.

It does not own surface syntax or name/type resolution (those belong to the
frontend in [D-01](../../D-01-frontend-and-ir-boundaries.md)); evaluation order and
closures (see [CC IR](cc-ir.md)); runtime layout (see [MIR](mir.md)); the
pattern decision compilation used when a `case` is lowered (see
[pattern matching](pattern-matching.md)); scalar operator definitions (see
[scalars and primitives](scalars-and-primitives.md)); dictionary passing (see
[type classes and dictionaries](type-classes-and-dictionaries.md)); or the
representation of effects (see [effects](effects.md)).

## Background

**Call-by-value and administrative normal form.** In a call-by-value (strict)
calculus, arguments are evaluated before a function is applied, so evaluation
effects happen in a definite order. *Administrative normal form* (ANF)
restructures a term so that every intermediate computation is bound to a name
and every argument of a call is a value or a variable. ANF was introduced for
compiling with continuations by Flanagan, Sabry, Duba, and Felleisen (1993),
building on Steele's (1978) lambda-lifting work and Appel's *Compiling with
Continuations* (1992). Core is an ordinary expression tree; P8's ANF pass makes
the order of evaluation explicit. See [CC IR](cc-ir.md).

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
[polymorphism and erasure](polymorphism-and-erasure.md).

**Effects.** Squaring a value with effects is the standard monadic translation
(Wadler). The platform-independent `Effect a` type is a distinguished type
former, and effectful code is ordinary monadic code; Core has no ambient side
effects. The final runtime representation is specified in [effects](effects.md),
following Wadler and, if the effect language grows, Levy's call-by-push-value.

## Model

### Types

```text
Type = Variable(TypeVariableId)
     | I32 | F64 | Boolean | String | Char | Unit
     | Constructor(TypeConstructor)
     | Application(TypeId, TypeId)
     | Record([(String, TypeId)])
     | Function { parameter: TypeId, result: TypeId }

TypeConstructor = Array | User(HirTypeId)
```

`TypeId` is an index into `Module.types`. The source-level reading of the
constructors is fixed:

| Source type | Core type |
| --- | --- |
| `Int` | `I32` (signed 32-bit) |
| `Number` | `F64` (IEEE-754 binary64) |
| `Boolean` | `Boolean` |
| `Char` | `Char` (a Unicode scalar) |
| `Unit` | `Unit` (no payload) |
| `a -> b` | `Function { parameter, result }` |
| records and tuples | `Record([(label, TypeId)])`; a tuple is an anonymous product |
| `Array a` | `Application(Constructor(Array), a)` |
| data types | `Constructor(User(HirTypeId))`, optionally applied to arguments |
| `String` | `String`, a platform value supplied by the WASI boundary |

A data type's cases are not part of its `Type`; they are `ConstructorInfo`
records naming a tag, a field count, and field types. A sum is therefore an
ordered set of cases with stable tags, not a nested pair of constructors.

`Effect a` is a distinguished type former in the source and
resolved/typed front-end representations. The type checker elaborates it to the
ordinary function token used by the platform-independent `Effect` library, `Boolean -> a`
(`crates/psrs-typecheck/src/typecheck/signature.rs`), so Core itself needs no
`Effect` type node. The token is an internal execution token, not source-visible
API; the final representation is [effects](effects.md)'s concern.

`Module.newtype_ids` records single-field newtypes that are represented by their
field below Core. Erasing a newtype is representation metadata, not a change to
the source type: the type still exists, but Core lowering and later stages do
not allocate a wrapper for it.

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
in [scalars and primitives](scalars-and-primitives.md).

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
            | Constructor { symbol: SymbolId, arguments: [Pattern] }
            | Record { fields: [(String, Pattern)] }
```

Patterns are nested and source-oriented; compiling them into tests, and checking
exhaustiveness and redundancy, happens at the Core-to-CC boundary
([pattern matching](pattern-matching.md)).

### Semantics

- **Strict.** Functions evaluate their argument before the call; constructors,
  records, and arrays evaluate their elements before allocation.
- **Order is explicit after ANF.** Core terms are expression trees and do not
  encode a particular evaluation order beyond call-by-value. P8 chooses a
  left-to-right order as it binds intermediate computations in ANF, and CC
  records it. Pure terms are order-independent; the order becomes observable
  only once effects exist, which is why ANF, not Core, owns it.
- **Types are erased.** No source type is inspected at runtime. Polymorphism is
  dictionary passing ([type classes and dictionaries](type-classes-and-dictionaries.md));
  the erased representation is [polymorphism and erasure](polymorphism-and-erasure.md).
- **Recursion is the only iteration.** There is no loop term. Iteration is
  tail recursion, made cheap by control-flow lowering
  ([control flow and tail calls](control-flow-and-tail-calls.md)).
- **Patterns are exhaustive.** Coverage and redundancy are established when the
  Core `case` is produced and again by the decision compiler at the boundary.
- **`arrayUpdate` is pure.** It returns an updated array without mutating the
  input or any alias; repeated updates from the same input are independent.

## Design

### The chosen shape

Core is a compact System F(C)-style calculus: first-class functions, products,
sums, arrays, and rank-1 polymorphism, with no dedicated loop, class, or effect
node. This shape is chosen because every later stage has a meaning-preserving
counterpart:

| Core construct | Lowered by P8 to |
| --- | --- |
| `Lambda` / free variables | a lifted function plus explicit captures ([CC IR](cc-ir.md)) |
| `Application` | a direct call (callee is a `Global`) or an indirect closure call |
| `Let` and nested expressions | ordered ANF `Assignment`s |
| `Constructor` / `Case` | variant and product operations, then a pattern decision |
| `Record*` | product construction and field projection |
| `Array*` | array operations |
| rank-1 polymorphism | erased representation requirements and, later, dictionary arguments |
| `Effect a` | its elaborated token type; ordinary function values at runtime |

The full Core-to-CC contract, including the representation requirements that CC
introduces and the operations it owns, is specified in [CC IR](cc-ir.md).

### Lowering responsibilities

`functional-core -> CC` performs three transformations, all of which are
obligations of the P8 pass rather than new Core nodes:

1. **Administrative normalization (ANF).** Name every non-trivial computation in
   evaluation order; every call argument becomes a value or variable.
2. **Closure conversion.** Compute each lambda's captures, lift the lambda to a
   top-level generated function with an explicit closure parameter, and emit a
   function value that pairs the code with its captures.
3. **Dictionary elaboration.** Turn constrained types into functions over
   dictionary records and method selection into field projection. Until the
   frontend produces constraint evidence, this step is vacuous but in place.

`CC -> MIR` then fixes runtime representation; those steps are in
[MIR](mir.md).

### Rejected alternatives

- **Make Core a CPS or ANF language.** Rejected: ANF is a *form* of the same
  program and belongs to CC. Keeping Core as an ordinary expression tree keeps
  source diagnostics and type structure visible and lets ANF be verified
  separately ([CC IR](cc-ir.md)).
- **Represent polymorphism with runtime type tags or `Typeable`-style evidence
  in the terms.** Rejected: the typed core performs no runtime type analysis, so
  tags would add cost with no semantic need; dictionaries cover the instances
  that genuinely need runtime dispatch ([polymorphism and erasure](polymorphism-and-erasure.md)).
- **Add a dedicated loop term for recursion.** Rejected: recursion is already
  expressible and tail-recursion lowering in [control flow](control-flow-and-tail-calls.md)
  recovers looping without enlarging the core.
- **Make `Effect` a Core node.** Rejected for the same reason as a loop node:
  an effect is a value (a token-taking function), and treating it as ordinary
  data keeps CC and MIR free of effect special cases ([effects](effects.md)).
- **Keep surface `where`/guards/view patterns in Core.** Rejected: those are
  surface sugar and patterns; the desugaring and the decision compiler own them
  ([pattern matching](pattern-matching.md)).

## Algorithms

### Typing and generalization

Core is produced already typed by the front-end type checker; the Core pass
itself does not infer. A binding site generalizes the type variables its value
does not constrain; a use site instantiates them. Instantiation is a
substitution over the `Application`/`Variable` structure, and the resulting
expression's `ty` field records the instantiated type. `quantified` records
which variables were generalized.

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

### Lowering to CC

```text
lower_declaration(declaration):
    peel Lambda binders into function parameters
    lower_value(body) under a left-to-right ANF traversal
lift_lambda(lambda):
    captures = free locals of body, in first-use order
    generate a top-level function (closure_parameter, bound_parameter, ...)
    inside, bind each capture from ClosureGetCapture(closure_parameter, index)
    emit FunctionRef(generated, signature, captures)
lower_value(expr):
    for each operand in evaluation order: v = lower_value(operand)
    emit one Assignment naming the result
```

The concrete assignment shapes, capture ordering rules, and the representation
requirements passed to P9 are specified in [CC IR](cc-ir.md).

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
- `psrs-typecheck` owns elaboration from the surface AST to THIR, including the
  `Effect` type former and its elaboration to the platform-independent token
  type.
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
  scrutinee's type; record patterns name declared fields;
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
`fromMaybe`'s Core type is
`Function(Variable(a), Function(Application(Constructor(User(id)), Variable(a)), Variable(a)))`,
with `quantified = [a]`. Its body is a `Lambda` over `d`, a `Lambda` over `m`,
and a `Case` on `m` with two branches; the `Just` branch binds `x` and returns
it.

`main` is `Application(Application(Global(fromMaybe), Integer(0)), Constructor { symbol = Just, arguments = [Integer(42)] })`.
Its `ty` is `I32`, the instantiation of `a` to `Int`.

P8 then peels the two `Lambda`s of `fromMaybe` into function parameters whose
shapes are the erased `a` and the aggregate `Maybe a`, with an erased result.
Because `fromMaybe`'s type is polymorphic, that signature is the erased one; at
`main`'s call, which is fully applied and statically instantiated at `Int`, P8
boxes `0` into the erased representation, passes the `Maybe Int` value as an
aggregate, and unboxes the erased result back to an integer. The `Case` in
`fromMaybe` is lowered through the pattern decision compiler. Dictionary
elaboration is vacuous here because `fromMaybe` is polymorphic but not
constrained. The assignment shapes and the erased-adaptation operations used at
such a call are specified in [CC IR](cc-ir.md).

## Boundaries and interfaces

- **Input.** Typed Core is the stable frontend/backend boundary. The driver
  links and prunes Core declarations and selects the entry `SymbolId`
  ([D-01](../../D-01-frontend-and-ir-boundaries.md)); this document defines the
  calculus but not the front end that produces it.
- **Output.** A verified Core module with an explicit checked type on every
  expression, `quantified` variables at binding sites, source spans, external
  `symbols`, and newtype metadata. It is the only input to P8.
- **To P8 (CC IR).** Core does not fix evaluation order, captures, dictionary
  arguments, or any runtime shape; those are exactly what [CC IR](cc-ir.md)
  establishes.
- **To P9 (MIR).** Core types are not runtime types. Only the erased
  representation requirements that CC derives from them survive
  ([polymorphism and erasure](polymorphism-and-erasure.md)).

## Open questions and future work

- **General effects.** The token type `Boolean -> a` is the current encoding; the
  monadic/CBPV representation in [effects](effects.md) will change how effect
  operations are lowered without adding a Core node.
- **Higher-rank and constraints.** Rank-1 quantification and dictionary
  elaboration are specified; higher-rank types and the exact constraint
  evidence representation remain front-end work
  ([type classes and dictionaries](type-classes-and-dictionaries.md)).
- **Open rows.** Core records are closed. Row polymorphism, when it lands,
  extends `Record` and the record representation rules, not the term grammar.
- **Literal patterns.** `PatternKind` has no literal case. If source guards or
  literal patterns arrive, the decision compiler already has a literal test
  category ([pattern matching](pattern-matching.md)); the Core pattern grammar
  would gain one case.

## Implementation notes

The current front end produces a working subset of this core: monomorphic and
rank-1 polymorphic functions, non-parameterized and a restricted parameterized
ADT slice, closed concrete records, scalar arrays, `if`, `case`, strings, and
the current effect encoding. Local recursive `Let` groups are not yet lowered — only
top-level recursion through `Global` and the generated closure wrappers are —
and constraint evidence, open rows, and the final effect representation are not
yet produced. Nothing in the model above depends on those deviations.

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
