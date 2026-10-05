# WASI Platform Library

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Draft  
**Prerequisites:** the Component Model (core modules versus components, WIT worlds, the Canonical ABI), the WASI 0.2 `wasi:cli/command` world and its `run` export, and the project's effect representation. Read the [capability profile](capability-profile.md), [canonical ABI and WIT](canonical-abi-and-wit.md), and [effects](../fp/effects.md) first.  
**Summary:** A build produces a WASI 0.2 component that implements the `wasi:cli/command` world and exports `wasi:cli/run@0.2.12`. The PureScript-facing platform library is ordinary source code over WASI imports, so a program imports only the capabilities it uses. This document owns component packaging and the enabled WASI services.

## Scope

This document owns the artifact shape, the application world, componentization
with `wit-component`, the enabled WASI services, and validation and execution of
the component. It does not own canonical ABI adaptation and call lowering
([canonical ABI and WIT](canonical-abi-and-wit.md)), the byte boundary and
allocator ([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)),
the thin encoding ([Wasm encoding](encoding-and-structuring.md)), which
capability families are permitted ([capability profile](capability-profile.md)),
or the split between unexported primitive foreign imports and user-facing
wrappers ([primitive FFI and the standard library](primitive-ffi-and-stdlib.md)).

## Background

**Modules and components.** A core module is the WebAssembly unit with functions,
memory, and imports named by `(module, field)` strings. A component is the
Component Model unit; it imports and exports named WIT interfaces and is
instantiated with those interfaces supplied by the host. A core module is
*componentized* by describing the world it implements, after which the toolchain
lifts its exports into component imports and exports.

**The command world.** WASI 0.2 defines an application entry shape, the
`wasi:cli/command` world, whose only export is `wasi:cli/run@0.2.12`. The
exported `run` function returns a `result`; a runtime such as `wasmtime run`
loads the component, calls `run`, and treats a nonzero exit as failure. Process
termination with a specific code is requested through `wasi:cli/exit`.

**Capability-based imports.** WASI is a set of capability interfaces. A component
declares in its world which interfaces it may import, and only those are
available at instantiation. Importing fewer interfaces makes an artifact usable
under a more constrained host. This is why imports are capabilities rather than
a fixed runtime ABI ([DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md)).

**WASI 0.2 versus 0.3.** WASI 0.2 is synchronous and is the stable project
target: an `output-stream` is written with blocking calls. WASI 0.3 adds native
async (`async func`, `stream<T>`, `future<T>`) and would require async plumbing
even to write to standard output, so it is a separate target track
([capability profile](capability-profile.md)).

## Model

The artifact is a WASI 0.2 component with a `wasi:cli/command` entry: it exports
`wasi:cli/run@0.2.12`. It is built from a core module that:

- exports `wasi:cli/run@0.2.12#run` (the constant `RUN_CORE_EXPORT`) as the
  command entry;
- exports its linear memory as `memory` for the Canonical ABI; and
- imports only the WASI interfaces the lowered program references.

The application world is `psrs:app`, package `psrs:app`, world `command`:

```text
world command {
    import wasi:io/error@0.2.12;
    import wasi:io/poll@0.2.12;
    import wasi:io/streams@0.2.12;
    import wasi:clocks/monotonic-clock@0.2.12;
    import wasi:clocks/wall-clock@0.2.12;
    import wasi:random/random@0.2.12;
    import wasi:random/insecure@0.2.12;
    import wasi:random/insecure-seed@0.2.12;
    import wasi:cli/environment@0.2.12;
    import wasi:cli/exit@0.2.12;
    import wasi:cli/stdin@0.2.12;
    import wasi:cli/stdout@0.2.12;
    import wasi:cli/stderr@0.2.12;
    import wasi:filesystem/types@0.2.12;
    import wasi:filesystem/preopens@0.2.12;
    import wasi:sockets/network@0.2.12;
    import wasi:sockets/instance-network@0.2.12;
    import wasi:sockets/udp@0.2.12;
    import wasi:sockets/udp-create-socket@0.2.12;
    import wasi:sockets/tcp@0.2.12;
    import wasi:sockets/tcp-create-socket@0.2.12;
    export wasi:cli/run@0.2.12;
}
```

