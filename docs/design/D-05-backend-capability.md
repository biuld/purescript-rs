# D-05 — Backend Capability Profile

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress

## Purpose

[DEC-05](../decision/DEC-05-wasmtime-feature-set.md) fixes the policy: the
runtime baseline is pinned, but an artifact may require only the capabilities
declared by its target profile. This document turns that policy into a
verifiable capability profile: the Core Wasm baseline, proposal gates, the
Component Model/WASI surface, and the evidence required for each capability.
MIR layout and the structured Wasm encoding may only use what is enabled for
the selected target.

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
- **Wasmtime 48** remains the execution baseline for the current project
  decision. Its runtime support is broader than the compiler's stable output
  contract: runtime support alone does not turn a proposal into a backend
  dependency. GC structs, `call_ref`, and `i31` are the current representation
  primitives.
- **Toolchain:** `wasm-encoder`, `wasmparser`, and `wasmprinter` are pinned to
  `0.245.1` and emit, validate, and print the features above.
- **WASI:** Preview 1 (`wasi_snapshot_preview1`) is the legacy module API;
  Preview 2 (WASI 0.2) is the stable synchronous project target. WASI 0.3
  adds native async (`async func`, `stream<T>`, `future<T>`), but its Wasmtime
  host support is not part of this production profile.

## Capability profile

The checked-in `TargetCapabilities::wasmtime_wasi_0_2()` profile is the source
of truth for the default target. The following table summarizes its policy;
the implementation must not infer these values from a Wasmtime release.

| Capability family | Default profile | Current backend use |
| --- | --- | --- |
| Wasm MVP values, functions, memory, and structured control | Enabled | Required; the current module skeleton covers only a subset of MVP sections and instructions. |
| Mutable globals, sign extension, saturating float-to-int, multi-value, bulk memory, extended const | Enabled as target capability | Multi-value and bulk memory are reserved for later lowerings; their presence does not claim end-to-end support. |
| Reference types, typed function references, and GC | Enabled and required when used | Required by current closures, aggregates, casts, and `call_ref`. |
| SIMD and relaxed SIMD | Disabled | Optimization track only. |
| Tail calls, exceptions, multi-memory, threads, wide arithmetic | Disabled | No lowering may emit them until a profile change and execution tests land. |
| Memory64 | Disabled | Deferred: the pinned component toolchain cannot lift a 64-bit-memory core module and the WASI host path is incomplete; see below. |
| Component Model and WASI 0.2 | Enabled and required | Current artifact boundary; canonical ABI and WIT metadata are emitted. |
| WASI Preview 1 and WASI 0.3 | Disabled | Separate compatibility tracks; neither is the current artifact ABI. |
| Component async/map/implements and experimental proposals | Disabled | Not a stable compiler dependency. |

WASI 0.2 is the chosen component baseline rather than 0.3 because the runtime
is synchronous: 0.2 exposes blocking `output-stream`/`input-stream`
operations, while 0.3 expresses I/O with the component model's async
`stream<T>`/`future<T>` primitives, so even writing to standard output would
require async plumbing before the compiler has any async language features.
0.2 is also the most widely deployed component baseline. Moving to 0.3 later is
a revision of this profile and stays behind the WASI boundary, so it does not
reach the frontend.

Memory64 stays disabled because enabling it would break the WASI artifact path
that this profile defines:

- the pinned `wit-component` hardcodes 32-bit memories when it links a core
  module into a component, so a core module with a 64-bit memory cannot be
  lifted into a component artifact; and
- Wasmtime's WASI implementation does not yet support 64-bit memories, so the
  artifact could not execute even if it were lifted.

The retained linear-memory ABI boundary is written so a future memory64 profile
can change the address type without changing CC
([D-10](D-10-linear-memory-representation.md)); memory64 is revisited only when
the component toolchain lifts 64-bit memories and the WASI host supports them,
with its own lowering and execution tests.

Target representations per [D-02](D-02-wasm-lowering.md) and
[DEC-08](../decision/DEC-08-target-neutral-variant-representation.md): a data
type whose constructors are all nullary uses immediate integer tags; a supported
non-parameterized data type with fields uses one immutable GC `struct` per
constructor under a tag-carrying abstract supertype; closures are GC `struct`
values holding a `funcref` and their captures; records and arrays use GC
`struct`/`array`. Under [DEC-09](../decision/DEC-09-gc-only-language-heap.md) GC
is the only language heap; linear memory serves only the byte-oriented WASI
boundary ([D-10](../design/D-10-linear-memory-representation.md)).

## Runtime interface

- **Current artifact:** a WASI 0.2 component that exports `wasi:cli/run@0.2.12`
  and imports only the WASI interfaces the program uses (console via
  `wasi:cli/stdout` and `wasi:cli/stderr`, the monotonic clock via
  `wasi:clocks/monotonic-clock`, random bytes via `wasi:random/random`, and
  `wasi:cli/exit`). The core module is lifted into the component with
  `wit-component` and runs under `wasmtime`.
- **Platform target:** a WASI 0.2 component with a `wasi:cli/command` entry.
  WASI is the runtime ABI ([DEC-06](../decision/DEC-06-runtime-interface-via-wit.md)):
  the PureScript-facing standard library is built on WASI interfaces, and the
  backend lowers to their canonical ABI. A component imports only the WASI
  capabilities the program uses. `build` emits a component and the standard
  library implements console and exit over WASI.

