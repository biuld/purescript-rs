# DEC-11 — Primitive Foreign Imports and Standard-Library Wrappers

**Status:** Accepted
**Date:** 2026-09-26

## Context and constraints

[DEC-06](DEC-06-runtime-interface-via-wit.md) made WASI 0.2 the host interface
and the canonical ABI lowerer the glue. There is no user-written foreign file
and no project-specific host ABI. A PureScript foreign import still has one
result, and its declared type is a PureScript type, not a WIT type.

CC and MIR keep runtime shapes only. They drop the source name that would tell
an `i32` apart as `Int`, `Char`, an enum tag, or a handle, and they drop record
field names and enum case order. `SourceType` is captured from the foreign
import before CC so the lowerer can tell those primitives apart. `WasiParamKind`
is the WIT side of that boundary. WIT names stay out of CC and MIR.

WIT `option`, `result`, and tuples are a discriminant plus a payload, or
several core values. Issue #56 asked the type checker and the canonical ABI
mapping to grow source types for `Maybe`, `Either`, and tuples so the compiler
could lower those forms directly. That would make library types part of the
compiler and would use `SourceType` for something other than primitive
identity.

Official PureScript types the foreign import with a PureScript type, and a
separate foreign file adapts the runtime representation to the host. This
compiler has no such file. The adaptation of `Maybe`, `Either`, enums, flags,
and records is ordinary PureScript in the standard library. The foreign import
the lowerer sees uses only primitive types. The lowerer flattens those
primitives; it does not adapt `Maybe`.

## Decision

The standard library and the lowerer are two layers. The mechanism is
[primitive foreign imports and standard-library wrappers](../design/backend/wasm/primitive-ffi-and-stdlib.md).

- A foreign import the lowerer accepts uses only `Int`, `Boolean`, `Number`,
  `Char`, `String`, and `Unit`. A resource handle is declared as `Int`.
  `String` stays one source argument; the lowerer already expands it to
  `(pointer, length)`.
- The lowerer does not grow `SourceType` for higher-level WIT forms. It does
  not add `Option`, `Result`, or `Tuple`, and it does not recognize `Maybe`,
  `Either`, or tuples by constructor name or by `_1`/`_2` labels.
- The user-facing API is ordinary PureScript over unexported primitive
  imports. Wrappers may use `Maybe`, `Either`, enums, flags, records, and
  tuples. They pass primitives in canonical order. Flattening those primitives
  must equal `Resolve::wasm_signature` for that WIT function. Discriminants
  and dummy payloads are written in PureScript.
- A foreign import has one result, which the lowerer rebuilds as one
  primitive. The unit-success `result` that traps on failure stays `Unit`.
  A canonical result that is several values is not wrapped and is not given a
  compiler source type until the whole result is one primitive. A tuple return
  is not added to carry the extra words.
- Raw imports are not exported. Users do not pass pointers, lengths, or
  discriminants.

Today's lowerer also accepts nullary enums, closed records, and flags records.
That path stays. It is not how the standard library grows, and this decision
does not extend it.

## Consequences

- Standard-library authors write the glue that DEC-06 did not leave as a
  separate file. Adding a service whose parameters flatten to primitives does
  not change the compiler. Users see library types only.
- `SourceType` remains the primitive identity CC and MIR cannot recover. It is
  not an encoding of `Maybe` or `Either`.
- WIT functions whose results are several canonical values stay unreachable
  from PureScript until that result is one primitive. The compiler will not
  paper over that with a tuple.
- The approach in issue #56, growing compiler source types for `Maybe`,
  `Either`, and tuples, is not adopted. Those types stay in the library.
- Rejected with the mechanism: compiler builtins for those types, treating
  every `Nothing`/`Just` or `Left`/`Right` as WIT `option`/`result`, putting
  WIT or HIR types into CC or MIR, a project host ABI or a hand-written
  Preview 1 call, and a public API that takes pointers or discriminants.
