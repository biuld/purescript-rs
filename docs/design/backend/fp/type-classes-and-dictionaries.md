# Type Classes and Dictionaries

**Feature:** F-02  
**Status:** Draft (design)  
**Prerequisites:** [functional core](functional-core.md), [CC IR](cc-ir.md),
[polymorphism and erasure](polymorphism-and-erasure.md), and the record and
closure representations of [data representation](data-representation.md);
ad-hoc polymorphism and dictionary passing. Read
[IR boundaries](../00-ir-boundaries.md) first.  
**Summary:** Type classes are compiled by the standard dictionary-passing
translation: a class becomes a record of method closures plus superclass
dictionaries, an instance becomes a value of that record, and a constrained type
becomes a function arrow over a dictionary. Method selection is field
projection and superclass selection is nested projection, so the backend
introduces no runtime type tags and no dictionary-specific IR node — only
ordinary products and closures.

## Scope

This document owns the runtime representation of classes, instances, and
dictionary evidence, together with the resolution, dictionary construction, and
coherence checks at the Typed Core boundary. It does not own class and instance
syntax or the class environment (frontend; `FE-14`/`FE-15`/`FE-16` in
[DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md) and
[D-03](../../D-03-type-system.md)), the general record and closure layouts
([data representation](data-representation.md)), or the erased representation of
polymorphic values ([polymorphism and erasure](polymorphism-and-erasure.md)).

## Background

A type class lets one operation work uniformly over many types while the
*choice* of implementation is made at compile time. PureScript resolves that
choice through **dictionary passing**: a class is a record of methods, each
instance is a dictionary value, and a constrained function receives the
dictionary it needs as an ordinary argument. The translation was introduced by
Wadler and Blott, *How to Make Ad-hoc Polymorphism Less Ad Hoc* (1989), and its
design space — superclasses, instance contexts, coherence, and overlaps — is
surveyed by Peyton Jones, Jones, and Meijer, *Type Classes: Exploring the
Design Space* (1997).

The appeal for this compiler is that dictionary passing introduces **no new
runtime representation**. The official PureScript implementation also makes
class dictionaries explicit values before its CoreFn; that behavior is the
semantic reference here. A dictionary is a product whose fields are closures or
nested dictionaries; passing it and projecting from it use the same CC
operations the backend already verifies. No runtime type tag is needed because
the source type already determines which instance was selected
([polymorphism and erasure](polymorphism-and-erasure.md)).

## Model

### Classes, instances, and constraints

A class `C` with methods `m_i` and superclasses `S_j` is a record type:

```text
Dict(C, a) = {
    m_1 : τ_1(a),
    ...,
    m_n : τ_n(a),
    super_1 : Dict(S_1, a),
    ...,
    super_k : Dict(S_k, a),
}
```

The exact field order is an implementation detail fixed once per class; method
and superclass fields share the record and are addressed by logical field
index, not by a target offset.

An **instance** `instance C T` supplies one dictionary value of type
`Dict(C, T)`: each method field is a closure (or the top-level method
implementation), and each superclass field is a dictionary value that the
instance provides. An instance with an **instance context**, such as
`instance eqList :: Eq a => Eq (List a)`, is a function from its context
dictionaries to its dictionary.

A **constrained type** `C a => t` is a function arrow over a dictionary:

```text
[[ C a => t ]] = Dict(C, a) -> [[t]]
```

Multiple constraints become multiple dictionary parameters, in a fixed order.

### Elaboration

Resolution replaces each constraint at a use site with the dictionary that
satisfies it, and elaboration records that choice as an explicit value in Typed
Core:

- a class method used at type `τ` is a projection from the dictionary at `τ`;
- a superclass method is a projection of the superclass dictionary, then a
  projection of the method;
- a constrained binding becomes a lambda over the dictionaries of its
  constraints, so a shared binding derives its dictionary once;
- an instance context becomes a lambda over the context dictionaries that the
  instance's own dictionary fields capture.

Dictionaries are therefore ordinary values before CC ever sees them.
`psrs-hir::TypeKind::Constrained` carries the source constraint; the type
checker currently ignores it (`psrs-typecheck/src/typecheck/signature.rs`), so
the elaborate form is the target, not the current output.

### Runtime invariants

- **No runtime type tags.** A dictionary carries methods, never a type
  descriptor; type arguments are erased.
- **Coherence.** For every constraint there is exactly one dictionary value up
  to the class's semantic equality; instance heads do not overlap.
- **Instance-context size.** An instance context may not be larger than its
  head, so resolution terminates.
- **Binding-once.** A `let`-bound constrained value receives its dictionaries
  as parameters and does not re-derive them per use.
