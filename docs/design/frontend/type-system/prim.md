# Primitive Interfaces and Relations

**Feature:** F-02

**Status:** Draft

**Prerequisites:** [kinds](kinds.md), [type inference](type-inference.md), [classes and evidence](classes-and-evidence.md), [rows and records](rows-and-records.md), and [modules and resolution](../semantics/modules-and-resolution.md).

**Summary:** `Prim.*` is the compiler-provided interface family: nine virtual modules whose members carry stable identities, kinds, functional dependencies, and roles without appearing in any source file. Most members are ordinary declarations that participate in inference like any other type; a minority are relations the compiler itself must solve. This document owns that inventory, the ownership of each member's semantics, the dispatch from a stable class identity to its rule, and the contract a rule's result must satisfy. It adds no type node, no kind table, and no downstream exception for any member.

## Scope

This document owns the member inventory of the `Prim.*` family, which semantic layer owns each member, the dispatch from a class identity to its rule, the result contract a rule returns, the diagnostic interface for the members that report instead of solving, the classification of each member's result as a compile-time proof or a runtime dictionary, and the boundary between a `Prim` relation and an intrinsic value.

It does not own the virtual module interfaces or name resolution, which [modules and resolution](../semantics/modules-and-resolution.md) owns; kind legality and the kind of every unknown, which [kinds](kinds.md) owns; the constraint and evidence representation, generalization, and instance search, which [type inference](type-inference.md) and [classes and evidence](classes-and-evidence.md) own; row normalization, which [rows and records](rows-and-records.md) owns; or the lowering of primitive values and intrinsics, which the [backend dictionaries](../../backend/fp/type-classes-and-dictionaries.md) and [effects](../../backend/fp/effects.md) documents own.

## Background

Official PureScript embeds nine `Prim` modules in the compiler and refuses to let source declare or shadow them. Their members exist so that library code can name a compiler-guaranteed relation without the compiler inventing a special type form: `Prim.Row.Cons` is how a row extension is stated, `Prim.Symbol.Append` is how a type-level string is concatenated, `Prim.Int.Add` is how type-level integers are added. The members are ordinary class declarations with ordinary kinds and functional dependencies, marked primitive so that ordinary orphan and coherence rules do not apply to them. Their semantics, however, are compiler-owned: official `Environment.hs` supplies the kinds and fundeps as data, and `TypeChecker/Entailment.hs` supplies one solver per relation.

Two distinct mechanisms wear the `Prim` name, and conflating them is the most common way a `Prim` integration goes wrong. A **relation** is a class the compiler solves: `Prim.Int.Add 1 2 3` is a statement about types, discharged during type checking and erased afterwards. A **primitive value** is an operation the compiler implements directly: in official PureScript, value-level `+` on `Int` is a `foreign import add :: Int -> Int -> Int` in the Prelude and has nothing to do with `Prim.Int.Add`. The same split holds here, where `+` resolves to an `Intrinsic` through the compiler-known externals and `Prim.Int.Add` remains a type-level relation.

Official solving has a shape worth preserving. A relation's rule is tried before instance search and may decline; it may also emit extra obligations rather than guessing, which official carries through a writer and re-solves later. `Prim.Row.Lacks "a" r` with an open tail and at least one known label is *solved* by moving the obligation to the tail; with no known labels it *declines*, because no progress is possible; with the label already present it *cannot hold*. A rule that cannot distinguish those three answers reports a `Prim` obligation it cannot decide as a missing instance, which is why the distinction is a contract here rather than an implementation detail.

## Model

```text
PrimitiveDeclaration = { id: TypeId, module, name, kind: TypeExpr,
                         fundeps, roles, strategy }
Strategy = Interface            # a declaration only; inference uses it as a type
         | Relation             # the compiler solves it
         | Proof                # the compiler proves a type-level relation
         | Diagnostic           # it is carried to a diagnostic, never solved

PrimitiveRule = (class_id, arity) -> Rule
Rule = (&PrimitiveArgs, &mut InferState) -> PrimitiveOutcome

PrimitiveOutcome = Solved { evidence: Evidence, deferred: [Constraint] }
                 | Deferred { evidence: Option<Evidence>,
                              deferred: [Constraint] }
                 | Undecided
                 | Failed { code, detail }

EvidenceClass = CompileTimeProof    # no runtime value; a checked boundary
              | RuntimeDictionary   # a value that erases or is projected
              | ReportOnly          # no evidence; a diagnostic is the result
```

