# Type Inference and THIR

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [kinds](kinds.md), [modules and resolution](../semantics/modules-and-resolution.md), type schemes, subsumption, and skolemization.

**Summary:** P5 infers ordinary expressions and checks annotated polymorphic expressions using PureScript's quantified and constrained types. It performs higher-rank subsumption, row unification, and class entailment, then emits verified THIR with checked types and explicit evidence. Inference and the checked IR share one type spine, every type unknown carries the kind the kind layer maintains for it, and a declaration's inferred constraints become its scheme's constraints rather than obligations that must already be solved.

## Scope

This document owns schemes, instantiation, generalization, bidirectional checking, signatures, term and binder typing, subsumption, the shared type spine, and THIR. [Kinds](kinds.md) owns kind legality and the kind of every unknown; [rows](rows-and-records.md) owns row equations; [classes](classes-and-evidence.md) owns constraint entailment and evidence. Runtime erasure belongs to the backend.

## Background

HM inference explains unannotated let polymorphism, but PureScript additionally supports `forall` beneath arrows, constrained types, explicit type application, and row-polymorphic records. A use of a polymorphic value instantiates its `forall`; checking against an expected `forall` skolemizes it and rejects escaping skolems. Subsumption handles function variance and inserts dictionary evidence at permitted expression boundaries. Recursive declarations are checked as dependency groups; signatures supply polymorphic recursion where accepted by the source language.

## Model

```text
InferType = Constructor | Variable | Application | KindApplication
          | ForAll | Constrained | RowEmpty | RowExtend | TypeLevelString
          | TypeLevelInt | Skolem | Wildcard | Unknown(InferVarId)
Scheme = { quantified_type_vars, quantified_kind_vars, constraints, body }
THIR = { declarations, typed expressions, typed binders,
         explicit type and dictionary evidence, source_ranges }
```

`InferType` is the whole type spine plus the inference-only `Unknown` and `Wildcard` nodes. HIR type expressions are elaborated into it, the kind layer reads the same expressions in a kind position, and THIR and Core keep the result, so a row, a type-level literal, and a primitive head mean one thing at every stage. An arrow is `Application(Application(Constructor(Function), a), b)`, a record is `Application(Constructor(Record), row)`, and a user type keeps its resolved declaration identity however many arguments it is applied to.

`Constrained` is a typechecker node, not a checked-IR node. Finalization discharges each constraint of a scheme into a dictionary parameter, so THIR and Core keep the spine without it, exactly as official PureScript's `ConstrainedType` disappears once dictionaries are elaborated. `Unknown`, `Wildcard`, and `Skolem` are inference-only as well: a quantified variable that survives generalization is a THIR `ForAll` binder, not an exported solver cell.

Inference unknowns have levels, kinds, and substitutions private to P5. Quantified variables and skolems have distinct identities and scopes. `Effect a` is an ordinary imported type application for P5: no compiler-native effect row, handler, private constructor, or special inference rule is introduced, and unification does not identify it with a function. Its runtime closure is produced later by one representation lowering ([effects](../../backend/fp/effects.md)). THIR and Core carry the same representation ([DEC-15](../../../decision/DEC-15-unified-type-representation.md)).

Inference state has three owners with distinct lifetimes. The `SemanticEnv` is immutable for the module being checked: imported signatures, visible instances, checked kinds and roles, class and constructor tables, synonyms, and type names. The `InferState` is the mutable solver: substitutions, levels, the kind substitution and the kind of every unknown, wanted constraints, solved evidence, and diagnostics. `Scope` is the lexical chain: globals, locals, givens in scope, rigid variables, and annotation variables. Entering or leaving a scope and speculating over a candidate are operations on those three, with one snapshot and rollback, so a feature cannot save a different subset of fields than its neighbour.

## Design

Infer synthesizable expressions and check expressions with expected types. Instantiate `forall` and solve constrained uses through class entailment. When checking a signature or higher-rank argument, skolemize expected quantifiers, perform structural subsumption, and check that skolems do not escape. Function parameter comparison is contravariant and result comparison covariant; record subsumption compares common labels and checks closed-row extras and omissions. Evidence can be inserted at elaboration sites, while comparison under a type constructor cannot invent term-level dictionaries.

Infer a recursive SCC with shared placeholders, respecting explicit signatures, then solve and generalize only variables permitted by the environment and remaining constraints. Use kind-correct constructor and pattern types; type-check case alternatives, literals, arrays, record operations, newtypes, and foreign imports. Explicit type/kind applications and typed holes follow the official source rules. Build THIR only after zonking, ambiguity checks, and evidence elaboration.