- **Representation-freedom.** Every dictionary is a product of closures and
  nested products; no stage may introduce a Wasm type dedicated to
  dictionaries.

## Design

### Dictionary passing as the representation

The chosen representation is the standard one:

| Source concept | Representation | CC operation |
| --- | --- | --- |
| Class `C a` | Record type | `Representation::Product` |
| Instance `C T` | Dictionary value | `ProductNew` of method closures and superclass dictionaries |
| Constraint `C a` | Function parameter | Ordinary parameter of the enclosing function |
| Method selection `m @τ` | Field projection | `ProductGet` |
| Superclass selection `m @τ` for `m ∈ S_j` | Nested field projection | `ProductGet` then `ProductGet` |
| Instance with context `C a => ...` | Closure over context | Lambda + closure captures |

A method implementation stored in a dictionary is either a direct reference to
a top-level function (lowered to a closure with no captures) or a closure over
the instance's context dictionaries and captures. This is exactly the closure
representation of [CC IR](cc-ir.md) and [data representation](data-representation.md);
the `ProductNew`/`ProductGet` pair already handles arbitrary field shapes.

### Specialization is an optimization, not a requirement

Dictionary passing is the semantic baseline. A later MIR optimization may
**specialize** a dictionary at a use site: if an instance is statically known,
its methods can be inlined and the dictionary allocation eliminated, recovering
unboxed arithmetic and direct calls. Specialization must preserve the
dictionary-passing result; it may not change observable behavior or introduce a
representation that the unspecialized path cannot encode. This mirrors the
position already taken for erasure and monomorphization in
[polymorphism and erasure](polymorphism-and-erasure.md).

### Rejected alternatives

- **Runtime type tags and tag-based dispatch.** Rejected: it adds runtime
  overhead, duplicates the type checker's decision, and conflicts with the
  erasure model, which relies on the consumer's source type to recover a
  representation ([DEC-07](../../../decision/DEC-07-runtime-representation-for-parameterized-adts.md)).
- **A dedicated dictionary or "type-class" node in CC/MIR.** Rejected: a
  dictionary is a record and needs no special operation; a special node would
  couple CC to the class system and require a second verifier path.
- **Whole-program monomorphization as the baseline.** Rejected as the semantic
  contract: separate compilation and values whose instantiation is not visible
  at the boundary would need a global pass. It remains available as an
  optimization of the dictionary path.
- **Passing methods individually instead of a dictionary.** Rejected: it
  duplicates the dictionary argument for every method, changes call shapes with
  the number of used methods, and does not support superclasses or an instance
  context compactly.
- **Dictionary as a Wasm `struct` type constructed by the emitter.** Rejected:
  representation is chosen by P9's representation planner, not by the encoder;
  a class-specific Wasm type would leak the class system below Typed Core.

## Algorithms

### Resolution and dictionary construction

Resolution runs when Typed Core is produced and is driven by the constraint on
the use site. It is standard instance resolution with dictionary construction:

```text
resolve(constraint C τ, env):
    // Unify the constraint against every instance head for C.
    candidates = [ inst for inst in instances(C)
                   if unify(head(inst), C τ) succeeds ]
    if candidates is empty:
        report "no instance for C τ" at the constraint span
    if candidates has more than one:
        report overlap at the constraint span
    inst = the unique candidate
    context_values = [ resolve(sub, env) for sub in coerced_context(inst, τ) ]
    return dictionary_value(inst, context_values)
```

`dictionary_value` builds a record expression whose fields are the instance's
method implementations and superclass dictionaries, closing over
`context_values`. Superclass dictionaries for `inst` are themselves resolved by
`resolve` on the instance's superclass constraints.

### Elaboration to a dictionary arrow

```text
elaborate(declaration d with constraints [C_1 a, ..., C_n a]):
    body' = elaborate_body(d.body)
    return Lambda(dict_1, ... Lambda(dict_n, body'))

elaborate_body(use of method m at type τ):
    dict = dictionary_for(C, τ)          // a local parameter or a resolved value
    field = method_index(C, m)
    return Project(field, dict)

elaborate_body(use of superclass method m via S):
    dict = dictionary_for(C, τ)
    super = Project(super_index(C, S), dict)
    return Project(method_index(S, m), super)
```

Each projection is an ordinary `ProductGet`; each dictionary value is an
ordinary `ProductNew`. Because a class record mixes methods and superclass
dictionaries in one product, `method_index` and `super_index` must be assigned
once per class and used consistently by elaboration and by the verifier.

### Coherence checks

Coherence is checked when the class environment is built, before resolution is
used as an oracle:

```text
check_coherence(instances):
    for every pair (i, j) of instance heads of the same class:
        if heads(i) and heads(j) unify: report overlapping instances
    for every instance i:
        if context_size(i) > head_size(i): report context larger than head
    for every class:
        superclass graph is acyclic
        method and superclass field indices are unique within the class record
```

