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

Official solving has a shape worth preserving. A relation's rule is tried before ordinary dictionary lookup and may decline; it may also emit extra obligations rather than guessing, which official carries through a writer and re-solves later. `Prim.Row.Lacks "a" r` with an open tail and at least one known label is *solved* by moving the obligation to the tail; with no known labels it *declines*, because no progress is possible. If the known prefix already contains the label, the relation cannot hold even when the tail is open. Official represents that case by falling through to its missing-dictionary diagnostic; this compiler's rule outcome can state the semantic failure directly while preserving the official diagnostic code.

## Model

```text
PrimitiveDeclaration = { id: TypeId, module, name, kind: TypeExpr,
                         fundeps, roles, strategy }
Strategy = Interface            # a declaration only; inference uses it as a type
         | Relation             # the compiler solves it
         | Proof                # the compiler proves a type-level relation
         | Diagnostic           # it uses the report/diagnostic path

PrimitiveRule = (class_id, arity) -> Rule
Rule = (&PrimitiveArgs, &mut InferState) -> PrimitiveOutcome

PrimitiveOutcome = Solved { evidence: Evidence, deferred: [Constraint] }
                 | Deferred { evidence: Option<Evidence>,
                              deferred: [Constraint] }
                 | Undecided
                 | Failed { code, detail }

EvidenceClass = CompileTimeProof    # no runtime value; a checked boundary
              | RuntimeDictionary   # a value that erases or is projected
              | ReportingDictionary # a dictionary that also emits a warning
              | ReportOnly          # no evidence; a diagnostic is the result
```

`PrimitiveArgs` are ordinary `InferType` values. A rule receives them exactly as an instance head's arguments arrive: zonked where known, carrying unsolved inference variables with their levels and kinds, and bound by the shared substitution. A rule never receives a private copy of the solver and never re-parses source syntax.

### Member inventory

Kinds are the official ones, since source compatibility requires them. "Provided" means the compiler exposes the member through its primitive interface. Type and class declarations come from the registry; intrinsic values use the compiler-owned value identity. "Current path" records whether the relation rule or diagnostic path is present; `partial` marks a path with a known semantic limitation.

| Member | Kind | Strategy | Evidence | Provided | Current path |
| --- | --- | --- | --- | --- | --- |
| `Prim` builtins: `Type`, `Constraint`, `Symbol`, `Row`, `Function`, `Array`, `Record`, `String`, `Char`, `Number`, `Int`, `Boolean` | see [kinds](kinds.md) | Interface | — | yes | n/a |
| `Prim.Partial` | `Constraint` | Diagnostic | ReportOnly | yes | partial |
| `Prim.Boolean.True`, `Prim.Boolean.False` | `Boolean` | Interface | — | yes | n/a |
| `Safe.Coerce.coerce` (value) | `forall a b. Coercible a b => a -> b` | Interface | — | yes, as an intrinsic | n/a |
| `Prim.Coerce.Coercible` | `forall k. k -> k -> Constraint` | Proof | CompileTimeProof | yes | yes |
| `Prim.Ordering.Ordering` | `Type` | Interface | — | yes | n/a |
| `Prim.Ordering.LT`, `EQ`, `GT` | `Ordering` | Interface | — | yes | n/a |
| `Prim.Row.Cons` | `forall k. Symbol -> k -> Row k -> Row k -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.Row.Lacks` | `forall k. Symbol -> Row k -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.Row.Nub` | `forall k. Row k -> Row k -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.Row.Union` | `forall k. Row k -> Row k -> Row k -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.RowList` | `Type -> Type` (*phantom*) | Interface | — | yes | n/a |
| `Prim.RowList.Cons` | `forall k. Symbol -> k -> RowList k -> RowList k` (*phantom*) | Interface | — | yes | n/a |
| `Prim.RowList.Nil` | `forall k. RowList k` | Interface | — | yes | n/a |
| `Prim.RowList.RowToList` | `forall k. Row k -> RowList k -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.Symbol.Append` | `Symbol -> Symbol -> Symbol -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.Symbol.Compare` | `Symbol -> Symbol -> Ordering -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.Symbol.Cons` | `Symbol -> Symbol -> Symbol -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.Int.Add` | `Int -> Int -> Int -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.Int.Mul` | `Int -> Int -> Int -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.Int.Compare` | `Int -> Int -> Ordering -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.Int.ToString` | `Int -> Symbol -> Constraint` | Relation | RuntimeDictionary | yes | yes |
| `Prim.TypeError.Doc` | `Type` | Interface | — | yes | n/a |
| `Prim.TypeError.Text` | `Symbol -> Doc` (*phantom*) | Interface | — | yes | n/a |
| `Prim.TypeError.Quote` | `forall k. k -> Doc` (*phantom*) | Interface | — | yes | n/a |
| `Prim.TypeError.QuoteLabel` | `Symbol -> Doc` (*phantom*) | Interface | — | yes | n/a |
| `Prim.TypeError.Beside`, `Prim.TypeError.Above` | `Doc -> Doc -> Doc` (*phantom*) | Interface | — | yes | n/a |
| `Prim.TypeError.Fail` | `Doc -> Constraint` | Diagnostic | ReportOnly | yes | yes |
| `Prim.TypeError.Warn` | `Doc -> Constraint` | Diagnostic | ReportingDictionary | yes | yes |
| `Prim.undefined` | `forall a. a` (value) | Interface | — | yes | n/a |