`PrimitiveArgs` are ordinary `InferType` values. A rule receives them exactly as an instance head's arguments arrive: zonked where known, carrying unsolved inference variables with their levels and kinds, and bound by the shared substitution. A rule never receives a private copy of the solver and never re-parses source syntax.

### Member inventory

Kinds are the official ones, since source compatibility requires them. "Declared" means the registry supplies a stable identity, kind, fundeps, and role for the member; "Solved" means a rule discharges the relation. Roles marked *phantom* are the official declaration and are currently absent from the registry.

| Member | Kind | Strategy | Evidence | Declared | Solved |
| --- | --- | --- | --- | --- | --- |
| `Prim` builtins: `Type`, `Constraint`, `Symbol`, `Row`, `Function`, `Array`, `Record`, `String`, `Char`, `Number`, `Int`, `Boolean` | see [kinds](kinds.md) | Interface | — | yes | n/a |
| `Prim.Partial` | `Constraint` | Diagnostic | ReportOnly | yes | by design |
| `Prim.Boolean.True`, `Prim.Boolean.False` | `Boolean` | Interface | — | yes, kind differs | n/a |
| `Prim.Coerce.coerce` (value) | `forall a b. Coercible a b => a -> b` | Interface | — | yes, as an intrinsic | n/a |
| `Prim.Coerce.Coercible` | `forall k. k -> k -> Constraint` | Proof | CompileTimeProof | yes | yes |
| `Prim.Ordering.Ordering` | `Type` | Interface | — | yes | n/a |
| `Prim.Ordering.LT`, `EQ`, `GT` | `Ordering` | Interface | — | yes | n/a |
| `Prim.Row.Cons` | `forall k. Symbol -> k -> Row k -> Row k -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.Row.Lacks` | `forall k. Symbol -> Row k -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.Row.Nub` | `forall k. Row k -> Row k -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.Row.Union` | `forall k. Row k -> Row k -> Row k -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.RowList` | `Type -> Type` (*phantom*) | Interface | — | yes | n/a |
| `Prim.RowList.Cons` | `forall k. Symbol -> k -> RowList k -> RowList k` (*phantom*) | Interface | — | yes | n/a |
| `Prim.RowList.Nil` | `forall k. RowList k` | Interface | — | yes | n/a |
| `Prim.RowList.RowToList` | `forall k. Row k -> RowList k -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.Symbol.Append` | `Symbol -> Symbol -> Symbol -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.Symbol.Compare` | `Symbol -> Symbol -> Ordering -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.Symbol.Cons` | `Symbol -> Symbol -> Symbol -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.Int.Add` | `Int -> Int -> Int -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.Int.Mul` | `Int -> Int -> Int -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.Int.Compare` | `Int -> Int -> Ordering -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.Int.ToString` | `Int -> Symbol -> Constraint` | Relation | RuntimeDictionary | yes | no |
| `Prim.TypeError.Doc` | `Type` | Interface | — | yes | n/a |
| `Prim.TypeError.Text` | `Symbol -> Doc` (*phantom*) | Interface | — | yes | n/a |
| `Prim.TypeError.Quote` | `forall k. k -> Doc` (*phantom*) | Interface | — | yes | n/a |
| `Prim.TypeError.QuoteLabel` | `Symbol -> Doc` (*phantom*) | Interface | — | yes | n/a |
| `Prim.TypeError.Beside`, `Prim.TypeError.Above` | `Doc -> Doc -> Doc` (*phantom*) | Interface | — | yes | n/a |
| `Prim.TypeError.Fail` | `Doc -> Constraint` | Diagnostic | ReportOnly | yes | by design |
| `Prim.TypeError.Warn` | `Doc -> Constraint` | Diagnostic | RuntimeDictionary + report | yes | no |
| `Prim.undefined` | `forall a. a` (value) | Interface | — | **no** | n/a |

The inventory is the design's completeness statement: a member is either an interface others may name, a relation the compiler must decide, a proof it must derive, or a report it must emit, and it is never a fourth thing.

