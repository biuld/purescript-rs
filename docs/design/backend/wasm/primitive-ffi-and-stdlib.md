# Primitive Foreign Imports and Standard-Library Wrappers

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Draft  
**Prerequisites:** the Canonical ABI's flat signature (`Resolve::wasm_signature`: scalars, `string` as `(pointer, length)`, `option`/`result` as a discriminant plus a payload), and the difference between a PureScript foreign import and the host glue behind it. Read [DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md), [DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md), [canonical ABI and WIT](canonical-abi-and-wit.md), and [WASI platform library](wasi-platform-library.md) first.  
**Summary:** The host interface stays the WASI 0.2 canonical ABI. Foreign imports the lowerer is specified to accept use primitive source types and `Array` of a supported element for a non-byte `list<T>`, and the user-facing standard library is ordinary PureScript that wraps those imports. The compiler does not grow a source type for `Maybe`, `Either`, or tuples. A wrapper may pass a WIT aggregate only as primitive arguments whose flattening matches the canonical signature; a multi-value return stays unavailable.

## Scope

This document owns the two-layer contract between the canonical ABI lowerer and
the standard library: the primitive source types a foreign import may use, what
the lowerer refuses to recognize, how a wrapper encodes and decodes library
types, and why `SourceType` exists. It is the mechanism behind
[DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md).

It does not own canonical flattening, `lift`/`lower`, or `cabi_realloc`
([canonical ABI and WIT](canonical-abi-and-wit.md)), buffer lifetime
([linear memory boundary](linear-memory-and-canonical-abi-boundary.md),
[canonical buffer allocation and lifetime](canonical-buffer-allocation-and-lifetime.md)),
the component world and which WASI services exist
([WASI platform library](wasi-platform-library.md)), or frontend type checking of
`Maybe`, `Either`, records, and tuples. Those library types are ordinary
PureScript; this topic only says they are not a compiler ABI.

## Background

**Official PureScript.** A `.purs` foreign import is typed with a PureScript
type. The foreign implementation, historically a JavaScript file the compiler
does not see, adapts that representation to the host. The type in the `.purs`
file is the library shape, not the host ABI. This compiler has no user-written
glue file. [DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md) makes
the canonical ABI lowerer that glue: it binds a source declaration to a WIT
function and flattens the call. The primitive `foreign import` is the raw
layer. The exported PureScript functions are the standard API.

**What CC and MIR still know.** CC and MIR keep runtime shapes only: `i32`,
float, a GC reference, a string. They drop source type names, record field
names and order, enum case order, and which `i32` is `Int`, `Char`, an enum
tag, or a resource handle. A later pass cannot recover those distinctions from
the IR. WIT names stay out of CC and MIR for the same reason
([DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md),
[canonical ABI and WIT](canonical-abi-and-wit.md)).

**Why `SourceType` exists.** The foreign import's source signature is copied
into an `ExternalBindings` side table before CC. P9 reads that `SourceType`
when it lowers the call. `WasiParamKind` is the WIT side of the same boundary:
it says which canonical slot the value fills. `SourceType` is how the lowerer
tells the primitive representations apart. It is not a place to encode `Maybe`
or `Either`.

**Canonical aggregates.** WIT `option` and `result` are a discriminant plus a
payload. In the flat ABI the payload occupies its canonical slots even for the
empty case; a correct host does not read that payload when the discriminant
says it is absent. A tuple or a payload-bearing variant is likewise several
core values, or several words in a return area, not one PureScript result.
Issue #56 asked the type checker and this mapping to grow source types for
`Maybe`, `Either`, and tuples. [DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md)
replaces that approach: those types stay in the library, and the compiler
mapping does not learn them.

## Model

### The primitive set

A foreign import the lowerer accepts uses only these source types:

| Source type | Canonical role |
| --- | --- |
| `Int` | one `i32` or `i64` integer slot, or a resource handle (`i32`) |
| `Boolean` | canonical `0`/`1` |
| `Number` | `f32` or `f64` |
| `Char` | canonical `i32`; not an `Int` and not a handle |
| `String` | one source argument; the lowerer expands the GC string to `(pointer, length)` |
| `Unit` | the one result of an empty return, or of the unit-success `result` that traps on failure |