The inventory is the design's completeness statement: a member is either an interface others may name, a relation the compiler must decide, a proof it must derive, or a report it must emit, and it is never a fourth thing. The two compiler-provided values in the table are not registry declarations.

## Design

**One registry for type and class declarations, one identity, no per-member type form.** Every primitive type or class member above is a declaration in one registry that supplies its stable identity, kind, fundeps, and roles, and the resolver derives the virtual module interfaces from that registry. Compiler-provided values such as `Prim.undefined` and `Safe.Coerce.coerce` use the compiler-owned value identity and interface path. A member is named through its identity, so a qualified import, an alias, or a re-export reaches the same declaration and therefore the same rule. Because the identity is what selects behaviour, a user cannot obtain a different `Prim` semantics by importing a different spelling, and a downstream pass cannot obtain the right behaviour by matching a name. `Prim.Row.Cons` is a class constraint on the shared spine, not a node, a flag, or a field; `RowList`, `RowList.Cons`, `RowList.Nil`, `Doc`, `Text`, `Quote`, `QuoteLabel`, `Beside`, and `Above` are ordinary type constructors applied through ordinary application, and a type-level string or integer is an ordinary literal in the same spine. Giving a member its own type node is rejected, because the member is then only reachable through the path that knows that node, and every other path — instantiation, substitution, unification, generalization, scope checking — has to be taught about it again.

**Ownership by layer.** Each kind of content has exactly one owner, and the owner is the layer that already owns the mechanism rather than the layer that mentions the member.

| Content | Owner |
| --- | --- |
| Type and class member names, identities, kinds, fundeps, roles | the primitive registry, whose interfaces [modules and resolution](../semantics/modules-and-resolution.md) exposes |
| Compiler-provided value identities and types | the intrinsic environment and value interface, with term typing and lowering owned by the type checker and backend |
| `RowList`, `RowList.Cons/Nil`, `Doc`, `Text`, `QuoteLabel`, `Beside`, `Above`, and every other member's *type structure* | the shared type spine and ordinary constructor application |
| Type-level `Symbol` and `Int` literals | the shared type model: equality, substitution, kind checking, generalization |
| Row structure, extension, absence, union, nub, and row-to-list conversion | the public row mechanism in [rows and records](rows-and-records.md) |
| `Row.Cons/Lacks/Union/Nub`, `RowToList`, `Symbol.Append/Compare/Cons`, `Int.Add/Mul/Compare/ToString`, `Coercible` | one rule per relation in this document, dispatched by identity |
| `Fail`, `Warn`, `Partial` | constraint solving and the diagnostic interface in this document |
| Primitive values and intrinsics: arithmetic, comparison, array access, conversions, `coerce` | term typing in the type checker and the target lowering contract in the backend |