Names and orphan status remain frontend concerns; this document only requires
that a constraint resolves to exactly one dictionary so the backend lowering is
deterministic.

### Edge cases

- **Superclass chains.** `Ord a <= Eq a <= ...` projects one dictionary per
  link; each link is a `ProductGet`.
- **Recursive instances.** `instance eqList :: Eq a => Eq (List a)` builds a
  dictionary that recursively passes the element dictionary into the recursive
  method; resolution terminates because contexts are no larger than heads.
- **Method with a constrained type.** A method that itself needs a dictionary
  is a closure that takes the extra dictionary as a parameter; the caller
  resolves and supplies it at the use site.
- **Default methods.** A class default is a top-level function used when an
  instance omits the method field; the instance simply stores the default
  closure in that field.
- **Derived instances.** Deriving generates an ordinary instance and dictionary
  at elaboration time; it adds no backend representation (`FE-16`).
- **Erased polymorphism.** A dictionary passed through a polymorphic function
  is an ordinary erased value; recovery at the concrete consumer uses the
  erased protocol, not the dictionary.

## Code map

The class system spans the frontend that elaborates dictionaries and the backend
that consumes them as ordinary products and closures. This organization is
planned, not yet implemented; the implementation must conform to it.

```text
crates/psrs-hir/src/
  ty.rs
  types.rs
crates/psrs-resolve/src/resolver/type_resolution.rs
crates/psrs-kind/src/
  kind.rs
  check/infer.rs
crates/psrs-typecheck/src/
  class.rs
  solve.rs
  elaborate.rs
  signature.rs
crates/psrs-core/src/dictionary.rs
crates/psrs-backend/src/
  cc/representation.rs
  cc/layout/mod.rs
  cc/lower/record.rs
  cc/lower/lambda.rs
  cc/lower/call.rs
  mir/layout/mod.rs
  mir/lower/assignments.rs
```

Responsibilities and required types:

- `psrs-hir` must carry class and instance declarations and resolved constraints:
  `TypeDeclaration`, `TypeDeclarationKind::Class`, and `ClassMember` in
  `types.rs`, and `TypeKind::Constrained` in `ty.rs`. `psrs-resolve`'s
  `type_resolution.rs` must resolve class members, superclasses, and instance
  heads; `psrs-kind` must check that a class kind is `Constraint`.
- `psrs-typecheck` must own the class environment and constraint solving and
  must elaborate dictionaries. It must define:
  - a class record `ClassRecord { members, supers, method_indices, super_indices }`,
    whose method and superclass indices are fixed once per class;
  - an instance dictionary
    `InstanceDictionary { head, context, method_values, super_values }`;
  - a constraint arrow, the elaboration of `C a => t` to `Dict(C, a) -> t`.
  Required entry point in `elaborate.rs`:
  `fn elaborate_dictionaries(declarations, env) -> Result<TypedCore, Diagnostic>`,
  with the resolution oracle in `solve.rs`:
  `fn resolve_constraint(constraint, env) -> Result<Dictionary, Diagnostic>`.
  `class.rs` must check coherence when the environment is built, before the
  oracle is used, and must report a missing or ambiguous dictionary at the
  constraint span.
- `psrs-core` must represent an elaborated dictionary as an ordinary value in
  `dictionary.rs`; it must not add a class- or dictionary-specific Core node.
- The backend must consume dictionaries only through existing products and
  closures: `cc/representation.rs::Representation::Product` for the class
  record, `cc/layout/mod.rs` and `mir/layout/mod.rs` for the record layout,
  `cc/lower/record.rs` for `ProductNew`/`ProductGet`, and
  `cc/lower/lambda.rs`/`cc/lower/call.rs` for method closures. No backend stage
  may introduce a Wasm type dedicated to dictionaries or a runtime type tag
  ([polymorphism and erasure](polymorphism-and-erasure.md),
  [data representation](data-representation.md)).

## Invariants and verification

- Every class record has a fixed field order; method and superclass indices are
  unique and stable for the class's lifetime.
- Every dictionary value has exactly the class record's fields, each with the
  declared method or superclass dictionary type; the CC verifier checks product
  field shapes (`cc/verify/ops.rs`).
- Every method projection names a field in range with the expected shape; the
  MIR verifier checks `StructGet` against the planned concrete type
  (`mir/verify/instruction/mod.rs`).
- No runtime type tag is introduced anywhere; the erased protocol remains the
  only polymorphic mechanism ([polymorphism and erasure](polymorphism-and-erasure.md)).
- Coherence is checked once when the class environment is built; a constraint
  that does not resolve, or resolves ambiguously, is a source error and never
  reaches CC.