`wasi:io/streams` transitively pulls in the support interfaces
`wasi:io/error@0.2.12` and `wasi:io/poll@0.2.12`. The `wasi:cli` terminal and
`sockets/ip-name-lookup` interfaces are accepted but not wrapped by the platform
library. `component.rs` records the
full resolved set in `COMPONENT_INTERFACES`, next to the world, so ABI discovery
and component encoding share one contract; a test asserts the resolved world
matches that list exactly and imports only named interfaces. The vendored WASI
0.2.12 WIT is loaded in dependency order by `load_vendored_wasi`, and
`command_world` resolves `psrs:app` against it.

### Enabled services

The names in **Source-facing operation** are the user-facing wrappers, not the
types of the foreign imports. They are ordinary PureScript. The imports
underneath use primitives (`Int`, `String`, `Unit`), `Resource a` for a handle,
and the mapped aggregate forms (`Maybe`, `Either`, records, and library data
types, [DEC-13](../../../decision/DEC-13-wit-to-source-type-mapping.md)); they
must not be exported.

| Service | WIT interface | Source-facing operation |
| --- | --- | --- |
| Resource handles | every resource interface | `WASI.Resource.Resource a`, a newtype over `Int` |
| Streams, poll, error | `wasi:io/streams`, `wasi:io/poll`, `wasi:io/error`, `wasi:cli/{stdin,stdout,stderr}` | `WASI.IO`: `getStdin`/`getStdout`/`getStderr`, `read`/`blockingRead`/`skip`/`blockingSkip`, `write`/`blockingWriteAndFlush`/`flush`, `subscribeInput`/`subscribeOutput`, `poll`/`ready`/`block`, `toDebugString`, and the `drop*` helpers |
| Console output | `wasi:cli/stdout`, `wasi:cli/stderr`, `wasi:io/streams` | `WASI.Console.log`, `WASI.Console.warn`, `WASI.Console.error :: String -> Effect Unit` |
| Filesystem | `wasi:filesystem/types`, `wasi:filesystem/preopens` | `WASI.FileSystem`: `preopens`, `openRead`/`openWrite`/`openAppend`, `readFile`/`writeFile`/`readString`/`writeString`, `stat`/`statAt`/`getType`, `readDirectory`, `setTimes`, `withDescriptor` |
| Network | `wasi:sockets/network`, `wasi:sockets/{instance-network,tcp,udp,tcp-create-socket,udp-create-socket}` | `WASI.Network`: `instanceNetwork`, `createTcpSocket`/`createUdpSocket`, the `tcp*`/`udp*` operations, and the `drop*` helpers |
| Monotonic and wall clock | `wasi:clocks/monotonic-clock`, `wasi:clocks/wall-clock` | `WASI.Clock`: `now`, `wallNow`, `wallResolution`, `subscribeInstant`, `subscribeDuration` |
| Random bytes | `wasi:random/random`, `wasi:random/insecure`, `wasi:random/insecure-seed` | `WASI.Random`: `randomBytes`, `randomU64`, `insecureBytes`, `insecureU64`, `insecureSeed` |
| Process exit, arguments, environment | `wasi:cli/exit`, `wasi:cli/environment` | `WASI.Process`: `exitWithCode :: Int -> Effect Unit`, `arguments :: Effect (Array String)`, `environment :: Effect (Array { _1 :: String, _2 :: String })`. `main`'s integer code is still the synthesized `run` entry's call to `exit-with-code`, not this wrapper |
| Umbrella | all of the above | `WASI` re-exports the curated API |

### Corpus-facing wrappers

The `passing` suite imports names the WASI layer does not use. These modules are
thin wrappers over the platform layer, per
[DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md): the raw
import stays in `WASI.*` or in the compiler-owned `Prelude` externals, and the
wrapper owns the corpus-facing name.