## Design

**One registry, one identity, no per-member type form.** Every member above is a declaration in one registry that supplies its stable identity, kind, fundeps, and roles, and the resolver derives the virtual module interfaces from that registry. A member is named through its identity, so a qualified import, an alias, or a re-export reaches the same declaration and therefore the same rule. Because the identity is what selects behaviour, a user cannot obtain a different `Prim` semantics by importing a different spelling, and a downstream pass cannot obtain the right behaviour by matching a name. `Prim.Row.Cons` is a class constraint on the shared spine, not a node, a flag, or a field; `RowList`, `RowList.Cons`, `RowList.Nil`, `Doc`, `Text`, `Quote`, `QuoteLabel`, `Beside`, and `Above` are ordinary type constructors applied through ordinary application, and a type-level string or integer is an ordinary literal in the same spine. Giving a member its own type node is rejected, because the member is then only reachable through the path that knows that node, and every other path — instantiation, substitution, unification, generalization, scope checking — has to be taught about it again.

**Ownership by layer.** Each kind of content has exactly one owner, and the owner is the layer that already owns the mechanism rather than the layer that mentions the member.

| Content | Owner |
| --- | --- |
| Module names, member names, stable identities, kinds, fundeps, roles | the primitive registry, whose interfaces [modules and resolution](../semantics/modules-and-resolution.md) exposes |
| `RowList`, `RowList.Cons/Nil`, `Doc`, `Text`, `QuoteLabel`, `Beside`, `Above`, and every other member's *type structure* | the shared type spine and ordinary constructor application |
| Type-level `Symbol` and `Int` literals | the shared type model: equality, substitution, kind checking, generalization |
| Row structure, extension, absence, union, nub, and row-to-list conversion | the public row mechanism in [rows and records](rows-and-records.md) |
| `Row.Cons/Lacks/Union/Nub`, `RowToList`, `Symbol.Append/Compare/Cons`, `Int.Add/Mul/Compare/ToString`, `Coercible` | one rule per relation in this document, dispatched by identity |
| `Fail`, `Warn`, `Partial` | constraint solving and the diagnostic interface in this document |
| Primitive values and intrinsics: arithmetic, comparison, array access, conversions, `coerce` | term typing in the type checker and the target lowering contract in the backend |

**A rule consumes the shared mechanisms.** A relation's arguments are ordinary inference types, so a rule works on rows, symbols, and integers through the same normalizer, equality, and substitution that an instance head uses. A rule must not carry its own kind table, its own row representation, or a private reader for source literals; the reason is the failure the `Coercible` path already shows, where a private reading of `Row` and `Record` drifted from the kind checker's and the two no longer unify. A rule that needs a kind reads it through the shared solver, a row through the shared normalizer, and a literal through the shared type model. Consequently a `Prim` argument that arrives as an unsolved variable is treated as unknown by every rule in the same way, and no rule can succeed or fail on the strength of a shape it recognized differently.

**Dispatch by identity, once.** Constraint solving consults, in order: givens, the primitive rule table, then instance search. The table is keyed by class identity and checked before search, so a primitive relation never depends on an instance being visible. A member with no rule and no visible instance is reported as a missing instance for that relation, which is the shape the official suite expects; a member whose rule declines and whose class has visible instances continues into ordinary search, so declining is not a failure. Aliases and re-exports do not create a second entry, because the table is keyed by identity rather than by the name a use happened to spell.

**Four outcomes, and unknowns are none of the decisive ones.** A rule distinguishes four results. `Solved` carries explicit evidence and may carry additional obligations. `Deferred` carries obligations without the full answer, which is how a rule makes progress without guessing. `Undecided` means the rule does not apply. `Failed` means the rule applies and the obligation cannot hold. An unsolved inference variable in any argument is never enough for `Solved` or `Failed`: a rule either makes progress on the known part and defers the rest, or declines. This is what keeps a `Prim` obligation from being reported as impossible while its argument is still unknown, and it is what lets `Lacks "a" ("a" :: Int | r)` be a definite failure while `Lacks "a" ("b" | r)` is a deferral and `Lacks "a" r` is undecided. `Failed` is reported under the official code for that obligation, with the relation's own reason as detail, so a diagnostic agrees with the suite instead of inventing a code.