`String` is in this set even though its canonical form is two core values. The
lowerer already expands a GC string; the import does not take a pointer and a
length. A resource handle is an `i32` at the call and is declared as `Int`,
which is what `WASI.Console` already does for `get-stdout`. There is no seventh
source type for handles.

`Unit` is a result type. It contributes no canonical parameter, so a `Unit`
parameter does not validate.

### Non-byte lists

A WIT `list<T>` that is not a byte list lowers to the source type `Array a`,
where `a` is a supported element type. It is the one parameterized source type a
foreign import may use:

| Source type | Canonical role |
| --- | --- |
| `Array Int`, `Array Boolean`, `Array Number`, `Array Char`, `Array String` (and the narrowed-integer, `s64`/`u64`, and `f32` extensions) | a non-byte `list<T>`: the lowerer copies elements between the GC array and the canonical `(pointer, length)` buffer |

`Array String` is a `list<string>`: its elements are themselves `(pointer,
length)` pairs, transcoded element-wise and freed after the call. A `list<u8>`
stays `String`, not `Array Int`. A nested `Array (Array _)`, an array of
records, handles, tuples, or `option` values has no source mapping and is
rejected. `Array` is not a compiler encoding of any aggregate; it is the source
type for a non-byte WIT list of an element that already maps.

### Two layers

```text
user-facing function  :: library types -> Effect _
        |  case / pack / unpack, in PureScript
        v
unexported foreign import :: primitives -> one primitive
        |  SourceType + WasiParamKind, at P9
        v
Resolve::wasm_signature   flat core values, linked by componentize
```

The user-facing module may use `Maybe`, `Either`, enums, flags, records, and
tuples. Those are ordinary library types. A wrapper `case`s on them and calls
an unexported primitive `foreign import`. Raw foreign imports are not part of
the user API. Parameters are primitives in canonical order. Flattening those
primitives must equal `Resolve::wasm_signature` for that WIT function, so
componentize links. The binding string selects the WIT function. Validation
checks the flat shape; it does not check that a discriminant is a legal tag.
An illegal tag is a bug in the wrapper, as it would be in hand-written glue.

A PureScript foreign import has one result. The lowerer may rebuild one
primitive (`Int`, `Boolean`, `Number`, `Char`, `String`, or `Unit`). The
existing unit-success `result` that traps on failure stays a `Unit` return.
A WIT `option`, `result`, or `variant` whose canonical form is several values
in a return area is not wrapped, and is not given a compiler source type, until
that whole result can be expressed as one primitive.

### What `SourceType` is not

`SourceType` has no `Option`, `Result`, or `Tuple` form. The lowerer does not
recognize `Maybe`, `Either`, or tuples by constructor name (`Nothing`/`Just`,
`Left`/`Right`) or by `_1`/`_2` record labels. Its `Array` form is not an
aggregate encoding either: it exists only for a non-byte WIT `list<T>` whose
element already maps.

The normative set for a new standard-library foreign import is the primitives
above, plus `Array` of a supported element for a non-byte `list<T>`. A `Maybe`,
`Either`, tuple, `option`, or other new aggregate still
produces no `SourceType` and is rejected.

The existing enum, closed-record, and flags-record lowering still accepts
those declarations and is not deleted. They keep a source signature and still
lower. They are not the set new standard-library imports use.

## Design

The standard library grows by writing PureScript. The compiler does not gain
builtins for `Maybe`, `Either`, or tuples, and it does not treat every
`Nothing`/`Just` or `Left`/`Right` ADT as WIT `option`/`result`. Discriminant
values and dummy payloads for the empty case are written in the wrapper. They
follow the Canonical ABI: for `option`, `0` is none and `1` is some; a correct
host does not read the none payload. The dummy payload is a value of the
primitive the flat signature requires (an empty `String`, a zero `Int`).

The shape of a parameter wrapper, not a current library function:

```purescript
foreign import raw :: Int -> String -> Unit

send m = case m of
  Nothing -> raw 0 ""
  Just s  -> raw 1 s
```

