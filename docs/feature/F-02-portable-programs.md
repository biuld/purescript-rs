# F-02 — Build Portable Program Artifacts

**Status:** In progress
**Design:** [wasm encoding — Wasm Lowering](../design/backend/wasm/encoding-and-structuring.md),
[linking and runtime (Draft)](../design/backend/wasm/linking-and-runtime.md),
[algebraic effects and handlers (Draft proposal)](../design/backend/fp/algebraic-effects-and-handlers.md),
[native Effect/ST handlers (Draft proposal)](../design/backend/fp/native-effect-and-st-handlers.md),
[exceptions as effects](../design/backend/fp/exceptions-as-effects.md)

## User need

Users want to build a supported PureScript program once and run it in a
compatible portable environment.

## User-visible behavior

A proposed library-level algebraic effects API would let programs describe
operations and supply different handlers for production and testing, using
existing PureScript syntax and types. The representation for the generic handler
mechanism is recorded as a decision ([DEC-19](../decision/DEC-19-compiler-generated-resumable-frames.md));
a separate draft proposes implementing the official Effect/ST APIs over library
native handlers while preserving their public contracts. The low-level control surface now supports closed and open interpretation,
persistent multi-shot continuations, and explicit one-shot owners. Calling or
interpreting an expired continuation, reusing a spent owner, or resuming a
cancelled owner traps. Full library APIs, host interpretation and official-suite
acceptance remain incomplete; this does not restore the former `Effect Unit`
entry capability.

The typed-failure library provides abortive requests and recovery. Recovery
starts after the failed computation's scope has completed, leaving its retained
continuations expired. Handler nesting determines whether pure state results are
retained; shared mutation and external output are not automatically rolled back.
This library is separate from the official native exception APIs and does not
claim that arbitrary runtime failures can be caught as typed application errors.

The intended workflow is:

```sh
psrs build src/Helper.purs src/Main.purs -o main.wasm
<compatible WASI runtime> main.wasm
```

The first platform target is WASI 0.2 on the Component Model, whose synchronous
interfaces match the runtime; WASI 0.3's async model is later work. Programs
use the project's PureScript-facing WASI libraries for platform services.
Existing Node.js APIs and JavaScript FFI modules are not supported compatibility
targets. Programs that use unsupported syntax, types, or platform services
receive source-oriented diagnostics rather than a malformed artifact.

An already built application component can select pinned guest implementations
for its interface imports:

```sh
psrs link main.wasm --manifest providers.json -o linked.wasm --report link.json
```

The manifest selects each guest by interface name and artifact digest. Paths are
relative to the manifest. The linker follows the selected guests' dependencies,
rejects incompatible exports or missing providers, and reports the exact host
interfaces left unresolved. An explicitly selected guest does not fall back to
a host implementation. The initial interface binding is synchronous and covers
one whole interface, preserving resource lifetime and provider-owned memory.
Source `build` can explicitly select the same providers with `--manifest` and
emit a joined source/component report with `--report`. The compiler pins its
produced root bytes; externally supplied application components require a root
pin in the manifest. Guest executable initialization is rejected until it has
an explicit checked effect contract.

See [Explicit Component Linking](../workflow/component-linking.md) for the
manifest format and supported boundary.

The compiler selects `Main.main` when that declaration exists; otherwise it
requires exactly one top-level declaration named `main`. A direct entry returns
`Int`, which determines the process exit code. A package may explicitly configure
an ordinary runner to adapt its source entry to that integer command boundary.
The runner's implementation determines execution behavior; the compiler does
not discover an Effect type or synthesize its operations.

The former `Effect Unit` entry adaptation and lexical `runEffect` restriction
were removed on 2026-10-11. Their replacement remains a draft proposal. Earlier platform
coverage below does not establish current execution of package sources that
depend on the withdrawn Effect or State mechanisms.

The compiler emits a WASI 0.2 Component Model artifact. The legacy WASI
Preview 1 module ABI is not part of the supported output contract.

The executable baseline is a documented, pinned WebAssembly runtime
(`wasmtime`). An artifact requires only the features enabled by the selected
backend capability profile. The stable profile uses Wasm GC/reference types,
typed function references, and synchronous WASI 0.2 Component Model support;
optional proposals such as SIMD, tail calls, exceptions, threads, memory64,
and WASI 0.3 are separate target tracks. Portability is therefore defined by
the selected profile rather than by every feature a runtime happens to
support.

Compatibility is measured against the official PureScript test suite, layer by
layer, as decided in [DEC-04](../decision/DEC-04-official-test-suite-roadmap.md):
layout, parse, name resolution, kinds, types, classes, then runtime. The
frontend and backend feature matrices define the scope and current support
state; the suite is the acceptance oracle. Coverage is per-file agreement with
the official compiler on accept/reject and diagnostic code. Suite files that
require JavaScript or Node.js FFI are excluded from the target and count as
neither coverage nor gaps.

