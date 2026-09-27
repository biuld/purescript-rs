# DEC-14 — Resource Handle Ownership via Explicit Drops

**Status:** Proposed
**Date:** 2026-09-27

## Context and constraints

WIT represents a resource as a handle in one of two modes: `own<T>` is an owned
handle that must be released with `resource.drop` exactly once, and `borrow<T>`
is a non-owning reference valid only for the dynamic extent of the call that
produced it.

[DEC-13](DEC-13-wit-to-source-type-mapping.md) maps aggregates to library types,
so a handle can now appear nested in a result: `result<own<T>, E>`, a record
field, a variant payload, or a `list<own<T>>`. The compiler's current ownership
tracking is per top-level SSA value: it records an obligation for a whole
`own<T>` result and inserts `resource.drop` at the function end, and it releases
a whole `borrow<T>` result after the call. It cannot see a handle nested inside
an aggregate, because the boundary value is the aggregate, not the handle.

PureScript has no linear or affine types, so the compiler cannot statically
enforce "drop exactly once". Wasm GC has no finalizers, so a collection-based
release is unavailable.

Industry tooling (wit-bindgen) maps ownership onto each host language: Rust uses
native ownership and `Drop`; C and Go expose explicit drop functions; JavaScript
and Python use finalizers plus an explicit dispose. No generator has the compiler
do dataflow tracking of ownership through aggregates.

## Decision

The standard library owns the resource lifetime discipline. The compiler does
not insert `resource.drop` on its own and does not track ownership through
aggregates.

- A resource handle's source representation is a nominal opaque type
  (`foreign import data`) or the `Int` placeholder. No new compiler type is
  introduced for ownership.
- The compiler exposes the canonical `resource.drop` to source as a
  source-callable drop (a resource-drop intrinsic or declaration). The standard
  library calls it at the right point: when an owned handle is consumed or
  discarded, and to release a borrowed handle at the end of its call scope.
- A handle nested in an aggregate is an ordinary value of the aggregate. The
  standard-library wrapper that destructures the aggregate is responsible for
  dropping each owned handle it extracts.
- A `borrow<T>` in a result is rejected: the borrow scope is the call that
  produced it, which has ended.
- The standard library may provide an ordinary `Resource a` wrapper with an
  explicit `drop` and a bracket-style `withResource` for ergonomics. It is
  library code, not a compiler type, and it does not enforce linearity; it only
  makes the explicit-drop discipline convenient and hard to forget on the normal
  path. For example:

  ```purescript
  foreign import data Resource :: Type -> Type
  drop :: forall a. Resource a -> Effect Unit
  withResource :: forall a b. Resource a -> (Resource a -> Effect b) -> Effect b
  ```

## Consequences

- Consistent and industry-aligned: no compiler guessing and no ownership
  dataflow through aggregates. The same model covers top-level and nested
  handles.
- The compiler's existing automatic drops are removed: the top-level `own<T>`
  result drop and the `borrow<T>` result release move to the standard library.
  Existing handle tests and the standard-library wrappers are updated.
- Correctness of "drop exactly once" is a runtime discipline in the library, as
  it is in C and Go. A program that forgets to drop leaks a handle; the compiler
  does not enforce it.
- The resource-drop intrinsic must be reachable from source, so the standard
  library can be written in ordinary PureScript.

Rejected alternatives:

- **Compiler dataflow tracking through aggregates.** Costly, error-prone, and
  still cannot enforce "exactly once" without linear types.
- **Automatic drop at the boundary or the function end.** Wrong when the handle
  escapes into a returned aggregate or is still in use.
- **A GC finalizer.** Wasm GC has no finalizer mechanism.
- **Ownership wrapper types (`Own a` / `Borrow a`) in the compiler.** They
  cannot enforce linearity and grow the language; the library may add ordinary
  wrappers for documentation if useful.