| Module | Corpus name | Over | Owner |
| --- | --- | --- | --- |
| `Prelude` | `Effect`, `pure`, `bind`, `discard`, `map`, `apply`, `runEffect`, `trap`, and the re-exported `class Semigroup`, `append`, `<>`, `const`, `flip`, `$`, `#` | the `psrs:effect` interface the compiler synthesizes bodies for, and `Data.Function` / `Data.Semigroup` | FE-02, FE-05, FE-09, FE-14, BE-02, BE-21 |
| `Data.Function` | `apply`, `applyFlipped`, `const`, `flip`, `on`, `$`, `#` | nothing: it is the definition site, and `$`/`#` are its fixity aliases | FE-05 |
| `Data.Semigroup` | `class Semigroup`, `append`, `<>` | nothing: it is the definition site for the class, and `append` for `String` is built from the compiler's `stringToBytes` / `arrayAppend` / `bytesToString`; `append` for `Array a` is `arrayAppend` | FE-14, BE-10 |
| `Data.Monoid` | `class Monoid`, `mempty` | `Data.Semigroup`; `String`, `Unit`, and `Array a` identities | FE-14 |
| `Data.Foldable` | `class Foldable`, `foldr`, `foldl`, `foldMap` | `Data.Monoid` and the array index primitives; `Array`, `Maybe`, and `Either a` instances | FE-14 |
| `Data.Tuple` | `Tuple`, `fst`, `snd`, `curry`, `uncurry`, `swap` | the closed record `{ _1 :: a, _2 :: b }` that FE-06 already lowers a tuple to; `type Tuple a b` is that record, not an algebraic `data Tuple a b = Tuple a b` | FE-06 |
| `Effect` | re-exports the `Prelude` surface above | `Prelude` | FE-02 |
| `Effect.Console` | `log`, `warn`, `error`, `logShow` | `WASI.Console`, and the library `Data.Show.show` for `logShow` | BE-21 |
| `Test.Assert` | `assert`, `assert'`, `assertTrue`, `assertFalse` | `Effect.Console.error` and `Prelude.trap` | BE-21, BE-27 |

### Capability matrix

Each intended standard-library feature has one owner. The library is now the
consolidated capability layout: `WASI.Resource`, `WASI.IO`, `WASI.Console`,
`WASI.FileSystem`, `WASI.Network`, `WASI.Clock`, `WASI.Random`, and
`WASI.Process`, with the `WASI` umbrella. It does not move the WIT root or add
`stdlib.toml`. The primitive-import contract is
[primitive FFI and the standard library](primitive-ffi-and-stdlib.md).

| Feature | Owner | State |
| --- | --- | --- |
| On-disk `stdlib/lib` and the trusted prefix | WASI-10 | Done. The driver reads `stdlib/lib/trusted`. |
| Exported wrappers | This library, [DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md) | Every service wrapper, plus the `WASI` umbrella. Raw imports stay unexported. |
| `wasi:cli/exit.exit` (`status: result`) | Not wrapped | One canonical `i32`, and still not a library wrapper. See below. |
| `Effect` as `foreign import data` | [Effects](../fp/effects.md) | Trusted library binding carries the resolved constructor and operation identities. Effect lowering turns applications into generic one-parameter closures, and import wrappers come only from plans formed from checked external schemes before `Effect` erasure. |
| `Maybe`, `Either`, records, and data types as wrappers | [DEC-13](../../../decision/DEC-13-wit-to-source-type-mapping.md) | Library types, not compiler types; every `result` is an `Either E O` with the error on `Left`. |
| **WASI-07 Arguments, environment, and filesystem** | #59 | Verified. `WASI.Process.arguments`/`environment` and `WASI.FileSystem` wrap `wasi:cli/environment` and `wasi:filesystem`; a file round-trip, a directory walk, and an environment read execute under Wasmtime. |
| **WASI-08 Sockets** | #59 | In progress. `WASI.Network` wraps the socket services and the wrapper surface lowers; no socket execution test yet. HTTP/TLS are excluded. |
| Resource `own` / `borrow` drop | [DEC-14](../../../decision/DEC-14-resource-handle-ownership.md) | Handles are `WASI.Resource.Resource a`; the library drops each extracted handle explicitly. |
| Type classes | #63 | Not this branch. |
| WIT root move and `stdlib.toml` | #66 | Not done. WIT stays in `crates/psrs-backend/wit/`. There is no `stdlib.toml`. |