**Deferral is re-queued, not forgotten.** A deferred obligation re-enters wanted solving with its origin retained, after the improvement pass has run again on the improved arguments. Rules are bounded: a deferral that makes no progress toward a solved argument is reported rather than retried, and search depth and work remain bounded as for instance contexts. A rule's speculative work — reading a row, unifying a literal, solving a nested obligation — runs under the shared speculation operation, so a declined or failed candidate leaves no substitution, level, kind, or diagnostic behind.

**Functional dependencies are how a relation informs inference.** The fundeps in the inventory are the official ones and are improvement, not runtime fields. They are applied before a rule is consulted, so a rule usually receives determined arguments, and again after each deferral. Improvement never assigns a rigid variable and never uses a later fallback.

**Members do not share one strategy.** The strategy column is normative: a `Prim` class is a relation, a proof, a report, or an interface, and the four behave differently on purpose. `Coercible` is a proof: it is discharged by the role analysis, its evidence is a compile-time boundary rather than a dictionary, and a user cannot supply it. `Fail`, `Warn`, and `Partial` are reports: `Fail` and `Partial` are never discharged by a rule and reach the diagnostic, where they carry their own meaning — a type-level string in `Fail`'s argument is the message, and `Partial` carries what an exhaustiveness check could not decide. `Warn` is the one member that both reports and discharges: it prefers a warning already in scope, so a warning can be deferred and propagated outward, and otherwise constructs a dictionary that erases and emits a warning at the obligation's span. The row and symbol and integer relations are ordinary relations whose dictionaries are empty and erase, but their rules decide real type-level facts and their evidence records the arguments they decided. Treating all `Prim` classes as one kind of obligation is rejected, because it produces a rule for `Fail` that silently accepts a program the official compiler rejects with a custom error.

**Compile-time proof and runtime dictionary are different results.** A `Proof` member leaves no runtime value: its evidence is a checked boundary that Core lowers to a representation conversion, and it is never passed as an argument. A `Relation` member's evidence is an ordinary dictionary node in THIR that erases when the relation is only about types; if a relation ever needs a value, that is a lowering decision made where the value's representation is chosen, not by the rule. A `ReportOnly` member produces a diagnostic and no evidence at all. Downstream stages consume the classification recorded in the evidence node; they never re-derive a `Prim` member's meaning from its name, and a backend that sees a `RepresentationCast` does not need to know that `Coercible` produced it.

**Intrinsics are a separate contract with a separate identity.** Primitive *values* are compiler-known externals with a fixed type and a target lowering, selected by their own identity and never through a class constraint. They are not in the registry's declaration list, they are not solved by the rule table, and no `Prim` relation is implemented by one. The two are adjacent in exactly one place worth naming: `Safe.Coerce.coerce` is a value whose own type is constrained by `Prim.Coerce.Coercible`, so the source API is an intrinsic while the class it mentions is a compiler-owned relation, and the coercion the intrinsic elaborates to carries the proof the rule derived. The same shape is possible in a library — a Prelude defining addition from `Prim.Int.Add` at the type level and from an intrinsic at the value level — but the two obligations have separate evidence and separate lowering.

**The kind environment covers `Prim` members like any other declaration.** Every member's kind is checked by the program-level kind pass from the registry, through the same denotation and primitive table as source declarations, and a `Prim` declaration participates in synonym cycles, role inference, and the checked environment exactly as a source declaration does. A member referenced without a checked scheme is a missing-metadata diagnostic, never a fresh kind variable; `Prim` declarations are supplied by the registry precisely so that this case cannot arise for them. A member's kind is also the arity and shape contract for its rule: a rule reads a `Symbol` argument as a type-level string because the declaration says its kind is `Symbol`, not because the argument happens to look like one.

## Algorithms

