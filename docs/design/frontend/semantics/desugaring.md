# Resolved HIR Desugaring

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [modules and resolution](modules-and-resolution.md),
[frontend boundaries](../00-ir-boundaries.md), and source evaluation order.

**Summary:** P2 retains equation and guard forms while normalizing case arity:
multiple scrutinees become one closed record product with `_1`, `_2`, and
subsequent fields, and `_` scrutinees become generated function inputs. P3
assigns those names stable IDs and resolves unary minus through the ordinary
in-scope `negate` name. P4 rewrites surface constructs into a smaller resolved
HIR, applying fixities, lowering unary minus, guards, equations, `do`/`ado`,
and `where` scope while preserving source order and origins for P5 diagnostics.

The generated case and equation products retain an explicit `MatchProduct`
expression through P3 and P4, with exact record patterns. Each field is an
independent scrutinee, so P5 preserves a local field's structural `forall`
unless the corresponding source patterns require a monotype. P5 converts the
checked product into an ordinary typed record with a closed row. A source
record literal still instantiates its field expressions normally; its fields
must not acquire independent pattern-scrutinee polymorphism. A source record pattern such as
`{ field }` remains partial and may leave the row tail open. This distinction
keeps compiler-created products concrete without changing source record
matching.

## Scope

This document owns same-representation, pre-typecheck term normalization. It
does not infer types, resolve new source names, choose pattern decision trees,
or introduce runtime representations. P5 checks the result; P6 removes the
remaining high-level forms on the way to Core.

## Background

Surface notation can describe the same operation in many forms. Fixity is
known only after P3 resolves operators. P2 converts case arity to a record
product without introducing a library tuple dependency; each source scrutinee
is a field expression in its original order. P2 retains guards and equation
alternatives for name resolution. Unary minus is syntax for applying the
ordinary `negate` value, so P3 resolves that name with the same local and import
rules as any other value. `do` describes ordered binds, and guarded equations
describe ordered alternatives. Lowering these before type inference gives the
checker fewer term forms while retaining the user's declaration and
subexpression ranges.

## Model

```text
P4 : ResolvedHIR -> ResolvedHIR
Origin = { source: SourceId, range: TextRange, generated_from: NodeId }
```

The output uses the same HIR IDs and type-expression forms. Generated local
binders receive fresh IDs in the proper scope and an origin range. P4 consumes
resolved value, constructor-pattern, and type operator chains, applying their
associated fixities before lowering them. Resolved type-operator heads preserve
their `Builtin`, `Named`, or `Opaque` identity, including imported foreign type
identity and built-in `Prim.Function` and `Prim.Int`. The normalized subset
contains applications, lambdas, `let`,
conditionals, cases, records, and primitive declarations, with operator,
unary-minus, and sequencing sugar expanded. Pattern syntax may remain for P5
and P6.

## Design

P4 applies resolved fixities to expression, constructor-pattern, and type
operator chains. It reassociates each chain by precedence and associativity,
then lowers value operators to applications, constructor operators to patterns,
and type operators to type applications. Resolved type-operator heads retain
their `Builtin`, `Named`, or `Opaque` identity, including imported foreign type
identity and the built-in `Prim.Function` and `Prim.Int` constructors. Sections
become lambdas whose bodies apply the
resolved operator to the saved operand and the new parameter in source order.
For unary minus, P3 has already resolved `negate` as either a local or global
value; P4 emits an application of that reference to the operand. Other
syntactic operators expand to applications of their resolved identity. P4
lowers `do` to `bind`/`pure` applications and `ado` to its applicative form
using the resolved library identities, never matching a name's spelling.
If one unparenthesized chain uses operators of the same precedence with mixed
associativity, or repeats non-associative operators at that precedence, P4
reports the ambiguity at an operator span and requires parentheses. Parentheses
form separate chains, so an inner group is validated independently.
It converts multiple equations and guarded right-hand sides to ordered cases
and conditions with explicit fallthrough, and makes `where` bindings explicit
in their original lexical scope.

Boolean `true` guards are unconditional only when their resolved symbol is a
compiler Boolean-true intrinsic or a whole-program declaration proven to be a
transparent alias of that intrinsic. The driver computes this proof from the
complete resolved module set before running P4; the proof follows resolved
symbol identity through typed expressions and local aliases, never an import
path or the spelling `otherwise`.

Boolean guards contain ordinary expressions, so `let ... in ...` is valid
inside a condition and its names remain local to that expression. The
additional bare `let` guard qualifier, such as `| let next = e, next > 0 =
result`, scopes `next` over the remaining guards and result; this is an
issue-requested project extension. PureScript 0.15.16 supports expression and
`<-` pattern guards, but not that bare cross-guard binding form. Literal,
array, named, and typed patterns remain structural through P4; P5 checks their
types and lowers them to checked THIR patterns. P6 preserves literals, arrays,
and aliases in Typed Core so the shared pattern matrix owns matching,
exhaustiveness, redundancy, and source coverage. No pattern lowers through a
source `Eq` lookup. Local `let` and guarded `where` declaration annotations
remain as resolved `Typed` expressions for P5.