`exit` is `func(status: result)`. The vendored `result` has no ok payload and
no err payload, so `Resolve::wasm_signature` flattens that parameter to one
`i32` discriminant rather than a discriminant plus a payload. It is still not
wrapped. The lowerer classifies a `result` parameter as unsupported, and it
accepts a primitive flattening only when the source arity differs from the WIT
arity. `Int -> Unit` has the same arity as that one WIT parameter, so the
declaration is rejected. The `i32` is also only a success/failure tag, which
[DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md) does not
export. Wrapping `exit` would need a compiler change this slice does not make.

Platform modules own their WIT bindings and expose effectful operations; the
portable `Prelude` does not import WASI. A program opts into a capability by
importing the platform module, and after linking, unreachable declarations are
pruned so an unused service adds no import.

### Invariants

- The core module exports exactly one command entry and one memory; the entry is
  zero-argument and returns `i32`.
- The component exports `wasi:cli/run@0.2.12` and imports only interfaces in the
  application world. An import outside the enabled set is rejected during ABI
  resolution.
- The core↔component bridge uses UTF-8 string encoding
  (`StringEncoding::UTF8`); the canonical ABI linearizes GC strings as UTF-8
  bytes ([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)).
- A built component validates as a component before it is written.

## Design

### Componentization

`componentize(core, resolve, world)` embeds component metadata describing the
world the core module implements, then encodes the component:

```text
componentize(core, resolve, world):
    bytes = core
    embed_component_metadata(bytes, resolve, world, StringEncoding::UTF8)
    ComponentEncoder::default()
        .module(bytes)?
        .validate(true)
        .encode()
```

`embed_component_metadata` records how the core module's `(module, field)`
imports and exports correspond to the world's WIT interfaces, and
`ComponentEncoder` lifts them. Only the interfaces the core module actually
references become component imports, so a program that does not use a service
does not import it. The build pipeline requires the Component Model and the
WASI 0.2 families to be enabled in the target profile
([capability profile](capability-profile.md)).

### Entry and exit

The driver selects one source declaration by resolved identity: `Main.main`
when present, otherwise the unique top-level declaration named `main`. The
selected declaration takes no arguments and returns `Int` or the trusted
`Effect Unit` type. An `Int` declaration keeps the existing zero-argument
integer command convention and preserves its result. For `Effect Unit`, P8
generates an ordinary Core adapter that evaluates the selected declaration,
runs the returned action once, and returns zero after normal completion. The
adapter becomes the Core entry before CC; no adapter is added around an `Int`
entry.

The `Effect Unit` wrapper propagates a guest trap instead of producing a
successful zero exit. P10's `run` entry calls the selected or generated
zero-argument integer function, passes its result to
`wasi:cli/exit.exit-with-code`, and returns `0`,
the canonical `ok` discriminant of the `run` result. `WASI.Process.exitWithCode`
is a separate effectful call to the same WIT function; it does not replace the
entry. A runtime that implements `exit-with-code` as process termination never
observes the trailing constant, which exists to give the entry its declared
`i32` result ([Wasm encoding](encoding-and-structuring.md)).

### The platform library