```text
solve(wanted):
    improve wanted using class fundeps and givens
    if a given unifies with wanted without assigning a rigid variable: Given
    if a superclass path from a given proves wanted: Superclass
    if wanted.class_id has a primitive rule:
        match rule(arguments, state):
            Solved   { evidence, deferred } -> keep evidence; re-queue deferred
            Deferred { evidence, deferred } -> keep evidence if any; re-queue deferred
            Undecided                      -> continue to instance search
            Failed { code, detail }        -> report code with detail
    collect visible candidate instances and chains
    select a branch as the ordinary rules in classes-and-evidence require

solve_coercible(source, target, env, visibility):   # the Proof member
    equal types, or compose given proofs in either direction
    match constructors and compare arguments by role: nominal equal,
        representational recursive, phantom irrelevant
    unwrap a newtype only while its constructor is visible
    read both arguments' kinds through the shared kind solver
    return a compile-time proof, never a dictionary

re-queue(constraint):
    improve it again, re-enter solve with its original span,
    and stop if the pass made no progress toward determined arguments

primitive rule, by member:
    Row.Cons      a known label builds the extension; an unknown label defers
    Row.Lacks     a known absent label is proved; the label present fails;
                  an open tail with known labels defers to the tail;
                  an open tail with no known label declines
    Row.Union     a closed side merges or splits; otherwise the known labels
                  move to the result and the remainder defers with a fresh tail
    Row.Nub       a closed row is canonicalized; an open row declines
    RowToList     a closed row becomes a RowList; otherwise declines
    Symbol.Append two known symbols concatenate; a known prefix or suffix splits
    Symbol.Cons   a known symbol splits into head and tail; a one-scalar head joins
    Symbol.Compare, Int.Compare  known operands decide the Ordering
    Int.Add, Int.Mul, Int.ToString  known literals decide forwards and backwards
    Coercible     roles, equality, givens, and visible newtypes prove the relation
    Warn          a warning in scope defers the report; otherwise report and discharge
    Fail, Partial never discharge; they reach the diagnostic
```

Every branch above reads its arguments through the shared row normalizer, the shared type model, and the shared kind solver, and every branch that unifies does so through the shared substitution under speculation.

## Code map

The registry owns declarations and nothing else:

- `crates/psrs-hir/src/primitives/` holds one module per family — `core.rs`, `rows.rs`, `numbers.rs`, `type_error.rs` — behind `primitive_type_declarations() -> Vec<(&'static str, TypeDeclaration)>`. Each entry carries its `TypeId`, name, declared kind, fundeps, and roles. The registry contains no rule, no solver, and no diagnostic text.
- `Interface::primitive_module` in the resolver derives each virtual module's members from the registry, so a recognized module always advertises exactly what the registry declares. [Modules and resolution](../semantics/modules-and-resolution.md) owns that derivation.
- `crates/psrs-typecheck/src/typecheck/prim/` holds the rule table: `mod.rs` for dispatch and the outcome types, and one module per family — `row.rs`, `symbol.rs`, `int.rs`, `type_error.rs`, `coercible.rs` — where each rule is a function over the shared `InferState` returning `PrimitiveOutcome`. `coercible.rs` uses the shared kind solver rather than a private one. The existing coercion code under `typecheck/classes/coercion/` is the one rule that exists today; it moves to this module when it stops carrying its own kind denotation, substitution, and unifier.
- `crates/psrs-typecheck/src/typecheck/classes/solve.rs` owns the single dispatch site, so givens, primitive rules, and instance search are consulted in one place and in one order.
- Intrinsic values stay where they are: `psrs_hir::Intrinsic` for identity, the type checker's intrinsic typing for term types, and the backend for lowering. No `Prim` name appears in that path.

## Invariants and verification

Every `Prim` member resolves to exactly one declaration identity, and that identity is unchanged by qualification, aliasing, and re-export. Every member's kind is checked by the program-level kind pass from the registry; no `Prim` member is checked against a fabricated kind and none is absent from the checked environment. Every `Prim` relation is dispatched by identity to at most one rule, and a member with no rule reaches instance search or a missing-instance diagnostic rather than an ad hoc path. No rule reports a definite outcome from an unsolved argument: an unknown argument yields progress with deferral, or a decline. A declined, deferred-then-abandoned, or failed rule leaves no substitution, level, kind, or diagnostic behind. A rule's decision is expressed through the shared substitution, so the arguments its evidence records are the arguments the constraint now has. A `Proof` member produces no runtime value, a `Relation` member's evidence is a dictionary node THIR verifies, and a `ReportOnly` member produces a diagnostic and no evidence. No downstream stage recovers a member's meaning from its name.

