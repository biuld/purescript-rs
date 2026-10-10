# Frontend Type System

P5 consumes normalized resolved HIR and produces verified THIR. It has five
ordered concerns about the language's types and one about what the compiler
itself provides:

| Document | Owns | Dependency |
| --- | --- | --- |
| [Kinds](kinds.md) | Constructor kinds, synonym expansion, kind legality, the program-wide checked kind and role environment | resolved HIR |
| [Type inference](type-inference.md) | The shared type spine, schemes, signatures, unification, generalization, THIR | kinds |
| [Classes and evidence](classes-and-evidence.md) | Constraints, instances, dictionaries, evidence elaboration | type inference |
| [Deriving](deriving.md) | The known deriving rules, their field-usage and variance checks, generated members, and newtype adaptation | classes, kinds |
| [Rows and records](rows-and-records.md) | Row kinds, the row normalizer, row unification and equations | kinds and type inference |
| [Primitives](prim.md) | The `Prim.*` member inventory, per-relation rules, report-only members, and the proof/dictionary split | kinds, type inference, classes, rows |

Deriving consumes the compiler-owned `Coercible` proof that [Primitives](prim.md)
defines, and it emits generated members that [Classes and evidence](classes-and-evidence.md)
checks and elaborates like written ones.

[Primitives](prim.md) answers what the compiler itself provides: it names every
`Prim.*` member, says which layer owns each member's semantics, and specifies the
contract a member's rule must satisfy. It owns no type form — a primitive is a
declaration on the same spine as any other, and its meaning is reached by identity.

The design follows the official PureScript type checker: polymorphic kinds,
higher-rank `forall`, multi-parameter classes with functional dependencies,
instance chains, and row-polymorphic records. `Effect` is a library type, not a
compiler-native effect row, and P5 does not give it a representation rule.
[Effects](../../backend/fp/effects.md) lowers it to a closure after checking.

## Shared bases

The five documents are separate owners, but they rest on bases that are shared
rather than re-implemented per concern. Each of these has one owner, and a feature
that needs one of them consumes that owner's result rather than reconstructing it:

| Base | Owner | Consumed by |
| --- | --- | --- |
| Kind denotation, primitive kind table, kind solver | [Kinds](kinds.md) | every kind check, every type binding, rows, coercion |
| The type spine (`Constructor`/`Application`, rows, quantifiers, literals) | [Type inference](type-inference.md) | kinds, classes, rows, THIR, Core |
| Quantifiers, scopes, and rigid variables | [Type inference](type-inference.md) | signatures, classes, row bindings |
| The constraint and `TypeTemplate` representation | [Classes and evidence](classes-and-evidence.md) | signatures, generalization, evidence |
| Row normalization and row equations | [Rows and records](rows-and-records.md) | unification, `Prim.Row` rules, record operations |
| Synonym expansion, roles, and the checked environment | [Kinds](kinds.md) | signatures, role walking, coercion |
| Evidence elaboration and checked-IR verification | [Classes and evidence](classes-and-evidence.md), [type inference](type-inference.md) | THIR, Core lowering |
| Stable declaration identity and the per-member rule table | [Primitives](prim.md) | constraint solving, diagnostics, lowering |

Two rules keep those bases shared. First, a semantic fact has one owner: a kind is
read through the kind denotation, a role through the checked environment, a
constraint through the constraint representation, and a row through the row
normalizer. Second, every consumer of a checked program sees the same checked
metadata: an imported type's kind and roles are the ones its declaring module
checked, so a conflict is reported once, against the module that declares it.

A compiler-provided member is not an exception to those rules. `Prim.Row.Cons` is a
class on the shared spine whose rule reads the shared row normalizer; `Prim.Int.Add`
is a class whose rule reads the shared type model; neither introduces a form, and
both are selected by declaration identity rather than by the name a use spelled.

## P5 in sequence

```text
P4 resolved HIR
  -> check_program: declaration kinds, synonyms, roles, program-wide diagnostics
  -> per module: check_module against that environment
  -> infer: solve obligations, retain the residual constraints, check ambiguity,
     generalize type and constraints, abstract dictionaries
  -> finalize: discharge constraints into dictionary arguments, zonk types
  -> verify THIR
```

Inference variables, worklists, and partial solutions remain private to the
checker; THIR contains zonked checked types, stable resolved IDs, explicit
evidence, and source ranges. What the THIR verifier checks and what it trusts from
the elaborator is stated in [type inference](type-inference.md#boundaries-and-interfaces).

[DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md) tracks
implementation coverage. These topic documents define the intended complete behavior. The
[frontend boundary contract](../00-ir-boundaries.md) owns the P4/P5 and P5/P6
handoffs; [Functional Core](../semantics/functional-core.md) owns the later
Core type and term model.