The platform library is source code under `stdlib/lib`, read from disk and
resolved, type-checked, and linked like any module. `stdlib/lib/trusted` fixes
the trusted prefix order (`Prelude`, `Data.Function`, `Data.Semigroup`,
`Data.Monoid`, `Data.Eq`, `Data.Ord`, `Data.Semiring`, `Data.Show`, `Effect`, `Effect.Console`, `Test.Assert`, `Data.Maybe`, `Data.Either`, `Data.Tuple`, `Data.Foldable`, `WASI.Resource`, `WASI.IO`, `WASI.Clock`, `WASI.Random`, `WASI.Console`, `WASI.Process`, `WASI.FileSystem`, `WASI.Network`, `WASI`). `Data.Function`
declares the application operators and their fixities; `Data.Semigroup`
declares the `Semigroup` class, its method, and the `<>` alias; `Data.Monoid` declares `Monoid` and `mempty` for `String`, `Unit`, and `Array a`; `Data.Foldable` declares `Foldable` with `foldr`, `foldl`, and `foldMap` for `Array`, `Maybe`, and `Either a`; `Prelude`
re-exports `$`, `#`, `const`, `flip`, and `Data.Semigroup`'s
`class Semigroup`, `append`, and `<>`, which is the official `Prelude`'s own
re-export list. `Data.Maybe` and `Data.Either` are ordinary library
types; they are not part of the trusted `Effect` representation. `Data.Tuple`
is the same kind of library declaration, placed after `Data.Either` because it
is not a `Prelude` re-export and it does not depend on `Maybe` or `Either`:
`Tuple a b` is the closed record `{ _1 :: a, _2 :: b }` FE-06 lowers `(a, b)`
to, so the module does not declare an algebraic constructor. `Effect` and
`Effect.Console` are the corpus-facing names for the effect interface and the
console; `Test.Assert` is the corpus's assertion surface and reports a failure
by writing a message and escaping through `Prelude.trap`. `WASI.Resource`
defines the `Resource a` newtype and
its bracket; `WASI.IO` defines the streams, poll, and error resource;
`WASI.Console` defines `log`, `warn`, and `error`; `WASI.Process` defines
`exitWithCode`, `arguments`, and `environment`; `WASI.Clock`, `WASI.Random`,
`WASI.FileSystem`, and `WASI.Network` define their service wrappers; and `WASI`
re-exports the curated API. Each WIT import is declared with a binding string
and lowered by the generic Canonical ABI adapter
([canonical ABI and WIT](canonical-abi-and-wit.md)); the compiler has no
per-service host function.

### Two library layers

Each enabled service is two layers
([primitive FFI and the standard library](primitive-ffi-and-stdlib.md),
[DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md)).
The raw `foreign import` is unexported and uses primitives, `Resource a` for a
handle, and the mapped aggregate forms. The names in the table above are the
exported wrappers. Those raw imports must not be exported; the
`stdlib_export_lists_do_not_expose_raw_foreign_imports` test checks every
standard-library module. Wrappers may use library types such as `Maybe`,
`Either`, records, or data types; they `case` on those types and pass values
whose flattening matches the WIT function. `arguments` passes the
`list<string>` result through as an `Array String`. This document does not move
that contract into the component world.

### Rejected alternatives

- **WASI Preview 1 (`wasi_snapshot_preview1`).** Rejected: it is the legacy
  module ABI and would bake a WASI version and an iovec layout into the backend
  ([DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md)).
- **A project-specific runtime ABI (`psrs:runtime`).** Rejected: WASI already
  plays that role and the standard library should be library code.
- **WASI 0.3 / async components in the stable profile.** Rejected for now: the
  source language has no async features and the runtime is synchronous.
- **A syntax-directed list of host functions in the compiler.** Rejected:
  services are ordinary source over WIT imports.

## Algorithms

### Build and componentize

```text
compile(program, target):
    cc            = lower Typed Core to CC with ExternalBindings
    (mir, wasi)   = lower CC to MIR, resolving and validating WIT bindings
    wasm          = structure MIR, synthesize the run entry, allocate indices
    core          = encode_module(wasm)
    (resolve, w)  = command_world()
    component     = componentize(core, resolve, w)
    validate_all(component) with features from `target`
    wat           = wasmprinter::print_bytes(component)
    return Artifact { wasm: component, wat }
```

### Build, validate, run

```text
psrs build <files> [-o out.wasm]
    compile each unit, link, compile to a component, validate, write out.wasm

wasmtime run out.wasm
    instantiate the component with the host's WASI interfaces
    call wasi:cli/run@0.2.12
    process exit code comes from wasi:cli/exit.exit-with-code
```

`psrs wat <files> [-o out.wat]` prints or writes the component's WAT form, and
`psrs dump <core|cc|mir> <file.purs>` prints an intermediate representation for
debugging.

### Edge cases

- A program that never reaches `main`'s result still exits through
  `exit-with-code`; the entry always calls `exit` on the CLI target.
- A program that imports no WASI interface still has the `run` export and the
  memory export; it simply has no imports.
