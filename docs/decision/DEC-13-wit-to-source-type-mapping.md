# DEC-13 — WIT-to-Source Type Mapping for Aggregates

**Status:** Proposed
**Date:** 2026-09-27

## Context and constraints

[DEC-12](DEC-12-resolved-wit-bindings.md) removed `SourceType`; the source side
of the ABI is now the declaration's resolved Core type plus the WIT descriptor.
[DEC-11](DEC-11-primitive-ffi-stdlib-wrappers.md) kept the source side to
primitives and `Array` and did not recognize `Maybe`, `Either`, or tuples. Its
stated mechanism ("do not grow `SourceType`") no longer applies: there is no
`SourceType` to grow.

A complete WASI standard library needs the higher-level WIT forms. The vendored
interfaces already require them:

- `wasi:cli/environment#get-environment` returns `list<tuple<string, string>>`.
- `wasi:filesystem#get-directories` returns `list<tuple<descriptor, string>>`.
- `wasi:filesystem#read` returns `result<tuple<list<u8>, bool>, error-code>`.
- `wasi:io/streams#read` returns `result<list<u8>, stream-error>`.
- `wasi:sockets` sends `list<outgoing-datagram>` and receives
  `result<list<incoming-datagram>, error-code>`.

PureScript already has idiomatic, common types for every one of these forms.
No new compiler type vocabulary is required: records (the frontend already treats a tuple as a closed record, FE-06), `Data.Maybe.Maybe`, `Data.Either.Either`, and
ordinary user data types. The functional shape of WIT maps directly onto them.

## Decision

Map each WIT aggregate form to the existing, idiomatic PureScript type. The
mapped types are ordinary library types, not compiler builtins. The compiler
carries the mapping as a small, documented table and validates it against the
declaration's resolved Core type.

| WIT form | Source type |
| --- | --- |
| `tuple<A, B, ...>` | a closed record `{ _1 :: A, _2 :: B, ... }` |
| `option<T>` | `Data.Maybe.Maybe T` |
| `result<O, E>` | `Data.Either.Either E O` |
| `result<_, E>` | `Data.Either.Either E Unit` |
| `result<O, _>` | `Data.Either.Either Unit O` |
| `result` | `Data.Either.Either Unit Unit` |
| `variant { ... }` | a source data type whose constructors correspond to the WIT cases in order |
| `enum { ... }` | a nullary source data type (existing) |
| `record { ... }` | a closed source record (existing) |
| `flags { ... }` | a closed source record of `Boolean` (existing) |
| a multi-value return | a closed record; a `result` return is `Either` |

Every `result` maps to `Either` with the error on `Left` (`Left = err`,
`Right = ok`), following the PureScript idiom. The canonical ABI orders a
`result`'s cases `[ok, err]` while `Either`'s constructors are
`Left`(err), `Right`(ok), so lowering swaps the discriminant: a canonical `ok`
builds or reads the `Right` tag, and a canonical `err` the `Left` tag. Each
payload position that is absent in the WIT declaration is a nullary case, so its
source field is `Unit`. There is no `Unit`/trap special case: a unit-success
`result<_, E>` decodes its error payload and builds an `Either`, exactly like a
`result<O, E>`.

- The compiler recognizes `Maybe` and `Either` by their standard-library
  qualified names (`Data.Maybe.Maybe`, `Data.Either.Either`), not by arbitrary
  constructor names. `Nothing`/`Just`/`Left`/`Right` are the library's
  constructors; the mapping belongs to the named library type, so two unrelated
  two-case ADTs are not confused.
- A WIT `variant` maps to a data type whose constructors are taken in WIT case
  order, exactly as an `enum` maps to a nullary data type today. The type is
  user- or library-declared, not a compiler builtin.
- These forms lower through the resolved-type path fixed by
  [DEC-12](DEC-12-resolved-wit-bindings.md): the frontend resolves the source
  type, the linking stage interns it and validates it against the WIT
  descriptor, CC derives the layout, and MIR lowers from the CC signature and
  the descriptor. `SourceType` is not reintroduced.
- DEC-11's mechanism clause ("do not grow `SourceType`"; "do not recognize
  `Maybe`/`Either`/tuples") is superseded by this decision. DEC-11's two-layer
  rule stands: the standard library wraps foreign imports in ordinary
  PureScript, and the compiler does not grow a parallel type vocabulary.

## Consequences

- The standard library can expose the full WASI surface: environment,
  filesystem, stream `read`, and sockets, not just the current CLI/clock/random
  subset.
- The compiler gains a small name table for `Maybe` and `Either`, not a type
  system. `Data.Maybe` and `Data.Either` remain ordinary source modules; the
  mapped types are the same types programs already use.
- Frontend support is required for the mapped types (data types, closed
  records, `Maybe`, `Either`) and for multi-constructor matching; that work is
  already on the FE-05/FE-06/FE-12 roadmap.
- Aggregate runtime representation (variants, nested records, lists of
  aggregates) is the same canonical ABI the backend already models; no new
  compiler type layer is added.

Rejected alternatives:

- **Compiler builtins for `Maybe`, `Either`, or tuples.** The library types are
  the idiomatic ones; a builtin would duplicate them and force a name on user
  code.
- **A dedicated WIT aggregate type in the compiler.** Reintroduces the
  vocabulary DEC-12 removed.
- **Structural recognition of any two-case ADT by constructor shape.** Two
  unrelated ADTs would be treated as `option`/`result`; recognition is by the
  named library type instead.
- **A project-specific encoding of tuples as anything other than a record.**
  FE-06 already fixes a tuple as `{ _1, _2, ... }`.