A declaration's scheme carries the constraints that were inferred for it, whether or not the source declared them. Generalization is therefore one sequence and not two:

```text
solve what can be solved       -> evidence for the obligations that a given,
                                  a superclass path, an instance, or a
                                  primitive relation discharges
retain what may be generalized -> the residual wanted constraints, with their
                                  origins, are the scheme's constraints
check ambiguity                -> every residual variable is determined by the
                                  result type and the class functional
                                  dependencies, or AmbiguousTypeVariables
generalize type and constraints -> quantify the unknowns that survive, each
                                  with the kind the kind layer recorded for it
abstract dictionaries          -> one dictionary parameter per retained
                                  constraint, in source order
```

Requiring every wanted constraint of a signatureless declaration to be solved before generalization rejects a program official PureScript accepts: `f x = method x` for `class C a where method :: a -> a` must infer `C a => a -> a`, and demanding the constraint first turns it into `NoInstance`. Only genuinely ambiguous or unsolved constraints are errors. Official PureScript draws the same line in the other direction: residual constraints of a recursive group are reported rather than generalized, because generalizing them would admit unsound polymorphic recursion.

Every type unknown's kind is maintained where the unknown is bound, not where it is used. `bind_type_variable` performs the occurs check, the skolem-escape check, level adjustment, and kind compatibility through the one kind solver; a row-valued binding checks `Row k`, and a `forall` binder's kind annotation is checked against the kind recorded for that variable. Whether a kind is therefore enforced must not depend on the feature that triggered the binding, so no feature keeps its own kind table or its own compatibility test.

Preserve `ForAll` beneath arrows and inside fields during signature
elaboration. A lambda parameter with a quantified expected type enters the
local environment as a polymorphic value; each reference opens only its
leading quantifiers with fresh unknowns. Constructor patterns and record
projections retain the same field quantifiers. Checking a record literal or
constructor argument propagates the expected field type to its value, rather
than first inferring all values as monotypes.

Checking a leading `forall` allocates a fresh skolem scope and checks the body
with rigid representatives of its binders. Substitutions of unknowns that
outlive that scope must not contain its skolems. Restore quantified binders in
the checked result after the escape audit; do not export solver skolems as free
Core variables. Generalization excludes locally bound variables and never
captures a skolem. Substitution below `ForAll` respects binder shadowing and
uses fresh identities to avoid capture.

Higher-rank checking does not imply arbitrary impredicative inference. When
an expected type is an unconstrained unknown, instantiate an inferred
polymorphic expression before solving the unknown, following the official
checker's rule. Explicit polymorphic fields and annotations supply the
boundaries at which a polymorphic value may be retained.

Rejected alternatives: pure HM cannot check higher-rank signatures; unifying a `forall` as though it were a monotype is unsound; generalizing recursive uses before group checking admits unsound polymorphic recursion; and carrying solver cells into THIR breaks the P5 boundary. Demanding that a signatureless declaration's constraints already be solved is rejected because it makes a hand-written signature a precondition for inferring a qualified type. Letting each feature module check kinds for itself is rejected because whether an operation is kind-corrected would then depend on the caller's path.

## Algorithms

```text
check(expr, expected):
    if expected begins with forall: skolemize binder, check body, reject escape
    if expected begins with constraint: bind given evidence, check body
    if expr is lambda and expected is arrow: bind parameter, check result
    if expr is record: check each field against its expected field type
    otherwise synthesize expr and subsume synthesized type against expected

infer_application(function, argument):
    synthesize function; instantiate only its leading quantifiers/constraints
    if its arrow is known: check argument against the parameter type
    otherwise infer a monomorphic argument and solve an arrow with fresh result

subsume(actual, expected):
    if expected begins with forall:
        enter a fresh scope; skolemize expected; recurse and reject escape
    if actual begins with forall:
        instantiate its leading binders inside the current comparison scope
    elaborate actual constraints where expression evidence can be inserted
    compare arrows contravariantly/covariantly and records by row rules
    otherwise unify kind-correct monotypes

infer_group(group, environment):
    allocate placeholders; expose declared signatures at recursive uses
    infer/check all bodies and patterns; solve row and class obligations
    for each declaration: solve the wanteds a given, superclass path, instance,
        or primitive relation discharges, and retain the rest as its constraints
    for each declaration: check that every residual variable is determined by
        the result type and the class functional dependencies
    generalize each declaration's type and constraints, quantifying the
        permitted unknowns with their kinds
    abstract one dictionary parameter per retained constraint
    zonk types and emit typed declarations with explicit evidence

bind_type_variable(variable, ty, span):
    reject an occurs-check failure and a skolem escape
    adjust the levels of ty to the variable's level
    unify the recorded kind of variable with kind_of_type(ty);
        report KindsDoNotUnify when they differ

speculate(f):
    snapshot the mutable InferState
    run f; on failure restore the snapshot and report no progress
    on success keep the result, including any diagnostic the caller must preserve

with_scope(f):
    push the lexical Scope; run f; pop it, restoring the rigids and givens it added
```