- A module that reaches a disabled service fails at ABI resolution with a
  source-associated diagnostic, not with a missing-import runtime error.
- Running without `wasmtime` installed skips execution tests locally but fails
  in the required CI workflow when `PSRS_REQUIRE_WASMTIME=1` is set.

## Code map

Component packaging and the platform library must be organized as follows. The
tree owns the artifact shape and the enabled service set; the effect
representation the imports ride on stays in
[effects](../fp/effects.md).

```text
crates/psrs-backend/
  wit/psrs-app.wit
  wit/deps/
  src/
    component.rs
    abi.rs
    abi/wasi.rs
    wasm/lower/mod.rs
    lib.rs
crates/psrs-cli/src/main.rs
crates/psrs-driver/src/wasi.rs
```

Responsibilities and required entry points:

- `component.rs` owns the application world and componentization. It must define
  the `psrs:app` `command` world, the resolved interface set
  `COMPONENT_INTERFACES`, the command entry name `RUN_CORE_EXPORT`, and the two
  required entry points:
  - `fn command_world() -> (Resolve, WorldId)`: load the vendored WASI 0.2.12
    WIT in dependency order and resolve `psrs:app` against it.
  - `fn componentize(core: &[u8], resolve: &Resolve, world: WorldId) -> Result<Vec<u8>>`:
    embed component metadata with `StringEncoding::UTF8`, lift the core module's
    imports and exports, and validate the encoded component. The enabled service
    set is the `psrs-app` `command` world: `wasi:io/error`, `wasi:io/poll`,
    `wasi:io/streams`, `wasi:clocks/monotonic-clock`, `wasi:clocks/wall-clock`,
    `wasi:random/random`, `wasi:random/insecure`, `wasi:random/insecure-seed`,
    `wasi:cli/environment`, `wasi:cli/exit`, `wasi:cli/stdin`,
    `wasi:cli/stdout`, `wasi:cli/stderr`, `wasi:filesystem/types`,
    `wasi:filesystem/preopens`, `wasi:sockets/network`,
    `wasi:sockets/instance-network`, `wasi:sockets/udp`,
    `wasi:sockets/udp-create-socket`, `wasi:sockets/tcp`, and
    `wasi:sockets/tcp-create-socket`; ABI resolution must reject an import
    outside that world.
- `abi.rs` and `abi/wasi.rs` own the service interface names and binding lookup.
  `abi/wasi.rs` must map each enabled service to its WIT interface and
  source-facing operation — the exported wrapper, not the foreign-import type —
  and expose the lookup MIR binding resolution uses. No module may hard-code a
  per-service host function.
- `wasm/lower/mod.rs` must synthesize the `run` entry that calls `main`, passes
  the result to `wasi:cli/exit.exit-with-code`, and returns `0`.
- `lib.rs` must own the build pipeline: lower to CC, lower to MIR, structure,
  encode, call `command_world` and `componentize`, validate with the target's
  features, and return the component and its WAT form.
- The WASI library must be ordinary PureScript source resolved, type-checked,
  and linked like any other module. The driver loads it from `stdlib/lib`
  rather than embedding it, and defines `log`, `error`, `now`, `randomBytes`,
  `randomU64`, `exitWithCode`, and `arguments` over WIT imports; the portable
  `Prelude` must
  not import WASI.
- `psrs-cli/src/main.rs` must expose `psrs build`, `psrs wat`, and `psrs dump`.
- `wit/psrs-app.wit` and `wit/deps/` own the vendored WASI 0.2.12 WIT sources.

## Invariants and verification

The component test suite checks that the resolved application world imports
exactly `COMPONENT_INTERFACES` and only named interfaces, that a minimal core
module componentizes and validates, and, when `wasmtime` is available, that
`wasmtime run` executes the component. The build pipeline validates the encoded
component with the profile's features before returning it
([capability profile](capability-profile.md)). Observable behavior is checked by
execution tests that run the component under the pinned runtime and compare
standard output and the process exit code; structural validation alone is not
sufficient ([IR boundaries](../00-ir-boundaries.md)).

## Worked example