- `let`-bound constrained values bind their dictionaries once; a later use does
  not re-resolve.

## Worked example

```purescript
class Eq a where
  eq :: a -> a -> Boolean

class Eq a <= Ord a where
  compare :: a -> a -> Int

instance eqInt :: Eq Int where
  eq x y = eqIntImpl x y

instance ordInt :: Ord Int where
  compare x y = compareIntImpl x y

greater :: forall a. Ord a => a -> a -> Boolean
greater x y = compare x y == 1
```

The class records, with superclass fields last, are:

```text
Dict(Eq, a)  = { eq      : a -> a -> Boolean }
Dict(Ord, a) = { compare : a -> a -> Int, super : Dict(Eq, a) }
```

The instance dictionaries are product values:

```text
eqIntDict  = ProductNew [ eqIntImpl ]                       : Dict(Eq, Int)
ordIntDict = ProductNew [ compareIntImpl, eqIntDict ]       : Dict(Ord, Int)
```

`greater` elaborates to a dictionary arrow: `compare` becomes a projection from
the `Ord` dictionary applied to `x` and `y`, and `== 1` is the ordinary `Int`
equality primitive:

```text
greater = \dictOrd -> \x -> \y ->
    IntEq(Project(compare, dictOrd) x y, 1)
```

Here `Project(compare, dictOrd)` reads field 0 of the dictionary. A use of `eq`
through `Ord` projects the superclass first:
`Project(eq, Project(super, dictOrd))`, i.e. `ProductGet(0, ProductGet(1, dictOrd))`.
A use `greater 3 2` resolves `Ord Int` to `ordIntDict` and passes it, so method
selection is one or two field reads plus a call. The backend verifies these as
ordinary products and closures; there is no dictionary-specific Wasm type and
no runtime check of `a`.

## Boundaries and interfaces

- **From the frontend.** The type checker produces explicit dictionary evidence
  as Typed Core values; the class environment (members, superclasses, instance
  heads and contexts) is a frontend input.
- **To CC.** Products and closures only. CC has no class, instance, or
  constraint concept and must not gain one.
- **To MIR.** A dictionary is a GC `struct` with one field per method and
  superclass, planned by the ordinary product path
  ([data representation](data-representation.md)).
- **To optimization.** Specialization reads dictionaries but preserves the
  dictionary-passing semantics; it may not be the only encoding.
- **Not owned.** Resolution at the source level, overlap/orphan diagnostics, and
  functional dependencies (`FE-15`), deriving (`FE-16`), and higher-rank
  subsumption (`FE-18`).

## Open questions and future work

- **Superclass selection strategy.** The project may store superclass
  dictionaries as fields (chosen here) or reconstruct them by resolution at
  use sites; the field representation is simpler but enlarges every dictionary.
- **Default methods.** Whether defaults are copied into every instance or
  looked up through a shared table is an optimization choice.
- **Functional dependencies.** Improvement changes constraint solving but not
  the runtime dictionary shape; it is tracked by `FE-15`.
- **Associated types and `Coercible`/roles.** They add type-level information but
  should still erase to products and coercions, per `FE-16`.
- **Specialization policy.** Which dictionaries to specialize, and how to
  preserve semantics under it, belongs to the MIR optimization track
  (`BE-12`).
- **Diagnostics.** Overlap, missing-instance, and context-size diagnostics need
  official `errorCode` agreement (`L5` in
  [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md)).

## Implementation notes

Planned, not implemented. Class and instance syntax is represented in the CST,
and class members and superclasses reach HIR as `TypeDeclaration`/`ClassMember`,
but there is no instance companion in HIR, no class environment, and no
constraint solving. `psrs-typecheck` drops `TypeKind::Constrained` and rejects
polymorphic declarations, so CC and MIR never receive dictionaries and
currently accept monomorphic and rank-1 polymorphic programs only. Nothing in
this document depends on a current implementation.

## References

- Wadler, P. and Blott, S., *How to Make Ad-hoc Polymorphism Less Ad Hoc*
  (1989).
- Peyton Jones, S., Jones, M., and Meijer, E., *Type Classes: Exploring the
  Design Space* (1997).
- Hall, C., Hammond, K., Peyton Jones, S., and Wadler, P., *Type Classes in
  Haskell* (1996).
- [DEC-04](../../../decision/DEC-04-official-test-suite-roadmap.md): `FE-14`
  classes and instances.
- [DEC-07](../../../decision/DEC-07-runtime-representation-for-parameterized-adts.md)
  and [polymorphism and erasure](polymorphism-and-erasure.md): no runtime type
  tags.
- [CC IR](cc-ir.md) and [data representation](data-representation.md): products
  and closures.