Occurs checks traverse applications, rows, quantifiers, and constraints. Type errors retain both expected and actual types and the originating source range. Row normalization reports an invalid shape rather than returning a partial row; see [rows](rows-and-records.md). A speculative candidate uses `speculate`, so a failed match leaves no substitution, level, kind, or diagnostic behind, and a diagnostic a caller must keep is re-emitted deliberately instead of surviving by accident.

Comparison-local unknowns may refer to the comparison's skolems; unknowns
created outside that scope may not. This distinction accepts two equivalent
quantified signatures without allowing an inferred monomorphic parameter to
escape into a higher-rank signature.

## Code map

`crates/psrs-typecheck/src/typecheck/` owns `infer/` for synthesis and expected
type propagation, `signature.rs` for scoped signature elaboration, `unify.rs` for
equality, `generalize.rs` for residual-constraint generalization, `rows.rs` for the
shared row normalizer, and focused `rank_n/` and `kind/` modules for subsumption,
quantified instantiation, skolem scopes, and the shared kind solver. `classes/`
provides the adjacent constraint solver. The semantic entry consumes HIR and the
program-wide checked kind environment and produces THIR. `crates/psrs-thir/src/`
owns checked types, expressions, evidence, and
`verify_module(&Module) -> Result<(), Vec<VerifyError>>`.

The state is split so the split is visible in the types:

- `SemanticEnv` is built once per module from the program-wide checked
  environment and the module's own declarations, and is read-only afterwards.
  Imported signatures, visible instances, `CheckedKindEnv`, class and constructor
  tables, synonyms, and type names live here.
- `InferState` holds the substitutions, levels, the kind substitution and per-unknown
  kinds, wanted constraints, solved evidence, and diagnostics. `InferState::snapshot`
  and `restore` are the only way state is saved, so a speculative path cannot
  accidentally omit the kind table that a later binding depends on.
- `Scope` holds globals, locals, givens, rigid variables, and annotation variables,
  and is changed only through `with_scope`, which restores what it added.

The higher-rank owner provides `check_expr(expr, expected)`,
`subsume(actual, expected, evidence_mode)`, and scoped
`instantiate_forall`/`skolemize_forall` operations. Evidence mode distinguishes
expression boundaries that can insert dictionaries from structural comparison
inside a type. Both THIR and Core retain `ForAll` with bound identities and
body references, including at local binder and field types. The kind owner
provides `denote_kind`, `primitive_kind`, `unify_kind`, `bind_kind_variable`, and
`kind_of_type` as described in [kinds](kinds.md); coercion and rows call those
rather than re-deriving kinds.

## Invariants and verification

Every THIR expression and binder has a kind-valid type; every reference is resolved; every required dictionary is supplied or abstracted; and no unknown, wildcard, or escaping skolem remains. Quantified variables have valid scope. Every type unknown has a kind, and every binding preserves kind compatibility, so no expression can be typed at a kind its declaration forbids. Every inferred declaration's constraints appear in its scheme, and every scheme constraint becomes a dictionary parameter in the emitted declaration, so a signature is never a precondition for using a class. Every speculative path restores the whole mutable state on failure. Accept and reject cases cover rank-N arguments, subsumption direction, explicit type application, recursive signatures, inferred constrained polymorphism, rows, ADTs, and ambiguous constraints; compare them with official `purs`.

## Worked example

`apply :: (forall a. a -> a) -> Int` requires an argument polymorphic at the call site. `apply (\x -> x)` checks the lambda against a skolemized `forall a. a -> a`; a monomorphic `Int -> Int` argument fails. By contrast, `let id = \x -> x in id id` generalizes `id` and instantiates its two uses independently.

For `class C a where method :: a -> a`, the declaration `f x = method x` has one wanted `C ?a` that no instance discharges. Generalization retains it, checks that `?a` occurs in `f`'s result type `?a -> ?a`, quantifies `?a` with kind `Type`, and gives `f` the scheme `forall a. C a => a -> a` with one dictionary parameter. `f 1` then instantiates that scheme and solves `C Int` at the use, while `f (\y -> y)` is rejected for lacking `C (Int -> Int)` if no such instance exists.

## Boundaries and interfaces