The language target includes rank-N polymorphism: annotated function
parameters, record fields, constructor fields, and returned values may carry
their own `forall` quantifiers. A caller can use one such value at different
types, while a function specialized to one concrete type cannot satisfy a
universally quantified parameter. Higher-rank programs generally require
annotations, following official PureScript's checking rules.

The current compiler can build a restricted program, including linked source
modules, to a validated WASI component and print its WAT form:

```sh
psrs build src/Helper.purs src/Main.purs -o main.wasm
psrs wat src/Helper.purs src/Main.purs -o main.wat
```

The initial slice supports direct top-level functions, integer and boolean
values, integer arithmetic and comparisons, scalar `let`, `if`, nullary enum
tags, non-parameterized data constructors with scalar or nested aggregate
fields, single-field `newtype` values, constructor patterns in `case` and
function parameters, including nested constructor patterns, a restricted
parameterized ADT slice with erased scalar fields, concrete scalar array
literals and indexing, closed concrete records, field reads, record updates,
and closed concrete record patterns with variable, wildcard, and nested
constructor or record field bindings, function values including
scalar-capturing closures, higher-order calls, and the implemented effect-based
WASI console and clock libraries plus random imports.
The command boundary returns `Int`, directly or through an explicitly configured
ordinary runner. Generic direct calls and annotated rank-N values are lowered,
including quantified parameters, record and constructor fields, captures, and
returned functions. Each use can
instantiate a quantified value independently; nested constraints are supplied
through the corresponding class instances. Generic arrays
and records cross the supported polymorphic boundaries with their contents
preserved. Open record rows support field access, record patterns, and immutable
updates while preserving extra fields. Unsupported WIT shapes receive
source-oriented diagnostics.

A non-exhaustive pattern match requires `Partial`. An inferred declaration may
retain that constraint; an explicit signature must provide it or checking
reports a missing case at the original source range. `unsafePartial` permits
calling the partial value, and an unmatched input traps if that path executes.

Role annotations describe whether a type parameter is nominal,
representational, or phantom. Inferred roles follow the type's fields, including
referenced declarations from other modules. An annotation may restrict an
inferred role but cannot weaken it; a foreign type's declared roles are an
explicit interface promise.

The `coerce` operation requires a compiler-proven `Coercible` relation. It can
convert visible newtypes and lift permitted conversions through arrays,
functions, and data parameters according to their roles. Unwrapping a newtype
requires its constructor to be in scope. Users cannot define `Coercible`
instances to authorize unrelated conversions. Unsupported coercion obligations
receive a source diagnostic.

`derive instance` generates implementations for the supported standard classes
from a locally declared type's constructors and fields. Structural `Eq` and
`Ord`, and the covered `Functor` and `Bifunctor` mappings, are supported.
`derive newtype instance` reuses an instance for the wrapped type. Method
boundaries are representation casts authorized by the newtype declaration and
the wrapped instance, not by ordinary `Coercible`. Derived instances participate in the same
constraint checks and module imports as explicitly written instances. Other
standard deriving rules and additional field shapes remain incomplete.

`arrayUpdate` is a pure operation: it returns an updated array without changing
the input array or any aliases of it. Repeated updates from the same input are
therefore independent. This behavior uses the GC language heap; linear memory is
reserved for the canonical ABI boundary.

Scalar operators cover the full `Int`, `Number`, `Boolean`, and `Char` sets the
standard library exposes, including bitwise and shift operations, conversions,
and floor `Int` division and divisor-sign modulus. A data type with fields constructs and
pattern matches on every target profile the compiler supports. Platform services
grow as PureScript-facing WASI libraries: console, clock, and random are
implemented, and arguments, environment, and files follow as their WIT forms are
supported.

Unary minus applies the ordinary in-scope `negate` function to its operand, so
local bindings and imports follow the same name resolution rules as other
function calls.

## Acceptance criteria

- A supported source program produces a validated WASI component at the
  requested output path.
- The compiler prints WAT or writes it at the requested output path.
- The WASI artifact runs in a compatible runtime and preserves observable
  behavior. An `Int` entry preserves its exit code; a configured ordinary runner
  preserves its declared behavior, and guest traps propagate as failures.
- Unsupported constructs fail with a source-oriented diagnostic.
- Each added platform service has documented behavior and executable tests.
- Explicit guest linking verifies artifact pins and interface compatibility,
  preserves canonical ownership, and reports the permitted residual host imports.

## Initial scope

- Language and platform support grows in documented increments, with the
  official test suite as the compatibility baseline rather than promising
  compatibility with every existing PureScript program at once.
- Standard platform services are provided through PureScript-facing WASI
  libraries and a stable runtime interface.
- Optimizations preserve observable program behavior.
- Node.js and JavaScript FFI compatibility, sockets, HTTP, and asynchronous
  WASI services are later or separate work.

Synchronous `Effect.Exception` preserves the official public API with a
message-only Error: name is Error, stack is Nothing, and Show uses the stackless
error-string fallback. Throw/catch actions execute when interpreted. Source
catches recover source errors only; stack exhaustion and engine traps propagate.
Source stack traces and asynchronous exceptions remain separate work. See the
[exception contract](../design/backend/fp/effect-exception.md).