Take `main = log "hello"` with the inferred type `Effect Unit`. The platform
library defines `log` over the WIT imports `wasi:cli/stdout#get-stdout` and
`wasi:io/streams#[method]output-stream.blocking-write-and-flush`. Linking keeps
those imports because `main` reaches them, and prunes them otherwise. Lowering
produces the Canonical ABI call shown in
[canonical ABI and WIT](canonical-abi-and-wit.md), and the string literal lives
in a data segment ([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)).
The generated command wrapper runs the selected action once and returns zero.
`componentize` lifts the core module: the component imports
`wasi:cli/stdout@0.2.12` and `wasi:io/streams@0.2.12` (plus their support
interfaces) and exports `wasi:cli/run@0.2.12`. `wasmtime run hello.wasm` calls
`run`, which writes `hello\n` and exits successfully.

## Boundaries and interfaces

- **Input:** the core module bytes from P11, the vendored WASI 0.2.12 WIT, the
  `psrs:app` world, and the target profile.
- **Output:** a validated component artifact and its WAT form.
- **To the ABI layer:** the world and the interface set it may import; a service
  outside the world is rejected before MIR emits a call.
- **To the driver:** `command_world` and `componentize`; the CLI writes the
  artifact and the WAT form.

## Open questions and future work

- **More services.** Arguments, environment, and filesystem, then sockets, HTTP,
  and TLS, each behind its own capability flag and source library.
- **WASI 0.3 / async components**, once the language has async features and the
  runtime target is revised.
- **Reclamation**, so returned lists and resources do not leak
  ([canonical ABI and WIT](canonical-abi-and-wit.md)).

## Implementation notes

Console (stdout and stderr), streams/poll/error, filesystem, sockets, monotonic
and wall clocks, random, the environment arguments and variables, and the
synthesized command exit are implemented. `exitWithCode` lowers and stays
inside its effect closure until `runEffect`; there is no execution test that
calls it, because that terminates the process. `wasi:cli/exit.exit` is not
wrapped. `WASI.Process.arguments` and `WASI.Process.environment` wrap
`get-arguments` and `get-environment`; `WASI.FileSystem` wraps `wasi:filesystem`
with execution tests. `WASI.Network` wraps the socket services and lowers; it
has no execution test, and HTTP/TLS are not implemented, so their capability
flags stay disabled in the default profile. The standard library is read from
`stdlib/lib` at runtime (`stdlib/lib/trusted` lists `Prelude`, `Data.Function`,
`Data.Semigroup`, `Data.Monoid`, `Data.Eq`, `Data.Ord`, `Data.Semiring`, `Data.Show`, `Effect`, `Effect.Console`, `Test.Assert`, `Data.Maybe`, `Data.Either`, `Data.Tuple`, `Data.Foldable`, `WASI.Resource`, `WASI.IO`, `WASI.Clock`, `WASI.Random`, `WASI.Console`, `WASI.Process`, `WASI.FileSystem`, `WASI.Network`, and `WASI` in
trusted-prefix order). The driver discovers user modules from the entry files'
directories (`psrs_driver::load_program_files`): it indexes sibling `.purs`
files by module name and follows the `import` graph, never searching names the
on-disk library provides. Resolution, duplicate-module, and cycle checks remain
in P3.

## References

- WebAssembly Component Model: components, worlds, lifting, and `wit-component`.
- WASI 0.2 `wasi:cli/command` world and the `wasi:cli/run` export.
- `wit-component` `ComponentEncoder`, `embed_component_metadata`,
  `StringEncoding::UTF8`.
- [DEC-06 — Runtime Interface via WASI and the Component Model](../../../decision/DEC-06-runtime-interface-via-wit.md),
  [DEC-05 — Target wasmtime's WebAssembly Feature Set](../../../decision/DEC-05-wasmtime-feature-set.md),
  [DEC-11 — Primitive foreign imports and standard-library wrappers](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md).
- [Primitive FFI and the standard library](primitive-ffi-and-stdlib.md).
- [capability profile](capability-profile.md),
  [canonical ABI and WIT](canonical-abi-and-wit.md),
  [linear memory boundary](linear-memory-and-canonical-abi-boundary.md),
  [effects](../fp/effects.md).