P5 consumes normalized HIR and the program-wide checked kind environment. It emits verified THIR for [Core lowering](../semantics/core-lowering.md). No inference substitutions, class search state, or runtime layouts cross the boundary. WIT-bound foreign declarations receive ordinary checked PureScript types; ABI validation occurs later.

THIR is verified, and the verification has a stated boundary. `verify_module` checks what THIR alone can decide: every type reference indexes the module's type table, quantified variables are in scope at every use, each expression and binder is typed against its checked type, a local or global reference is a valid instance of its scheme, literals and aggregates match their type's shape, and each evidence node matches the constraint it claims to prove at the expression boundary. Three guarantees are trusted from P5 rather than re-derived, because THIR deliberately does not carry the metadata they need: kind legality of a type, which the checked kind environment established and THIR does not repeat; coherence of instance selection, which the solver established when it chose a branch; and the derivation of a `Coercible` proof from roles and newtype visibility. THIR keeps quantifier identity and variable scope precisely because it does check those, and it retains the checked `Coercible` boundary types so the verifier can reject a proof attached to a different conversion. Any future guarantee that must be verified rather than trusted needs the corresponding immutable metadata added to THIR; the verifier does not claim a check it cannot perform.

## Open questions and future work

Track official behavior for partial signatures, visible type applications, and ambiguity/defaulting in executable compatibility cases. [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md) records implementation coverage, not changes to this semantic target.

## References

- [PureScript type checker](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Types.hs), [subsumption](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Subsumption.hs), and [skolems](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Skolems.hs). `typesOf` is the reference for the solve, retain, ambiguity, and generalize sequence and for rejecting residual constraints in a recursive group.
- [Kinds](kinds.md), [rows](rows-and-records.md), [classes](classes-and-evidence.md), and [primitives](prim.md).

## Implementation notes

`InferType` has no `KindApplication` and no explicit row-constructor node.
Signature elaboration accepts the `Record` and `Row` primitive heads, the general
`Row` form, type-level `String` and `Integer` literals, and a record row tail that
is an ordinary type; `Type`, `Constraint`, and `Symbol` are still rejected,
because naming a kind in a type position needs `KindApplication` and rejecting
them is more honest than giving them a fresh unknown kind. Record syntax and an
equivalent `Record` application reach one construction: `elaborate_record` builds
the same `Application(Constructor(Record), row)` the explicit application does.
The `Prim.Row`, `Prim.RowList`, `Prim.Symbol`, `Prim.Int`, and `Prim.TypeError`
classes are declared with their kinds and functional dependencies but have no
rule; see [primitives](prim.md).

Generalization currently keeps only a signature's constraints. A signatureless
declaration starts as a monomorphic scheme with no constraints and every wanted
constraint is solved before generalization, so an unsolvable one becomes
`NoInstance` and no residual-constraint abstraction exists. A scheme records only
its quantified type variables, not their kinds.

`bind_variable` is the inference-side rule `kinds.md` states. It runs the occurs
check, the skolem-escape check, the level adjustment, and then the kind
compatibility check, in that order, through the one kind solver in
`typecheck/kind.rs`. A variable's kind is recorded when the variable is created,
and `record_variable_kind` is the only writer of that table, so a fresh unknown, a
`forall` binder, a constructor parameter, and a quantified variable's fresh
instance all establish a kind the same way. A binding refused for a kind reason
records nothing and returns `false`, which is the same answer a refused occurs
check gives, so a fixed-point caller reads it correctly. A type whose kind the
checked environment does not supply is left alone, because the missing scheme is
the kind pass's diagnostic and inference must not reject the same module twice.

The three state owners are declared in `typecheck/state.rs`: `SemanticEnv`,
`InferState`, and `Scope`, with `Checker` owning all three. `InferState::snapshot`
and `restore` are the only way solver state is saved, and the operations that use
them are named for what they do rather than for their caller:
`speculate` discards a failed candidate's diagnostics, `speculate_reporting`
re-emits them, and `without_diagnostics` keeps everything a re-elaboration solved
while dropping only what it reported — the three the earlier code had written out
by hand and had made disagree. `with_scope`, `with_givens`, `with_given_chain`,
`with_skolem_scope`, and `in_nested_level` are the only ways a scope is entered
and left; a given's argument variables are rigid only inside their given, while a
skolem stays rigid after its scope, so those two are separate operations rather
than one scope that restores everything. `rigid` lives in `InferState` rather than
in `Scope` because it is keyed by an identity the solver allocates, so a rollback
that recycles a variable has to recycle its rigidity with it.

`normalize_row` reports an invalid row shape instead of reading it as a closed
row, and it carries the range of the operation that reached the shape because
`InferType` holds no ranges of its own. Finalization discharges `Constrained` into
dictionary arrows, which matches the boundary stated above.