`raw` is not exported. `send` is. Callers pass `Maybe String`. They do not pass
pointers, lengths, or discriminants.

Returns stay one value. The wrapper may `case` on that one primitive after the
call — for example map an `Int` tag onto a library enum — only when the
canonical result rebuilds as that primitive. It must not invent a tuple return
to smuggle a discriminant out beside a payload. If the canonical return is
several values, the foreign import is rejected and the wrapper has nothing to
`case` on.

`WASI.Console` is the pattern already in the library: `log` is the exported
function, and `writeStdout` is the primitive import
(`Int -> String -> Unit`, handle then byte list). The correction is that
`writeStdout` must not be exported. The same rule applies to every raw import
in the module (`getStdout`, `getStderr`, and the clock's `monotonicNow`): the
module export list names the wrappers only.

### Current lowering that remains

Today's lowerer also accepts nullary enums, closed records, and flags records,
and it projects record fields in WIT order. This decision does not delete or
rewrite that path. It is current lowering, not the way the standard library
grows. New foreign imports in the standard library use the primitive set.
`SourceType` is not extended for `option`, `result`, non-unit `variant`, tuple,
or any other aggregate WIT form. A public enum, flags record, or record is
still ordinary PureScript: the wrapper passes the tag `Int`, the packed flag
words, or the fields as primitive arguments in canonical order.

### Rejected alternatives

- **Teaching the compiler that any `Nothing`/`Just` or `Left`/`Right` ADT is
  WIT `option`/`result`.** Rejected: those names are library constructors, not
  an ABI. Two unrelated ADTs would lower differently, or the same ADT would be
  ambiguous, and the compiler would own types the standard library defines.
- **Compiler builtins for `Maybe`, `Either`, or tuple.** Rejected: official
  PureScript keeps them in the library. A builtin would also put a source type
  identity into the lowering, which is what `SourceType` is not for.
- **Putting WIT types or HIR types into CC or MIR.** Rejected: CC and MIR keep
  runtime shapes. The side table already carries the source signature
  ([DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md)).
- **A project-specific host ABI, or a hand-written Preview 1 call.** Already
  rejected by [DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md).
- **Requiring users to pass pointers, lengths, or discriminants at the public
  API.** Rejected: that leaks the canonical ABI into user code. The unexported
  import may take a discriminant; the exported function may not.
- **A tuple (or other aggregate) return whose only purpose is to expose a
  multi-value canonical result.** Rejected: a foreign import has one result.
  Inventing a tuple would be a compiler source type for a WIT shape this
  decision refuses.

## Algorithms

Flattening of `String` to `(pointer, length)`, integer narrowing, and the
return-area layout are defined by
[canonical ABI and WIT](canonical-abi-and-wit.md). The steps here only decide
whether a primitive declaration matches that flat signature, and whether a
wrapper is allowed to observe the result.

### Validating a primitive foreign import

```text
validate_primitive_import(binding, wit_function):
    signature = binding.signature
    if signature is missing:
        reject "source type is not a primitive foreign-import type"
    canonical = Resolve::wasm_signature(wit_function, GuestImport)
    # core parameter types, excluding a trailing return pointer
    expected = canonical.params without retptr

    if any source type in signature.parameters or signature.result
       is outside {Int, Boolean, Number, Char, String, Unit}:
        reject
    if any parameter is Unit:
        reject "Unit is not a canonical parameter"

    flat = []
    for source in signature.parameters:
        flat.extend(flatten_primitive(source))
        # Int | Boolean | Char | Number | handle-as-Int -> one core value
        # String -> (pointer, length)
        # the expansions themselves are the canonical ABI's
    if flat != expected:
        reject "primitive flattening does not match the canonical signature"

    visible = visible_return(canonical, wit_function)
    if visible is Reject:
        reject "multi-value return is not one primitive"
    if signature.result != visible:
        reject
```

WIT-level arity and source arity need not match. `option<string>` is one WIT
parameter; the import is `Int -> String -> Unit` because the flat form is a
discriminant plus `(pointer, length)`. The binding's interface and function
name select the WIT function. The comparison is against
`Resolve::wasm_signature`, not against a `SourceType` for the aggregate.

`flatten_primitive` does not inspect `Maybe`, constructor names, or record
labels. A declaration written as `Maybe String -> Unit` never reaches this
comparison: projecting its source type fails, `signature` is missing, and the
import is rejected.

The lowerer still distinguishes an `Int` from a `Char` even though both can be
`i32`. `Char` matches only a WIT `char`. `Int` matches an integer slot or a
handle. That distinction is `SourceType` beside `WasiParamKind`; it is not
recoverable from the `i32` in MIR.

### What a wrapper may see of a return

```text
visible_return(canonical, wit_function):
    if the result is the unit-success result
       (success payload empty, failure not returned to source):
        return Unit          # lowerer traps on a nonzero discriminant
    values = canonical result values
             (the single core result, or the words in the return area)
    if values rebuild to exactly one of Int, Boolean, Number, Char, String:
        return that primitive
    return Reject
```

A wrapper is allowed to see a return only by calling a foreign import that
validates. Concretely:

- **Allowed.** The canonical result is one primitive. The wrapper receives
  that primitive and may `case` on it in PureScript. The unit-success `result`
  is the `Unit` case of this rule: the wrapper sees `Unit`, not the
  discriminant, and failure traps inside the lowerer.
- **Not allowed.** The canonical result is several values (a discriminant plus
  a payload, a tuple, a record, or a non-unit variant in the return area).
  The foreign import is rejected. The wrapper cannot bind those words, and the
  compiler does not add a tuple or ADT result to carry them. The service stays
  unexposed until the whole result can be expressed as one primitive.

The wrapper never receives the return area, the return pointer, or a partial
payload. Packing and unpacking of parameters happen before the call, in
PureScript, by passing primitives. Unpacking of a result happens after the
call only when `visible_return` produced one primitive.

## Code map

The library and the side table must be organized as follows. Packaging, the
application world, and embedding stay in
[WASI platform library](wasi-platform-library.md). Flattening stays in
[canonical ABI and WIT](canonical-abi-and-wit.md).

```text
stdlib/lib/
  Prelude.purs                 ordinary library types (Effect)
  Data/Maybe.purs              data Maybe a = Nothing | Just a, plus eliminators
  Data/Either.purs             data Either a b = Left a | Right b, plus eliminators
  WASI/Console.purs            export list is the wrappers only
  WASI/Clock.purs              same split
crates/psrs-backend/src/
  bindings.rs                  ExternalBindings side table
  abi.rs                       SourceType, SourceSignature, validate_signature
  mir/wit/                     lower a validated primitive call
```

- A platform module exports only user-facing functions. `WASI.Console` exports
  `log :: String -> Effect Unit` and `error :: String -> Effect Unit`. It does
  not export `writeStdout`, `getStdout`, or `getStderr`. `WASI.Clock` exports
  `now :: Effect Int` and does not export `monotonicNow`. `WASI.Exit` exports
  `exitWithCode :: Int -> Effect Unit` and does not export `exitWithCodeRaw`.
- Each raw binding is an unexported `foreign import` whose parameters and
  single result are in the primitive set. The binding string is
  `<interface>#<function>`, as in the canonical ABI topic.
- `ExternalBindings::from_core` runs before CC. It copies each WIT foreign
  import's source signature into `SourceSignature`. A type outside the
  primitive set (and outside the current enum/record/flags lowering) is stored
  as a missing signature and rejected. CC and MIR receive the symbol and the
  runtime shapes, not `SourceType` and not WIT names.
- `SourceType` is `Int | Boolean | Number | Char | String | Unit`, plus the
  existing `Enum` and `Record` forms the current lowerer already has. It must
  not gain `Option`, `Result`, or `Tuple`. No function recognizes `Nothing`,
  `Just`, `Left`, `Right`, or `_1`/`_2` as a WIT shape.
- `WasiRegistry::validate_signature` implements `validate_primitive_import`
  for imports in this set: primitive flattening equals
  `Resolve::wasm_signature`, and `visible_return` is the one declared result.
  `mir::wit::lower` lowers a call that has already passed that check. It
  expands `String`, pushes a handle `Int` as `i32`, and rebuilds one primitive
  result. It does not grow a new aggregate lowering for `option` or `result`.

The current enum, record, and flags lowering stays behind the same
`validate_signature` and `mir::wit::lower` entry points. New standard-library
imports do not use it.

## Invariants and verification

- Every standard-library foreign import that this contract admits uses only
  `Int`, `Boolean`, `Number`, `Char`, `String`, and `Unit`, with handles
  declared as `Int`.
- Flattening those parameters equals `Resolve::wasm_signature` for the bound
  WIT function, excluding the return pointer. A mismatch is a diagnostic on
  the declaration, including when the declaration is unused.
- The import's source result is exactly `visible_return`. A multi-value
  canonical result is rejected, not approximated and not returned as a tuple.
- The unit-success `result` is a `Unit` return that traps on failure. The
  wrapper does not observe the discriminant.
- Exported platform names are wrappers. Raw imports are absent from the module
  export list. Tests that import `WASI.Console` can name `log` and cannot name
  `writeStdout`.
- `SourceType` carries primitive identity that CC and MIR drop. It does not
  carry `Maybe`, `Either`, or tuple structure. WIT interface and function names
  remain only on `ExternalBindings` and the `WasiRegistry`.
- A foreign import declared at `Maybe`, `Either`, or a tuple type is rejected
  before MIR emits a call. Constructor names are not consulted.

## Worked example

### `log`, as the library defines it today

The embedded console module is ordinary PureScript:

```purescript
foreign import "wasi:cli/stdout#get-stdout" getStdout :: Int
foreign import "wasi:io/streams#[method]output-stream.blocking-write-and-flush"
  writeStdout :: Int -> String -> Unit

log :: String -> Effect Unit
log s = \token ->
  let ignored = writeStdout getStdout s
  in writeStdout getStdout "\n"
```

`log` is the user-facing function. Its type uses `String`, `Effect`, and
`Unit`. The effect body calls the raw imports. `getStdout` is a handle
declared as `Int`. `writeStdout` takes that handle and a `String` and returns
`Unit`. Neither raw import is part of the public API. The module exports
`log` and `error` only. The call trace does not depend on that list.

Before CC, `ExternalBindings::from_core` records `writeStdout` as parameters
`Int` and `String` and result `Unit`, and records the WIT binding
`wasi:io/streams` / `[method]output-stream.blocking-write-and-flush`. CC and
MIR then see an `i32`, a GC string, and a call symbol. They do not see the
names `Int`, `String`, or `output-stream`.

P9 reads the side table. `Int` beside a handle kind pushes the `i32`. `String`
is expanded to a transient `(pointer, length)` by the canonical ABI lowering.
The WIT result is `result<_, stream-error>` with a unit success payload, so
`visible_return` is `Unit`: the lowerer passes a return pointer, traps on a
nonzero discriminant, and rebuilds `Unit`. The flattened parameters are the
handle plus `(pointer, length)`, which is `Resolve::wasm_signature` for that
function, so componentize links the core import. The instructions are the ones
in the worked example of
[canonical ABI and WIT](canonical-abi-and-wit.md). `log` itself is not a
foreign import; after the two `writeStdout` calls return `Unit`, the effect
function returns `Unit`.

### A `Maybe` parameter, specified and not added to the library

A WIT function `send : func(m: option<string>)` has no compiler source type.
The standard library may still expose it. This module is a specification
example; it is not in the library. The primitive import `Int -> String -> Unit`
validates and lowers. A declaration written as `Maybe String -> Unit` still
has no source signature and is rejected.

```purescript
module WASI.Example (send) where

import Prelude

data Maybe a = Nothing | Just a

foreign import "example:api#send" rawSend :: Int -> String -> Unit

send :: Maybe String -> Effect Unit
send message = \token ->
  case message of
    Nothing -> rawSend 0 ""
    Just text -> rawSend 1 text
```

`Maybe` is an ordinary ADT. The type checker never asks the ABI about it.
`send` cases on it and passes primitives. `Nothing` passes discriminant `0`
and a dummy empty string, which a correct host does not read. `Just text`
passes `1` and the string. `rawSend` is not exported.

Validation does not look at `Maybe`. It flattens `Int` to one `i32` and
`String` to `(pointer, length)` and requires that sequence to equal
`Resolve::wasm_signature` of `example:api#send`, whose flat parameters are the
`option` discriminant and the string's pointer and length. The result is
`Unit`. Componentize then links the same flat core signature.

The same function written as `foreign import rawSend :: Maybe String -> Unit`
is rejected. Projecting `Maybe String` produces no `SourceType`, so the
binding has no signature. The lowerer does not match `Nothing` and `Just`.

If `send` instead returned `option<string>`, the canonical result would be a
discriminant plus `(pointer, length)` in the return area. `visible_return`
rejects it. There is no `foreign import read :: Int -> String` that receives
both words, and no tuple return that packages them. A wrapper cannot `case`
on a success string it never receives. The function stays unexposed.

## Boundaries and interfaces

- **From the frontend:** a typed foreign import and its binding string. This
  topic does not change how `Maybe` or `Either` are type-checked. A `Maybe`,
  `Either`, tuple, or other new aggregate fails ABI validation because it has
  no `SourceType`. An existing nullary enum, closed record, or flags record
  still validates on the current lowering path.
- **To CC:** `ExternalBindings` already captured `SourceType`. CC does not gain
  WIT types, HIR types, or a `SourceType` of its own.
- **To the canonical ABI:** the primitive declaration and the resolved WIT
  function. That topic owns flattening, the return pointer, and the
  unit-success trap. This topic owns the rule that the primitive flat list
  must equal `Resolve::wasm_signature`, including when the WIT-level parameter
  is an aggregate with no compiler source type.
- **To the platform library:** which services exist, the component world, and
  how the library is embedded or loaded. This topic owns only the split inside
  those modules between unexported imports and exported wrappers.
- **To buffer lifetime:** `String` expansion allocates a transient buffer. The
  lifetime rules stay in the linear-memory and buffer topics.

## Open questions and future work

- **Which services stay unexposed.** Filesystem metadata and any other WIT
  function whose result is a discriminant plus a payload stays out of the
  library until that whole result is one primitive. This topic does not choose
  a future representation for that case.

## Implementation notes

`WASI.Console` exports `log` and `error`. `WASI.Clock` exports `now`.
`WASI.Random` exports `randomBytes` and `randomU64`. `WASI.Exit` exports
`exitWithCode`. Raw imports (`writeStdout`, `getStdout`, `getStderr`,
`monotonicNow`, `getRandomBytes`, `getRandomU64`, `exitWithCodeRaw`) stay in
their modules and are not exported. `wasi:cli/exit.exit` is not wrapped; the
[capability matrix](wasi-platform-library.md#capability-matrix) records why.
A primitive import
of an `option` parameter is accepted when the declared primitives flatten to
the canonical parameter list; `option<string>` is `Int -> String -> Unit`.
`Maybe` and `Either` are ordinary data types in `Data.Maybe` and `Data.Either`,
not in `Prelude` and not compiler builtins. Nullary enum, closed record, and
flags-record foreign imports still lower; new library code should not use that
path. No `SourceType::Option`, `SourceType::Result`, or `SourceType::Tuple`
exists.

## References

- [DEC-11 — Primitive foreign imports and standard-library wrappers](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md).
- [DEC-06 — Runtime Interface via WASI and the Component Model](../../../decision/DEC-06-runtime-interface-via-wit.md).
- [Canonical ABI and WIT](canonical-abi-and-wit.md), including
  `Resolve::wasm_signature` and the unit-success `result`.
- [WASI platform library](wasi-platform-library.md).
- [Linear memory and the canonical ABI boundary](linear-memory-and-canonical-abi-boundary.md).
- WebAssembly Component Model Canonical ABI: flattening of `option`, `result`,
  `string`, and tuples.
- Issue #56, whose request to grow compiler source types for `Maybe`, `Either`,
  and tuples this contract replaces. The user-facing types remain library types.
