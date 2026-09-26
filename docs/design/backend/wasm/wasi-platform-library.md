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
the thin encoding ([Wasm encoding](encoding-and-structuring.md)), or which
capability families are permitted ([capability profile](capability-profile.md)).

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
    import wasi:cli/stdout@0.2.12;
    import wasi:cli/stderr@0.2.12;
    import wasi:io/streams@0.2.12;
    import wasi:cli/exit@0.2.12;
    import wasi:clocks/monotonic-clock@0.2.12;
    import wasi:random/random@0.2.12;
    export wasi:cli/run@0.2.12;
}
```

`wasi:io/streams` transitively pulls in the support interfaces
`wasi:io/error@0.2.12` and `wasi:io/poll@0.2.12`. `component.rs` records the
full resolved set in `COMPONENT_INTERFACES`, next to the world, so ABI discovery
and component encoding share one contract; a test asserts the resolved world
matches that list exactly and imports only named interfaces. The vendored WASI
0.2.12 WIT is loaded in dependency order by `load_vendored_wasi`, and
`command_world` resolves `psrs:app` against it.

### Enabled services

| Service | WIT interface | Source-facing operation |
| --- | --- | --- |
| Console output | `wasi:cli/stdout`, `wasi:io/streams` | `log :: String -> Effect Unit` |
| Console error | `wasi:cli/stderr`, `wasi:io/streams` | `error :: String -> Effect Unit` |
| Process exit | `wasi:cli/exit` | the synthesized `run` entry and `main`'s code |
| Monotonic clock | `wasi:clocks/monotonic-clock` | `now :: Effect Int` |
| Random bytes | `wasi:random/random` | the library's random operations |

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
  (`StringEncoding::UTF8`), matching the length-prefixed UTF-8 string
  representation ([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)).
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

The selected entry declaration is a zero-argument `Int` function. P10
synthesizes the `run` entry that calls `main`, passes the result to
`wasi:cli/exit.exit-with-code`, and returns `0`, the canonical `ok`
discriminant of the `run` result. A runtime that implements `exit-with-code` as
process termination never observes the trailing constant, which exists to give
the entry its declared `i32` result
([Wasm encoding](encoding-and-structuring.md)).

### The platform library

The platform library is source code, embedded in the driver because there is no
filesystem module loader yet, but resolved, type-checked, and linked like any
module. `WASI.Console` defines `log` over `writeStdout`/`getStdout`, and
`WASI.Clock` defines `now` over the monotonic clock. Each WIT import is declared
with a binding string and lowered by the generic Canonical ABI adapter
([canonical ABI and WIT](canonical-abi-and-wit.md)); the compiler has no
per-service host function.

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
    set must be exactly `wasi:cli/stdout`, `wasi:cli/stderr`, `wasi:cli/exit`,
    `wasi:io/streams` (with `wasi:io/error` and `wasi:io/poll`),
    `wasi:clocks/monotonic-clock`, and `wasi:random/random`; ABI resolution must
    reject an import outside that world.
- `abi.rs` and `abi/wasi.rs` own the service interface names and binding lookup.
  `abi/wasi.rs` must map each enabled service to its WIT interface and
  source-facing operation and expose the lookup MIR binding resolution uses. No
  module may hard-code a per-service host function.
- `wasm/lower/mod.rs` must synthesize the `run` entry that calls `main`, passes
  the result to `wasi:cli/exit.exit-with-code`, and returns `0`.
- `lib.rs` must own the build pipeline: lower to CC, lower to MIR, structure,
  encode, call `command_world` and `componentize`, validate with the target's
  features, and return the component and its WAT form.
- The WASI library must be ordinary PureScript source resolved, type-checked,
  and linked like any other module. `psrs-driver/src/wasi.rs` must embed it and
  define `log`, `error`, `now`, and the random operations over WIT imports; the
  portable `Prelude` must not import WASI.
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

Take `main = log "hello"`. The platform library defines `log` over the WIT
imports `wasi:cli/stdout#get-stdout` and
`wasi:io/streams#[method]output-stream.blocking-write-and-flush`. Linking keeps
those imports because `main` reaches them, and prunes them otherwise. Lowering
produces the Canonical ABI call shown in
[canonical ABI and WIT](canonical-abi-and-wit.md), the string literal lives in a
data segment ([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)),
and P10 synthesizes the entry that calls `main` and exits. `componentize` then
lifts the core module: the component imports `wasi:cli/stdout@0.2.12` and
`wasi:io/streams@0.2.12` (plus their support interfaces) and exports
`wasi:cli/run@0.2.12`. `wasmtime run hello.wasm` calls `run`, which writes
`hello\n` and exits with `main`'s code.

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
- **A filesystem module loader**, so libraries are discovered rather than
  embedded in the driver.
- **WASI 0.3 / async components**, once the language has async features and the
  runtime target is revised.
- **Reclamation**, so returned lists and resources do not leak
  ([canonical ABI and WIT](canonical-abi-and-wit.md)).

## Implementation notes

Console (stdout and stderr), monotonic clock, random, and exit are implemented
and have execution tests. Filesystem, arguments, environment, sockets, HTTP, and
TLS are specified but not implemented; their capability flags are disabled in
the default profile. The standard library remains embedded, but the driver
discovers user modules from the entry files' directories
(`psrs_driver::load_program_files`): it indexes sibling `.purs` files by module
name and follows the `import` graph, never searching names the embedded library
provides. Resolution, duplicate-module, and cycle checks remain in P3.

## References

- WebAssembly Component Model: components, worlds, lifting, and `wit-component`.
- WASI 0.2 `wasi:cli/command` world and the `wasi:cli/run` export.
- `wit-component` `ComponentEncoder`, `embed_component_metadata`,
  `StringEncoding::UTF8`.
- [DEC-06 — Runtime Interface via WASI and the Component Model](../../../decision/DEC-06-runtime-interface-via-wit.md),
  [DEC-05 — Target wasmtime's WebAssembly Feature Set](../../../decision/DEC-05-wasmtime-feature-set.md).
- [capability profile](capability-profile.md),
  [canonical ABI and WIT](canonical-abi-and-wit.md),
  [linear memory boundary](linear-memory-and-canonical-abi-boundary.md),
  [effects](../fp/effects.md).