**A rule consumes the shared mechanisms.** A relation's arguments are ordinary inference types, so a rule works on rows, symbols, and integers through the same normalizer, equality, and substitution that an instance head uses. A rule must not carry its own kind table, its own row representation, or a private reader for source literals; the reason is the failure the `Coercible` path already shows, where a private reading of `Row` and `Record` drifted from the kind checker's and the two no longer unify. A rule that needs a kind reads it through the shared solver, a row through the shared normalizer, and a literal through the shared type model. Unknown parts remain unknown, while the rule may use a known row prefix or other known structure to decide whether to solve, defer a residual obligation, fail, or decline.

**Dispatch by identity, with evidence-defined ordering.** The table is keyed by class identity, so a primitive relation never depends on an instance being visible and an alias or re-export does not create a second entry. `Proof` and `Relation` rules run before direct given lookup and ordinary instance search, so a checked proof boundary or a type-level decision cannot be replaced by dictionary evidence. If such a rule declines, the solver continues to givens and ordinary search. Report rules run after direct given lookup: `Warn` can prefer an in-scope warning dictionary, and `Fail` or `Partial` can remain as constraints under the enclosing declaration's retention policy. A member with no applicable rule, report, given, or visible instance is diagnosed through the ordinary constraint path; declining is not itself a failure.

The ordering follows from the evidence class rather than being arbitrary. A `Proof` member's evidence is a compile-time boundary, and the checked IR rejects a `Given` or `Superclass` node as that boundary, so its rule runs before direct given lookup and composes assumed proofs itself. A `Relation` produces a type-level dictionary, so its rule also runs before a caller's given can mask the relation's decision. `ReportingDictionary` and `ReportOnly` members run after givens: `Warn` deliberately prefers an in-scope warning so it can propagate outward, while `Fail` and `Partial` respect the enclosing constraint's retention policy. Official PureScript gives `Coercible` a first attempt in `forClassNameM`, handles `Warn` before its general primitive rules, and tries the relation rules before the final `findDicts` arm in `forClassName`.

**Four outcomes, selected from the known semantic shape.** A rule distinguishes four results. `Solved` carries explicit evidence and may carry additional obligations. `Deferred` may carry partial evidence together with residual obligations, which is how a rule preserves known progress without guessing about an open tail. `Undecided` means the known arguments do not give the rule a decision. `Failed` means known facts establish that the obligation cannot hold. An unknown variable does not erase semantic evidence in the other arguments: an empty closed row satisfies `Lacks` for any label; `Lacks "a" ("a" :: Int | r)` fails because the known prefix contains the forbidden label even though `r` is open; `Lacks "a" ("b" | r)` returns evidence for the known prefix and a residual `Lacks "a" r`; and `Lacks "a" r` declines because no row field is known. `Failed` uses an existing official diagnostic code and preserves the relation's reason as detail, rather than inventing a new code.

**The framework checks a decision; a rule owns applicability.** A rule returns `Failed` when the known structure rules out every possible answer, as `Cons "ab" "c" s` does because a head must contain one scalar. It can do so even when another argument remains open, as `Lacks "a" ("a" :: Int | r)` demonstrates. A contradiction between a value the rule decided and an argument the goal already fixed is different: official solving has no explicit failure result, because a rule returns `Maybe [TypeClassDict]` without inspecting the goal, and the solver unifies each produced dictionary's own arguments against the goal's, in order, for every dictionary it produces (`Entailment.hs:301`). A failed unification aborts that goal and reports the mismatch. Here the rule's dictionary carries the type it decided at the position it decides and the goal's own argument everywhere else — official's `tcdInstanceTypes` — and the framework runs that check through the shared unifier. It binds a decided argument or reports a contradiction; applicability, deferral, and facts that no possible answer exists remain the rule's responsibility.

