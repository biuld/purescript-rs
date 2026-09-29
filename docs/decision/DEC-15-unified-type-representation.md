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
application spine, so a polymorphic effect such as `Effect (b -> c)` or
`Effect (Effect a)` has no uniform place to record that the head constructor is
`Effect` and how its application is represented.

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

Runtime arity and calling convention are derived from the application spine and
the head constructor's representation, never from bare arrow syntax and never by
sniffing a specific type name. The `Effect` monad's runtime representation — a
closure with one hidden context parameter whose result is a value — is
registered by the trusted elaboration by type identity. That registration is a
representation and calling-convention detail, not a Core type. There is no
effect-specific node and no effect-specific token type.

This aligns the project with official PureScript's `TypeApp` while keeping rows,
quantifiers, constraints, and kinds as dedicated structural nodes exactly as
official does.

## Consequences

- Polymorphic-effect arity is fixed without a Core token type: `Effect (b -> c)`,
  `Effect (Effect a)`, and higher-order effects all reduce to the head
  constructor of the spine plus its registered representation.
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
- Every backend pass that reads a type head must consult the spine and the head
  constructor's registered representation rather than inspecting a bare arrow,
  which is the property that removes the effect-specific special case.

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
