# D-05 — Backend Capability Profile

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Purpose

[DEC-05](../decision/DEC-05-wasmtime-feature-set.md) fixes the policy: the
target is the WebAssembly feature set of a pinned `wasmtime` release, including
features that are standardized as well as proposals still in preview. This
document turns that policy into a concrete, verifiable capability profile: the
spec baseline, the exact features the backend may use, the WASI surface, and how
each is verified. MIR layout and the structured Wasm encoding may only use what
is listed here.

## Spec baseline

As of 2026-09:

- **Core WebAssembly 3.0** is the current live standard (released 2025-09-17,
  W3C Candidate Recommendation drafts continuing through 2026). It includes
  garbage collection, typed function references, tail calls, exception handling,
  multiple memories, 64-bit memory, relaxed SIMD, extended constant expressions,
  and branch hinting.
- **Proposal phases** ([WebAssembly/proposals](https://github.com/WebAssembly/proposals)):
  Phase 4 (standardize) covers threads, wide arithmetic, and compact imports;
  Phase 3 (implementation/preview) covers stack switching, custom page sizes,
  custom descriptors, and ESM integration; the component model is tracked
  separately and is not yet Phase 4 in the Working Group process even though
  runtimes and toolchains deploy it.
- **`wasmtime` 48.0.2** enables by default the Phase 4+ set: mutable globals,
  sign extension, saturating float-to-int, reference types, multi-value, bulk
  memory, SIMD, relaxed SIMD, threads, tail calls, multi-memory, exceptions,
  64-bit memory, extended constant expressions, the component model, typed
  function references, and garbage collection. It leaves shared-everything
  threads, memory control, custom page sizes, and stack switching off by
  default. GC structs, `call_ref`, and `i31` were verified to run under this
  release.
- **Toolchain:** `wasm-encoder`, `wasmparser`, and `wasmprinter` are pinned to
  `0.245.1` and emit, validate, and print the features above.
- **WASI:** Preview 1 (`wasi_snapshot_preview1`) is the legacy module API;
  Preview 2 (WASI 0.2) is stable on the Component Model; Preview 3 (WASI 0.3)
  is the current stable release and adds native async (`async func`,
  `stream<T>`, `future<T>`). `wasmtime` 46 and later run WASI 0.3 components by
  default.

## Capability profile

| Capability | Baseline | Use |
| --- | --- | --- |
| Core numeric and control (`i32`/`i64`/`f32`/`f64`, structured control) | WebAssembly 3.0 | Required |
| Reference types and typed function references | wasmtime default | Required for closures and aggregate references |
| Garbage collection (`struct`, `array`, `i31`, rec groups, subtyping, `ref.test`/`br_on_cast`) | wasmtime default | Required for aggregates and closures |
| Tail calls | wasmtime default | Allowed; used where a construct needs guaranteed tail calls |
| Exception handling | wasmtime default | Allowed; not used initially |
| Multi-memory and 64-bit memory | wasmtime default | Allowed; not used initially |
| SIMD and relaxed SIMD | wasmtime default | Allowed; not used initially |
| Threads | wasmtime default | Allowed; the runtime is single-threaded initially |
| Component model and WASI 0.2 | wasmtime default | Platform target; synchronous interfaces match the runtime, and a component emitter and canonical ABI are required |
| WASI 0.3 | wasmtime 46+ | Later opt-in when async, streams, or futures are needed |
| WASI Preview 1 (`wasi_snapshot_preview1`) | wasmtime default | Not used; the standard library targets WASI 0.2 |
| Stack switching, shared-everything threads, custom page sizes, custom descriptors | opt-in preview | Not used; adopting one requires updating this profile first |

WASI 0.2 is the chosen component baseline rather than 0.3 because the runtime
is synchronous: 0.2 exposes blocking `output-stream`/`input-stream`
operations, while 0.3 expresses I/O with the component model's async
`stream<T>`/`future<T>` primitives, so even writing to standard output would
require async plumbing before the compiler has any async language features.
0.2 is also the most widely deployed component baseline. Moving to 0.3 later is
a revision of this profile and stays behind the WASI boundary, so it does not
reach the frontend.

Target representations per [D-02](D-02-wasm-lowering.md): a data type whose
constructors are all nullary uses immediate integer tags; a data type with
fields uses a `rec` group of GC `struct` types under an abstract supertype;
closures are GC `struct` values holding a `funcref` and their captures; records
and arrays use GC `struct`/`array`. Linear memory is reserved for the
byte-oriented WASI boundary.

## Runtime interface

- **Current artifact:** a WASI 0.2 component that exports `wasi:cli/run@0.2.12`
  and imports only the WASI interfaces the program uses (console via
  `wasi:cli/stdout` and `wasi:cli/stderr`, the monotonic clock via
  `wasi:clocks/monotonic-clock`, and `wasi:cli/exit`). The core module is lifted
  into the component with `wit-component` and runs under `wasmtime`.
- **Platform target:** a WASI 0.2 component with a `wasi:cli/command` entry.
  WASI is the runtime ABI ([DEC-06](../decision/DEC-06-runtime-interface-via-wit.md)):
  the PureScript-facing standard library is built on WASI interfaces, and the
  backend lowers to their canonical ABI. A component imports only the WASI
  capabilities the program uses. `build` emits a component and the standard
  library implements console and exit over WASI.

## Verification

- The validator is configured for the same feature set the baseline enables.
  `wasmparser`'s default feature set currently matches `wasmtime` 48's default
  set, including GC, so a GC artifact validates without custom feature
  configuration; if the defaults diverge, the compiler must set the features
  explicitly.
- Execution tests run under the pinned `wasmtime`, and the artifact may require
  the features in this profile to load.
- New capabilities are adopted by adding a row to the profile and an execution
  test, not by relying on engine accident.

## Open items

- Building the remaining WASI 0.2 interfaces (files, clocks, random, sockets)
  as PureScript-facing libraries.
- Whether a language feature needs tail calls, exceptions, or stack switching;
  each is added to the profile before use.
- Tracking the `wasmtime` baseline: a new release is a deliberate revision of
  this profile and DEC-05.