That separation is load-bearing. The framework must not infer a rule's semantic answer from whether its work changed the inference-variable set: a rule may prove part of an open row and defer the rest, or reject an obligation from a known prefix while its tail stays unknown. The shared unifier remains the authority for installing returned decisions and detecting contradictions. Declined candidates and rejected speculative work roll back through the shared snapshot, while diagnostics that establish the result retain their origin.

**Deferral is re-queued, not forgotten.** A deferred obligation re-enters wanted solving with its origin retained, after the improvement pass has run again on the improved arguments. The shared solver bounds search depth and limits a deferral tree to 32 re-entered obligations; an active-path cycle or exhausted budget is reported instead of retried. Whether unresolved residuals may be generalized belongs to the enclosing policy: `Retain` can carry a flexible residual into the inferred scheme, while `RequireSolved` reports it at the concrete obligation. A rule's speculative work — reading a row, deciding a literal, solving a nested obligation — runs under the shared speculation operation, so an undecided rule leaves no substitution, level, kind, or diagnostic behind. A decision that contradicts the obligation keeps the shared unifier's diagnostic, because that diagnostic is the result rather than a side effect of a trial that did not apply.

**Functional dependencies are how a relation informs inference.** The fundeps in the inventory are the official ones and are improvement, not runtime fields. They are applied before a rule is consulted, so a rule usually receives determined arguments, and again after each deferral. Improvement never assigns a rigid variable and never uses a later fallback.

**Members do not share one strategy.** The strategy column is normative: a `Prim` class is a relation, a proof, a report, or an interface, and each has a different evidence contract. `Coercible` is a proof: role analysis derives a compile-time boundary rather than a dictionary, and a user cannot provide it. `Fail`, `Warn`, and `Partial` use report behavior. `Fail` and `Partial` do not produce dictionary evidence; their constraints remain residual when policy permits, then reach report handling at a concrete use. A valid `Fail Doc` supplies the custom error message. `Partial` is specified to report exhaustiveness information, but the current HIR has no producer for that metadata, so unresolved `Partial` still receives generic `NoInstanceFound` text. `Warn` prefers an in-scope warning dictionary; otherwise it emits a warning and returns an empty primitive dictionary that erases. The row, symbol, and integer relations also use empty dictionaries, but their rules decide type-level facts and record the decided arguments. Treating all `Prim` classes as one kind of obligation is rejected, because it would erase these distinct evidence and reporting behaviors.

**Compile-time proof and runtime dictionary are different results.** A `Proof` member leaves no runtime value: its evidence is a checked boundary that Core lowers to a representation conversion, and it is never passed as an argument. A `Relation` member's evidence is an ordinary dictionary node in THIR that erases when the relation is only about types; if a relation ever needs a value, that is a lowering decision made where the value's representation is chosen, not by the rule. `ReportOnly` members produce no dictionary evidence; `Warn` is a `ReportingDictionary` because it both records a warning and supplies an empty dictionary. Downstream stages consume the classification recorded in evidence; they never re-derive a `Prim` member's meaning from its name, and a backend that sees a `RepresentationCast` does not need to know that `Coercible` produced it.

**Intrinsics are a separate contract with a separate identity.** Primitive *values* are compiler-known externals with a fixed type and a target lowering, selected by their own identity and never through a class constraint. They are not in the registry's declaration list, they are not solved by the rule table, and no `Prim` relation is implemented by one. The two are adjacent in exactly one place worth naming: `Safe.Coerce.coerce` is a value whose own type is constrained by `Prim.Coerce.Coercible`, so the source API is an intrinsic while the class it mentions is a compiler-owned relation, and the coercion the intrinsic elaborates to carries the proof the rule derived. The same shape is possible in a library — a Prelude defining addition from `Prim.Int.Add` at the type level and from an intrinsic at the value level — but the two obligations have separate evidence and separate lowering.