Verification follows the whole chain rather than one stage: interface and identity, kind checking, inference with improvement, solving and evidence elaboration, THIR verification, and lowering where the member has a runtime effect. The chain is checked per member with its own case, so that a member which resolves but is not solved is evidenced as resolved-and-unsolved rather than as absent.

The combination cases matter more than the isolated ones:

- a qualified import, an alias, and a re-export of the same member select the same rule and produce the same diagnostic;
- an explicit signature and an inferred obligation reach the same rule with the same arguments;
- a `Prim` relation combines with an ordinary class, with functional dependencies on both sides, with rank-N positions, with open rows, and with type synonyms;
- an unknown argument, an illegal kind on an argument, a contradictory pair of obligations, and an escaping skolem each produce their own outcome rather than a shared failure;
- a `Proof` member, a `Relation` member, and a `ReportOnly` member are each verified for what they are: a boundary with no runtime value, a dictionary THIR checks, and a diagnostic with no evidence;
- any new relation or diagnostic has an official differential or suite case, and any path with runtime behaviour is executed, not only compiled.

## Worked example

`Prim.Int.ToString` has fundep `int -> string` and one rule: with a known type-level integer, the string argument is determined. The corpus case `failing/IntToString1.purs` exercises exactly that.

```purescript
testToString :: forall i s. ToString i s => Proxy i -> Proxy s
testToString _ = Proxy

posToString :: Proxy "a"
posToString = testToString (Proxy :: Proxy 1)
```

The obligation is `ToString 1 "a"`. Improvement has nothing to add, no given matches, and the rule fires on the known integer: it decides the string as `"1"` and returns an empty dictionary whose recorded arguments are `[1, "1"]`. Unifying that decided string with the wanted `"a"` then fails through ordinary type equality, so the program is rejected as `TypesDoNotUnify` on two type-level strings. The diagnostic comes from equality on a decided argument, not from a rule that inspected the wanted string, which is why the same rule accepts `Proxy "1"` and rejects `Proxy "a"`.

The two neighbouring cases differ only in the integer, and the rule decides each of them the same way: `Proxy (-1)` becomes `"-1"` and `Proxy 0` becomes `"0"`, so each is rejected for the same reason. Those two integers also show why the literal is a type and not a token: the negative case is a type-level integer whose value is `-1`, and both reach the rule as the same kind of argument.

Had the integer been an unsolved variable, the rule would have had no known argument and declined, leaving the obligation to instance search. Had the wanted string been a variable instead, the rule would still have decided it and ordinary unification would have solved the use. The same shape applies to `Row.Lacks`, where a known absent label over a closed row is proved, the same label over that row fails, and an open tail defers.

## Boundaries and interfaces

P5 consumes the registry, checks its members with the program-level kind pass, and solves its relations with the shared constraint machinery. It emits an ordinary dictionary node, a compile-time proof boundary, or a diagnostic, and [Core lowering](../semantics/core-lowering.md) turns the first into a value and the second into a representation conversion. The backend receives an evidence classification, never a `Prim` name; [type classes and dictionaries](../../backend/fp/type-classes-and-dictionaries.md) owns dictionary representation and [effects](../../backend/fp/effects.md) owns how a library type such as `Effect` becomes a closure. Neither re-derives a relation's meaning.

A `Prim` declaration is not source and cannot be shadowed, replaced, or given a different meaning by an import; that is resolution's contract and this document depends on it. A member that is only an interface — `RowList`, `Doc`, `Text` — reaches later stages as an ordinary nominal type constructor with its identity, and a stage that needs to know it is a row list must not special-case its name.

## Open questions and future work

`Prim.undefined` has no compiler-owned value identity and is not exported by the virtual interface; it needs an identity, a type, and a lowering decision together, because a partial value is a runtime concern as much as a typing one. `Prim.Boolean.True` and `.False` are declared at kind `Type` where the official environment gives them kind `Boolean`, and the official phantom roles for `RowList`, `RowList.Cons`, `Text`, `Quote`, `QuoteLabel`, `Beside`, and `Above` are absent, which makes those declarations nominal here and therefore restricts `Coercible` at them. `Prim.Partial` is registered as a class where the official environment registers both a constraint-kinded type and a parameterless class. Type-level `Reflectable` and `IsSymbol` relations exist in later official versions and are not part of the inventory above; adding a member is a registry change with the same requirements as any other.

