# F-02 — Build Portable Program Artifacts

**Status:** In progress
**Design:** [wasm encoding — Wasm Lowering](../design/backend/wasm/encoding-and-structuring.md)

## User need

Users want to build a supported PureScript program once and run it in a
compatible portable environment.

## User-visible behavior

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

The selected command entry is a zero-argument integer `main`. It may use the
provided `runEffect` operation to execute effect values; other source
declarations cannot invoke the runner. An `Effect` value is opaque to source
code, and constructing it does not execute it.

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
function parameters, including nested constructor patterns,
a restricted parameterized ADT slice with erased scalar
fields, concrete scalar array literals and indexing, closed concrete records,
field reads, record updates, and closed concrete record patterns with variable,
wildcard, and nested constructor or record field bindings, function values including scalar-capturing closures,
higher-order calls, and the
implemented effect-based WASI console and clock libraries plus random imports.
The selected entry must be a zero-argument integer `main` function. Generic direct
calls and annotated rank-N values are lowered, including quantified parameters,
record and constructor fields, captures, and returned functions. Each use can
instantiate a quantified value independently; nested constraints are supplied
through the corresponding class instances. Generic arrays
and records cross the supported polymorphic boundaries with their contents
preserved. Open rows and unsupported WIT shapes receive source-oriented
diagnostics.

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
`derive newtype instance` reuses an instance for the wrapped type, with checked
conversions at method boundaries. Derived instances participate in the same
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

- A supported source program produces a validated core Wasm module at the
  requested output path.
- The compiler prints WAT or writes it at the requested output path.
- The eventual WASI artifact runs in a compatible WASI runtime and produces
  the program's expected observable result.
- The artifact runs in a compatible WASI runtime and produces the program's
  expected observable result.
- Unsupported constructs fail with a source-oriented diagnostic.
- Each added platform service has documented behavior and executable tests.

## Initial scope

- Language and platform support grows in documented increments, with the
  official test suite as the compatibility baseline rather than promising
  compatibility with every existing PureScript program at once.
- Standard platform services are provided through PureScript-facing WASI
  libraries and a stable runtime interface.
- Optimizations preserve observable program behavior.
- Node.js and JavaScript FFI compatibility, sockets, HTTP, and asynchronous
  WASI services are later or separate work.