**The kind environment covers `Prim` members like any other declaration.** Every member's kind is checked by the program-level kind pass from the registry, through the same denotation and primitive table as source declarations, and a `Prim` declaration participates in synonym cycles, role inference, and the checked environment exactly as a source declaration does. A member referenced without a checked scheme is a missing-metadata diagnostic, never a fresh kind variable; `Prim` declarations are supplied by the registry precisely so that this case cannot arise for them. A member's kind is also the arity and shape contract for its rule: a rule reads a `Symbol` argument as a type-level string because the declaration says its kind is `Symbol`, not because the argument happens to look like one.

## Algorithms

```text
solve(wanted):
    improve wanted using class fundeps and givens
    if wanted.class_id has a primitive rule:
        match rule(arguments, state):
            Solved   { evidence, deferred } -> unify each decided argument with the
                                              goal's at that position; report a
                                              mismatch if any fails; keep evidence;
                                              re-queue deferred
            Deferred { evidence, deferred } -> the same check on the evidence, if any;
                                              keep it; re-queue deferred
            Undecided                      -> continue to givens and instance search
            Failed { code, detail }        -> report code with detail when the
                                              rule's known-shape failure condition holds
    if a given unifies with wanted without assigning a rigid variable: Given
    if a superclass path from a given proves wanted: Superclass
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
    improve it again and re-enter solve with its original span;
    bound repeated obligations by the shared solve-depth and work limits

primitive rule, by member:
    Row.Cons      a known label builds the extension; an unknown label defers
    Row.Lacks     a known absent label is proved; the label present fails;
                  an open tail with a known prefix proves that prefix and
                  defers the residual to the tail;
                  an open tail with no known label declines
    Row.Union     a closed side merges or splits; otherwise the known labels
                  move to the result and a residual relation defers with a
                  fresh tail
    Row.Nub       a closed row is canonicalized; an open row declines
    RowToList     a closed row becomes a RowList; otherwise declines
    Symbol.Append two known symbols concatenate; a known prefix or suffix splits
    Symbol.Cons   a known symbol splits into head and tail; a one-scalar head joins
    Symbol.Compare two known symbols decide the Ordering
    Int.Compare  two known literals decide; otherwise the relation over the
                 orderings and literals in scope decides by reachability
    Int.Add, Int.Mul, Int.ToString  known literals decide forwards and backwards
    Coercible     roles, equality, givens, and visible newtypes prove the relation
    Warn          prefer an in-scope warning dictionary; otherwise emit a warning
                  and return the empty dictionary
    Fail, Partial retain when policy permits and reach report handling at a
                  concrete use; Partial metadata is not yet produced by HIR
```

Every branch above reads its arguments through the shared row normalizer, the shared type model, and the shared kind solver, and every branch that unifies does so through the shared substitution under speculation.

## Code map

The registry owns declarations and nothing else:

- `crates/psrs-hir/src/primitives/` holds one module per family — `core.rs`, `rows.rs`, `numbers.rs`, `type_error.rs` — behind `primitive_type_declarations() -> Vec<(&'static str, TypeDeclaration)>`. Each entry carries its `TypeId`, name, declared kind, fundeps, and roles; `tests.rs` holds the fidelity tests against the official environment. The registry contains no rule, no solver, and no diagnostic text.
- `Interface::primitive_module` in the resolver derives each virtual module's members from the registry, so a recognized module always advertises exactly what the registry declares. [Modules and resolution](../semantics/modules-and-resolution.md) owns that derivation.
- `psrs_kind::check_roles` consumes the registry alongside the resolved modules, the same way the kind pass already consumed it when it built its schemes, so a member's declared roles reach the checked kind environment instead of the nominal default.
- `crates/psrs-typecheck/src/typecheck/prim/` holds the rule table and its shared contract: the table and outcome types, transactional acceptance, verification of returned decisions, bounded re-queueing, and one rule module per family — rows, symbols, integers, comparisons, coercions, and diagnostics. Each rule reads the shared `InferType` and solver state. The coercion rule reads roles from the checked kind environment and kinds from the shared kind solver.
- `crates/psrs-typecheck/src/typecheck/classes/solve/` owns the single constraint-search path: it orders givens, superclass projection, primitive rules, and instance search according to the evidence contract.
- Intrinsic values stay where they are: `psrs_hir::Intrinsic` for identity, the type checker's intrinsic typing for term types, and the backend for lowering. `Prim.undefined` is one of them and reaches the root `Prim` interface the same way `Safe.Coerce.coerce` does. No `Prim` name appears in that path.

