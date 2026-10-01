# DEC-15 — Unified Application-Spine Type Representation

**Status:** Proposed
**Date:** 2026-09-29

## Context and constraints

The frontend type-system design fixes the checked-type spine:

```text
CheckedType = Var | Constructor | Application | KindApplication | ForAll
            | Constrained | RowEmpty | RowExtend | TypeLevelString
            | TypeLevelInt | Skolem
```

An arrow is application of the function kind constructor, and the primitive
constructors are `Row`, `Record`, `Function`, and `Array`
([kinds](../design/frontend/type-system/kinds.md),
[type inference](../design/frontend/type-system/type-inference.md)). Official
PureScript uses exactly this shape: `Type a` is
`TUnknown | TypeVar | TypeLevelString | TypeLevelInt | TypeConstructor | TypeApp |
KindApp | ForAll | ConstrainedType | Skolem | REmpty | RCons | KindedType | ...`,
and `b -> c` is `TypeApp (TypeApp (TypeConstructor C.Function) b) c`
(`Language/PureScript/Types.hs`). A record is `TypeConstructor C.Record` applied
to a row; rows are the dedicated nodes `REmpty` and `RCons`; quantifiers,
constraints, kinds, and type-level literals are dedicated structural nodes.

THIR and Typed Core currently deviate from that design. `psrs_thir::Type` and
`psrs_core::Type` add ad-hoc variants `Function { parameter, result }`,
`Record([(String, TypeId)])`, `OpenRecord`, and inline primitive variants
`I32 | F64 | Boolean | String | Char | Unit`, with a small
`TypeConstructor = Array | User(HirTypeId)` head. Runtime arity and calling
convention are read from the bare arrow variant instead of from a general
application spine. A library type such as `Effect (b -> c)` is only an
application of `User(effect_id)`. The spine does not record a calling
convention for it, and no later pass should invent one by matching that
constructor.

The constraint is to keep the frontend and both typed IRs on one representation
without introducing an effect-specific compiler type, flag, or node, and without
sniffing a well-known type name to decide arity.

## Decision

THIR and Typed Core adopt the uniform application-spine type representation
already specified for the frontend:

```text
Type = Variable
     | Constructor(TyCon)
     | Application(Type, Type)
     | KindApplication
     | ForAll
     | Constrained
     | RowEmpty
     | RowExtend
     | TypeLevelString
     | TypeLevelInt
     | Skolem
```

Typed Core omits inference-only nodes such as `TUnknown` and wildcards; THIR may
retain them only where P5 still needs them, never past the checked boundary.

- `Constructor` covers `Function`, `Record`, `Array`, `Int`, `Number`,
  `Boolean`, `String`, `Char`, `Unit`, `Row`, and `User(HirTypeId)`.
- `a -> b` is `Application(Application(Constructor(Function), a), b)`.
- A record is `Application(Constructor(Record), row)`, where a row is
  `RowEmpty` or `RowExtend`; a tuple is the closed record `{ _1, _2, ... }`.
- `Array a` is `Application(Constructor(Array), a)`.
- Primitive and user constructors are heads on the same spine; nothing is
  special-cased by syntax.

Runtime arity of a source function is the curried `Function` spine, read until
the result is no longer an arrow. It is not read from a bare arrow variant and
not from a library constructor. `Effect a` stays `User(effect_id)` applied to
`a` through THIR and Typed Core. One representation lowering, specified in
[effects](../design/backend/fp/effects.md), is the only pass that recognizes
that constructor: it emits a closure whose parameter list is the runtime token
and whose result is the lowering of the type argument. `Effect (b -> c)` is
therefore one token parameter and a function value, not a two-parameter call.
There is no effect-specific Core node, no effect token type, and no callable
side table for later passes to consult.

This aligns the project with official PureScript's `TypeApp` while keeping rows,
quantifiers, constraints, and kinds as dedicated structural nodes exactly as
official does.

## Consequences

- `Effect (b -> c)`, `Effect (Effect a)`, and higher-order effects keep their
  source arity out of the type spine. The representation lowering gives each
  `Effect` layer its own closure; a function or effect in the result is another
  call.
- Rows, classes, kinds, and higher-kinded types get one representation to build
  on, so the type-level features already planned have a single shape to target.
- The migration is large but mechanical: every site that matches on
  `psrs_thir::Type` or `psrs_core::Type` moves from the ad-hoc
  `Function`/`Record`/inline-primitive/`OpenRecord` variants to the spine. It is
  staged, beginning with the representation and lowering sites that already need
  a head constructor.
- Until the migration completes, the ad-hoc variants are the deviation from the
  frontend design; the design documents record the target, not a second
  representation.
- Every backend pass that needs a source arity reads the `Function` spine.
  After representation lowering, an effect is already a closure value, so those
  passes do not match the `Effect` constructor.

## Rejected alternatives

- **Keep the ad-hoc `Function`/`Record`/inline-primitive variants (the current
  deviation).** Rejected: it cannot express a polymorphic effect's arity without
  an effect-specific rule, and it diverges from the already-fixed frontend
  `CheckedType` and from official PureScript's `TypeApp`.
- **Add a `Type::EffectToken` variant.** Rejected: it makes the effect encoding a
  Core type and bakes one library's representation into the shared IR, so a
  different effect representation or a new effect library would need a new Core
  type.
- **Add a dedicated effect Core node.** Rejected: an effect is an ordinary value
  and effects compose through `pure`, `bind`, and `runEffect`; a dedicated node
  would couple Core and the backend to the effect library, contradict
  [DEC-03](DEC-03-purescript-faithful-type-system.md), and add a verifier path
  for no semantic gain.