## Feature-aware lowering and verification

`psrs-backend` exposes `TargetCapabilities` and accepts an explicit profile
through `compile_with_target`. The default `compile` path uses the stable
WASI 0.2 profile. The Wasm lowering inspects MIR and rejects a module that
requires GC, reference types, typed function references, or multi-value when
the selected profile disables that capability. The validator is created from
the same profile, starting at the MVP set rather than the dependency's default
feature set.

This is a capability gate, not an implicit fallback implementation. The GC
planner is the only language-heap strategy
([DEC-09](../decision/DEC-09-gc-only-language-heap.md)); linear memory carries
only the byte-oriented canonical ABI and WASI boundary. A module that requires a
disabled capability fails with a source-associated backend diagnostic; there is
no fallback language-heap lowering.

Every new checklist item needs four pieces of evidence before it becomes
`Implemented`: a capability flag, lowering/validation coverage, a binary or
WAT regression test, and a Wasmtime execution test where the feature is
observable. Runtime support without these artifacts remains `Available`, not
`Implemented`. [D-11](D-11-gc-representation-and-evidence.md) tracks the
execution-evidence matrix.

## CC/MIR capability audit

The checklist is intentionally finer-grained than the current backend matrix.
The following audit records the ownership boundary so that a capability is not
considered covered merely because a Wasm opcode or a low-level type exists.

| Capability family | CC status | MIR status | Design assessment |
| --- | --- | --- | --- |
| MVP scalar values and operations | CC `Integer`, `Boolean`, and `Number` shapes lower through the full D-09 unary and binary operation set. | MIR checks exact scalar operand/result types; saturating `NumberToInt` uses an MVP sequence; GC execution fixtures cover every CC operation variant. | Scalar backend slice implemented. Source intrinsic integration remains tracked by the frontend matrix; broader call and ABI coverage remains partial. |
| Structured control | Expression-level `if` and `case` are explicit in ANF. | CFG has `if` diamonds, jumps, merge parameters, and returns. | Partial; the source language has no loops, so `loop`/`br_table` are added only when a language feature needs them. |
| Linear memory | Strings and WIT byte-list boundaries select the pointer representation. | Only the canonical ABI boundary uses linear memory: length-prefixed UTF-8 strings and byte lists, the return area for canonical calls, active data segments, and a bump `cabi_realloc`. The verifier checks value types, memory identity, alignment, and statically addressed access extents. There is no linear-memory language heap ([DEC-09](../decision/DEC-09-gc-only-language-heap.md)); language arrays and aggregates use GC operations. | Partial; broader canonical ABI support remains in [D-10](../design/D-10-linear-memory-representation.md) and [D-07](D-07-wit-imports-and-std.md). |
| Multi-value | No multi-result CC operation. | Function/import signatures and calls have one result; the verifier rejects multi-result `call_ref`. | Correctly marked Partial; do not enable it as an implementation claim. |
| Bulk memory, globals, SIMD, tail calls, exceptions, threads | CC has no proposal-specific shapes. | Language arrays and aggregates use GC operations; the listed proposal families have no operations or resources in the current backend. | Profile flags are policy inputs only; they are not lowering coverage. |
| Reference types and GC | CC carries symbolic `ReprId`/`SignatureId` requirements, abstract references, closure shapes, logical fields, and one `Variant` per sum type ([DEC-08](../decision/DEC-08-target-neutral-variant-representation.md)); it has no Wasm type indices. | MIR plans the current GC layout and owns `RecGroup` and concrete reference types; the GC planner realizes a variant as a tag-carrying abstract supertype plus case subtypes. Typed erased scalar/reference adaptation has GC execution evidence. | Partial; broader proposal coverage remains outside the enabled profile. |
| Component Model and WASI | CC keeps only an external `SymbolId` and abstract signature. `BackendInput::externals` carries WIT names and source signatures beside CC. | MIR resolves bindings through the WIT ABI registry, emits adapters for referenced calls, and retains only used runtime imports. Direct record projections and WIT flag word packing have P9 lowering tests. | Partial; enum and flags runtime evidence, source record-signature integration, and broader aggregate ABI coverage remain in [D-07](D-07-wit-imports-and-std.md). |

The residual structural issue is deliberately called out here: linear memory is
a byte-oriented canonical ABI boundary, not a language heap, and it is not a
claim that every GC or WIT operation has a linear representation
([DEC-09](../decision/DEC-09-gc-only-language-heap.md)). CC verification
type-checks every operation in the current abstract operation set. Any new CC
operation must add an operation-level verifier before it is used as evidence for
a capability row.

## Open items

- Building the remaining WASI 0.2 interfaces (files, sockets, and HTTP) as
  PureScript-facing libraries and enabling their capability flags.
- Whether a language feature needs tail calls, exceptions, or stack switching;
  each is added to the profile before use.
- Revisiting memory64 once `wit-component` lifts 64-bit-memory modules into
  components and the WASI host supports them; until then it stays disabled.
- Tracking the `wasmtime` baseline: a new release is a deliberate revision of
  this profile and DEC-05.