Every rewrite evaluates source operands in the order defined by the language.
A failed guard proceeds to the next guard without evaluating that guard's body.
A generated temporary binds an expression once when duplication would change
evaluation. The saved scrutinee is bound in an outer `let`, and fallthrough helpers share
an inner `let` with the case. They refer to outer locals and the saved product
directly. Helpers exist only for rows after the first guarded row, including the
final failure continuation. Earlier rows are checked directly as case branches
and have no unused helper copies: such copies would create additional inferred
class obligations without the original branch's expected result type.
Separating the saved value from the helper binding group preserves
its scope without making that group recursive. Its calls
pass an empty token to delay evaluation, rather than passing the product's
polymorphic fields through a newly inferred helper parameter. Passing one of those locals in as a value
argument would instantiate a polymorphic scheme once, before the guard body
applies the arguments that determine its constraints. P4 preserves the source
span of every retained user expression; generated scaffolding points to the
construct that introduced it.

Rejected alternatives: desugaring operators in P2 cannot respect imported
fixities; waiting until MIR would discard useful source types and spans; and
duplicating a scrutinee in each equation can duplicate effectful calls.

## Algorithms

```text
desugar(program):
    verify_resolved_hir(program)
    for each declaration in source order:
        reject mixed associativity and repeated non-associative operators
        reassociate expression, pattern, and type operator chains by fixity
        expand sections using the resolved operator and retained operand side
        replace unary minus with an application of its resolved negate reference
        expand sequencing forms
        compile equations/guards to ordered HIR cases
        turn where bindings into scoped lets
    verify_normalized_hir(program)
    return program
```

Fresh IDs are allocated monotonically per module so duplicated continuations
cannot reuse any declaration's local IDs. The verifier checks
generated references and that no eliminated surface form remains. A rewrite
that would need unavailable library evidence is diagnosed at its source span.
Unary minus preserves the minus-token span on the generated function reference
and the complete expression span on the application.

## Code map

The `psrs-desugar` organization separates fixity reassociation and type
normalization from section, unary-minus, sequencing, equation, and `where`
lowering around `desugar_module(module: hir::Module) -> Result<hir::Module,
Vec<DesugarError>>`. The fixity logic handles value chains,
constructor-pattern chains, and type chains; type normalization traverses
signatures, declaration types, and typed-pattern annotations. P2 case-arity
normalization and rich pattern retention, P3 local-ID assignment, and P4
guard/equation lowering keep
their own stage boundaries. The P4 guard paths preserve coverage provenance
and bind multi-scrutinee records once before pattern tests; a whole-program
proof recognizes transparent aliases of Boolean `true`. `psrs-hir::verify`
exposes resolved and normalized profile checks. The desugar crate depends on
HIR and source utilities, never THIR or backend types.

## Invariants and verification

Existing IDs keep their meaning, new local IDs are unique and scoped, and
source-origin ranges remain valid. Every output has the same observable
evaluation order as its input; tests cover fixity reassociation, ambiguity
diagnostics, sections, unary minus, ordered guards, and single evaluation of
scrutinees. The normalized verifier rejects expression, pattern, and type
operator chains, sections, unary-minus nodes, `do`/`ado`, guarded equations,
and `where` nodes after P4. Generated continuation branches use fresh local
IDs and retain source/generated coverage provenance. Literal, array, named,
and typed patterns remain available for P5 and P6; the verifier rejects only
surface forms that P4 owns, such as unresolved operator chains.

## Worked example

```purescript
f x | x > 0 = x
    | otherwise = 0
```

P4 resolves `>` and the symbol named `otherwise`, and treats the final branch
as unconditional only if whole-program analysis proves that resolved symbol
is a transparent alias of Boolean `true`. It then produces an ordered
conditional in a single equation body. `x` keeps its `LocalId`; the comparison
and each guard keep source origins. The second body runs only if the first
guard fails.

For `value = -x`, P3 resolves the generated `negate` reference to the ordinary
local or imported value in scope. P4 turns the resolved unary-minus node into
`negate x`, with the minus token on the function reference and the full `-x`
range on the application.

## Boundaries and interfaces

P4 receives verified resolved HIR and returns verified normalized HIR to P5.
Because it preserves the HIR representation, P5 does not need a new ID space.
P4's generated binders and library calls participate in ordinary type
checking. Core and CC never see the eliminated surface forms.

## Open questions and future work

The exact ordering of `ado` dependency groups and all official syntax forms
must be compared with the official suite. Additional syntax sugar should be
added here only when its lowering needs resolved identities but not checked
types.

## References

- [Modules and resolution](modules-and-resolution.md),
  [type inference](../type-system/type-inference.md), and
  [D-01](../../D-01-frontend-and-ir-boundaries.md).