Implementation coverage belongs in [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md). A rule for a member whose shared foundations are incomplete is not a local shortcut: the argument types it needs must participate in ordinary instantiation, substitution, unification, generalization, and scope checking first, and a rule that cannot satisfy that reports the limitation rather than approximating the member with a private path.

## References

- [PureScript `Constants/Prim.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Constants/Prim.hs) is the authoritative member list; `Environment.hs` supplies `primTypes`, `primClass`, `primRowClasses`, `primRowListClasses`, `primSymbolClasses`, `primIntClasses`, `primTypeErrorClasses`, and the phantom roles.
- [PureScript `TypeChecker/Entailment.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Entailment.hs) is the reference for dispatch order, per-relation rules, and deferral; `Entailment/Coercible.hs` and `Entailment/IntCompare.hs` cover the proof and the comparison relation.
- [Kinds](kinds.md), [type inference](type-inference.md), [classes and evidence](classes-and-evidence.md), and [rows and records](rows-and-records.md).

## Implementation notes

The registry covers every official member except one: `Prim.undefined` has no
identity here. Where the registry and the official environment disagree, it is
recorded in the inventory above — `Prim.Boolean.True` and `.False` are declared at
kind `Type` where the official environment gives them kind `Boolean`, the official
phantom roles for `RowList`, `RowList.Cons`, `Text`, `Quote`, `QuoteLabel`,
`Beside`, and `Above` are absent so those members are nominal here, and `Prim.Partial`
is registered only as a class where the official environment registers both a
constraint-kinded type and a parameterless class. Kinds and functional dependencies
otherwise match the official environment member by member.

Only `Coercible` has a rule, and it is the one rule that does not yet use the shared
foundations: its kind denotation, substitution, and unifier live in
`crates/psrs-typecheck/src/typecheck/classes/coercion/kinds.rs` and read `Row` as
`Type -> Type` and `Record` as `Row Type -> Type`, which the kind checker's own
readings contradict. It also owns a recursion-bounded solver that no other relation
shares. The rule is entered from `solve.rs` by comparing `class_id` against the
`COERCIBLE` constant directly, so the dispatch is a special case at the call site
rather than a table lookup, and the coercion helpers are reached from the given
rewriting and the newtype deriving rule as well.

The other twelve relations have no rule and no dispatch entry: `Prim.Row.Cons`,
`Lacks`, `Union`, `Nub`, `Prim.RowList.RowToList`, `Prim.Symbol.Append`, `Cons`,
`Compare`, `Prim.Int.Add`, `Mul`, `Compare`, and `ToString`. A wanted
`Prim.Row.Lacks`, `Prim.Row.Union`, `Prim.Row.Nub`, `Prim.Row.Cons`,
`Prim.RowList.RowToList`, `Prim.Symbol.Append`, `Prim.Symbol.Cons`,
`Prim.Symbol.Compare`, `Prim.Int.Add`, `Prim.Int.Mul`, `Prim.Int.Compare`, or
`Prim.Int.ToString` therefore reaches ordinary instance search and is reported as
a missing instance, which is the correct outcome for an unimplemented relation but
not for a supported one. `Warn` and `Fail` and `Partial` have no diagnostic
interface: `Fail` and `Partial` reach the same missing-instance path as any other
unsolved class, and `Warn` does not defer to an enclosing warning.

The shared types those rules need are now reachable: `InferType` and THIR
carry `TypeLevelString` and `TypeLevelInt`, a type-level literal is decided by
its value and is what an unknown variable is solved to, and signature
elaboration accepts a general `Row`, both literals, and the `Row` and `Record`
primitive heads, so a rule receives its arguments as ordinary types. A row
reaches one normalizer whether it was written with row syntax, record syntax,
or a `Record` application, and `normalize_row` reports an invalid shape rather
than reading it as a closed row. `Type`, `Constraint`, and `Symbol` are still
rejected in a type position because that needs `KindApplication`, and
`KindApplication` is still missing.