## Invariants and verification

Every `Prim` member resolves to exactly one declaration identity, and that identity is unchanged by qualification, aliasing, and re-export. Every type and class member's kind is checked by the program-level kind pass from the registry; no member is checked against a fabricated kind and none is absent from the checked environment. Every `Prim` relation is dispatched by identity to at most one rule, and a member with no rule reaches instance search or a missing-instance diagnostic rather than an ad hoc path. A rule bases `Solved`, `Deferred`, and `Failed` on the semantic facts it can read; an unrelated unknown argument does not erase a known prefix or impose a solver-wide applicability gate. The shared unifier verifies every decision carried by evidence. Rejected speculative work leaves no substitution, level, kind, or diagnostic behind; a diagnostic that is the result of a rejected relation decision is preserved with its source origin. A `Proof` member produces no runtime value, a `Relation` member's evidence is a dictionary node THIR verifies, and a `ReportOnly` member produces a diagnostic and no evidence. No downstream stage recovers a member's meaning from its name.

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

`Prim.undefined` now has an identity, a type, and an interface export, but not a
runtime representation: a partial value is a runtime concern as much as a typing
one, and no lowering for it is decided here. Core lowering reports it instead of
inventing one, which means a program that mentions it is rejected even when the
binding turns out to be dead.

`Prim.Partial` is one declaration here rather than the two the official
environment keeps, and that is a modelling difference rather than a gap: the
official `primTypes` entry `(C.Partial, kindConstraint)` and the `primClasses`
entry share the name `Partial`, the kind `Constraint`, and one meaning, and
`Interface::primitive_module` already exports a registry declaration in both the
type and the class namespace. Giving them two `TypeId`s would give one name two
identities, which the derivation this document depends on forbids. A source
constraint and a source `import Prim (Partial)` therefore reach
`TypeId::PRIM_PARTIAL`, and `purs` reads the same kind for `Partial` as this
compiler does.

Type-level `Reflectable` and `IsSymbol` relations exist in later official versions
and are not part of the inventory above; adding a member is a registry change
with the same requirements as any other.

Implementation coverage belongs in [D-04](../../D-04-suite-roadmap.md). A rule for a member whose shared foundations are incomplete is not a local shortcut: the argument types it needs must participate in ordinary instantiation, substitution, unification, generalization, and scope checking first, and a rule that cannot satisfy that reports the limitation rather than approximating the member with a private path.

## References

- [PureScript `Constants/Prim.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/Constants/Prim.hs) is the authoritative member list; `Environment.hs` supplies `primTypes`, `primClass`, `primRowClasses`, `primRowListClasses`, `primSymbolClasses`, `primIntClasses`, `primTypeErrorClasses`, and the phantom roles.
- [PureScript `TypeChecker/Entailment.hs`](https://github.com/purescript/purescript/blob/master/src/Language/PureScript/TypeChecker/Entailment.hs) is the reference for dispatch order, per-relation rules, and deferral; `Entailment/Coercible.hs` and `Entailment/IntCompare.hs` cover the proof and the comparison relation.
- [Kinds](kinds.md), [type inference](type-inference.md), [classes and evidence](classes-and-evidence.md), and [rows and records](rows-and-records.md).

## Implementation notes

The primitive registry supplies every official type and class declaration with
its stable identity, kind, functional dependencies, and roles. The compiler-known
values `Prim.undefined` and `Safe.Coerce.coerce` use the intrinsic identity and
interface path instead of registry declarations. `Prim.undefined` has type
`forall a. a`, but Core lowering still rejects it because the backend has no
runtime representation for an undefined value.

Type inference accepts `Type`, `Constraint`, and `Symbol` as ordinary primitive
type constructors with their declared kinds. This does not add explicit kind
application: `KindApplication` remains absent from the inference spine, and the
official source parser has no source-level producer for it. Type-level integer
values use `i64`, while official PureScript solves them as unbounded integers;
computed arithmetic outside the local range declines instead of wrapping.

The shared solver selects primitive behavior by resolved class identity. Proof
and relation rules run before direct given lookup; report behavior runs after it,
so a scoped warning dictionary can be used before a new warning is emitted. Each
rule owns applicability and chooses among `Solved`, `Deferred`, `Undecided`, and
`Failed` from the semantic structure it can read. The framework checks returned
dictionary arguments through the shared unifier, keeps a contradiction's
diagnostic, and rolls back a declined speculative rule. It does not impose a
global all-arguments-determined or variable-progress gate. Deferred obligations
are retried with their source origin, bounded by solve depth, active-path cycle
detection, and a shared limit of 32 re-entered obligations. The caller's
`Retain` or `RequireSolved` policy flows through selected instance contexts, so
residual constraints can be generalized when the enclosing declaration permits
it.

The row rules use the shared row normalizer and kind solver. `Cons` constructs
an extension from a known label even when the tail is open. `Lacks` succeeds for
any label on an empty closed row, fails when a known prefix contains the label,
and succeeds on a closed row when the label is absent. With an open tail and a
nonempty known prefix, it returns evidence for the prefix and a residual
`Lacks` on the tail; an open row with no known fields declines. `Union` merges a
closed left row into the right row; with a closed right row and closed output it
partitions the output by the right row's label multiset; otherwise a nonempty
known left prefix contributes to the output and leaves a residual `Union` on the
open tail. An open left row with no known fields declines. `Nub` and
`RowToList` still require closed rows. Rigid-tail row unification has known bugs;
see [D-04](../../D-04-suite-roadmap.md).

The symbol rules compare Unicode scalar sequences, split `Cons` at one scalar,
and apply the official ordered cases for `Append`. The integer rules implement
`Add`, `Mul`, `Compare`, and `ToString`; overflow outside `i64` remains a
limitation. `Int.Compare` also uses in-scope comparison evidence and literals to
decide ordering by graph reachability, as specified by official entailment.

`Warn` uses a matching scoped dictionary when an explicit signature provides
one. Without such a given it emits `UserDefinedWarning` and supplies an empty
dictionary that erases; a signatureless declaration therefore does not infer a
residual `Warn` context. The warning belongs to the enclosing top-level value
signature or declaration, or to the enclosing instance declaration. Wanted
constraints retain that report origin separately from their obligation span,
including across imported uses and deferred solving. `Fail` remains residual when
generalization permits, then reports a valid `Doc` as `Custom error:`; malformed
`Doc` falls back to ordinary `NoInstanceFound`. `Partial` also has report-only
evidence, but HIR currently has no producer for its missing-case metadata, so an
unresolved direct `Partial` still receives the generic no-instance diagnostic.

`Coercible` consumes the shared kind and role environment and produces a checked
proof boundary. Five Coercible board mismatches remain because this proof has no
dictionary arguments for the shared relation-decision verification path; the
proof needs a verified answer contract of its own. [Classes and evidence](classes-and-evidence.md)
owns its search position and evidence use, while [D-04](../../D-04-suite-roadmap.md)
records corpus coverage for these limitations and the other incomplete primitive
cases.
