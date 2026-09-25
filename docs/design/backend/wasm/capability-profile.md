# Capability Profile

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)  
**Status:** Stable (design)  
**Prerequisites:** the WebAssembly 3.0 standard and its proposal process; the wasmparser feature model; the Component Model and WASI 0.2. Read [DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md), [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md), and [Wasm encoding](encoding-and-structuring.md) first.  
**Summary:** The target capability profile is the explicit set of WebAssembly, Component Model, and WASI capabilities an artifact is allowed to require. A pinned runtime may implement more proposals than the compiler uses, so runtime support is never inherited; a profile is a reviewed policy input. The same flags gate MIR lowering, construct the validator, and select WASI services.

## Scope

This document owns the `TargetCapabilities` profile, the meaning of each gate,
the default stable profile, and the capability checks that run during lowering
and validation. It does not own concrete layouts (see [Wasm encoding](encoding-and-structuring.md)
and [MIR](../fp/mir.md)), canonical ABI adaptation
([canonical ABI and WIT](canonical-abi-and-wit.md)), or the component world and
WASI services ([WASI platform library](wasi-platform-library.md)).

## Background

**WebAssembly 3.0 and proposals.** Core WebAssembly 3.0 is the current live
standard. It folds in several former proposals: garbage collection, typed
function references, tail calls, exception handling, multiple memories, 64-bit
memory, relaxed SIMD, extended constant expressions, and branch hinting. Other
proposals (threads, wide arithmetic, compact imports, stack switching, custom
page sizes, custom descriptors, ESM integration) are at earlier phases. The
Component Model is tracked by its own process but is deployed by runtimes and
toolchains.

**Runtime support is not a contract.** A pinned Wasmtime release can execute
more proposals than the project chooses to emit. If a lowering silently used
whatever the host accepted, the artifact's contract would change with every
runtime upgrade. [DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md)
therefore separates the execution oracle (the pinned runtime) from the
compilation target (an explicit profile), and makes adding a proposal a
reviewed decision rather than a side effect.

**Capabilities versus implemented features.** A capability flag is an *adoption
gate*: it permits a lowering to use a family. It is not a claim that the
compiler implements every instruction in that family, nor that a proposal is
end-to-end supported. Evidence is separate and is recorded per feature as a
flag, lowering and validation coverage, a regression test, and an observable
execution test.

**Component Model and WASI.** The artifact is a component, not a raw core
module. A component imports named WIT interfaces; WASI 0.2 provides the stable
synchronous set. Preview 1 is the legacy `wasi_snapshot_preview1` module ABI,
and WASI 0.3 adds native async; neither is the project's artifact ABI
([DEC-06](../../../decision/DEC-06-runtime-interface-via-wit.md)).

## Model

`TargetCapabilities` is a `Copy` struct of explicit boolean fields. Every field
is independent, so enabling one proposal cannot implicitly enable another.

```text
TargetCapabilities = {
    mutable_globals, sign_extension, nontrapping_float_to_int,
    multi_value, bulk_memory, extended_const,
    reference_types, function_references, gc,
    simd, relaxed_simd, tail_call, multi_memory, memory64,
    exceptions, wide_arithmetic, threads,
    component_model, component_async, component_map, component_implements,
    wasi_p1, wasi_p2, wasi_p3,
    wasi_cli, wasi_io, wasi_clocks, wasi_random,
    wasi_filesystem, wasi_sockets, wasi_http, wasi_tls
}
```

Two derived views make the profile operational:

- `TargetCapabilities::wasm_features() -> wasmparser::WasmFeatures` starts from
  the MVP feature set and enables exactly the fields above that have a
  wasmparser counterpart. `component_implements` has no separate wasmparser flag
  in the pinned release and is therefore recorded but not mapped.
- The MIR verifier computes a `RequiredCapabilities { reference_types,
  function_references, gc, multi_value }` set from the module it is checking,
  then compares it against the target.

### Gate meanings

| Field(s) | Gate |
| --- | --- |
| `mutable_globals`, `sign_extension`, `nontrapping_float_to_int`, `multi_value`, `bulk_memory`, `extended_const` | Core post-MVP scalar/structural features the lowerings may use. |
| `reference_types` | Any `ref`-typed value or reference instruction. |
| `function_references` | `ref.func` and `call_ref`; implies `reference_types` in practice. |
| `gc` | Defined `struct`/`array` types and `struct.*`, `array.*`, `ref.test`, `ref.cast`, `i31.*`. |
| `simd`, `relaxed_simd` | v128 value types and SIMD instructions. |
| `tail_call` | `return_call`/`return_call_ref`. |
| `multi_memory`, `memory64` | Additional memories and 64-bit addresses. |
| `exceptions`, `threads`, `wide_arithmetic` | Their respective instruction families. |
| `component_model`, `component_async`, `component_map`, `component_implements` | Component-model encoding and its async/map/implements extensions. |
| `wasi_p1`, `wasi_p2`, `wasi_p3` | Which WASI ABI generation the artifact targets. |
| `wasi_cli`, `wasi_io`, `wasi_clocks`, `wasi_random`, `wasi_filesystem`, `wasi_sockets`, `wasi_http`, `wasi_tls` | Which WASI service packages the ABI registry may resolve. |

### Invariants

- A profile is the only source of truth for what may be emitted; a lowering
  never inspects the runtime to decide.
- The validator's feature set is derived from the same profile as the lowering,
  so a module the compiler emits is accepted by exactly the features it was
  allowed to use.
- A module that requires a disabled capability is rejected with a
  source-associated diagnostic before bytes are emitted; there is no fallback.
- A WASI import from a package disabled by the profile is rejected during ABI
  resolution.

## Design

### The default profile

The checked-in `TargetCapabilities::wasmtime_wasi_0_2()` is the source of truth
for the default target and is also `Default::default()`. Its policy is:

| Capability family | Default | Rationale |
| --- | --- | --- |
| MVP values, functions, memory, structured control | Enabled | Required baseline. |
| Mutable globals, sign extension, saturating float-to-int, multi-value, bulk memory, extended const | Enabled as target capability | Cheap post-MVP features the lowerings may use; presence does not claim end-to-end support. |
| Reference types, typed function references, GC | Enabled and required when used | Closures, aggregates, casts, and `call_ref` depend on them. |
| Component Model and WASI 0.2 (`wasi_cli`, `wasi_io`, `wasi_clocks`, `wasi_random`) | Enabled and required | The artifact boundary; canonical ABI and WIT metadata are emitted. |
| SIMD, relaxed SIMD | Disabled | Optimization track only. |
| Tail calls, exceptions, multi-memory, wide arithmetic, threads | Disabled | Tail-call lowering and execution tests now exist but are gated on `tail_call`; enabling it in the stable profile still requires a profile revision ([control flow and tail calls](../fp/control-flow-and-tail-calls.md)). Other proposals have no lowering. |
| Memory64 | Disabled | Deferred; see below. |
| WASI Preview 1, WASI 0.3 | Disabled | Separate compatibility tracks; neither is the artifact ABI. |
| Component async/map/implements | Disabled | Not a stable compiler dependency. |

WASI 0.2 is chosen over 0.3 because the runtime is synchronous: 0.2 exposes
blocking `output-stream`/`input-stream` operations, while 0.3 expresses I/O with
the Component Model's async `stream<T>`/`future<T>` primitives, so even writing
to standard output would require async plumbing before the language has any
async feature. 0.2 is also the most widely deployed component baseline. Moving
to 0.3 later is a revision of this profile behind the WASI boundary and does not
reach the frontend.

Memory64 stays disabled because enabling it would break the artifact path this
profile defines: the pinned `wit-component` hardcodes 32-bit memories when it
lifts a core module into a component, and Wasmtime's WASI implementation does
not support 64-bit memories. The retained linear-memory ABI boundary is written
so a future memory64 profile can change the address type without changing CC
([linear memory boundary](linear-memory-and-canonical-abi-boundary.md)).

### Capability gating in lowering

`psrs-backend` exposes `TargetCapabilities` to callers through
`compile_with_target`, and the default `compile` path uses the stable WASI 0.2
profile. Gating happens at three boundaries:

1. **ABI resolution.** `WasiRegistry` resolves a `wasi:*` interface only if the
   corresponding package flag is set, and records an `unsupported` reason
   otherwise. P9 fails a binding with that reason before MIR emits a call.
2. **MIR verification.** `validate_target_capabilities` scans the concrete type
   table and every instruction, derives the required set, and rejects a module
   whose required set exceeds the target.
3. **Artifact pipeline.** `compile_with_target` requires `component_model`,
   `wasi_p2`, `wasi_cli`, `wasi_io`, `wasi_clocks`, and `wasi_random`, then
   encodes the component and validates it with `validator_for(target)`.

### Rejected alternatives

- **Inheriting every feature the runtime supports.** Rejected: the artifact
  contract would silently change with a runtime upgrade, and the project could
  not reason about portability ([DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md)).
- **Silent fallbacks.** Rejected: a module that needs GC must not be quietly
  lowered through a different heap. GC is the only language heap
  ([DEC-09](../../../decision/DEC-09-gc-only-language-heap.md)); an unsupported
  module fails with a source-associated diagnostic.
- **Enabling a proposal to make one instruction available.** Rejected: a flag is
  a reviewed target change that needs lowering, validation, and execution
  evidence, not a local fix.
- **Deciding the feature set in the frontend.** Rejected: target features are
  confined below Typed Core and never leak into CST, AST, HIR, THIR, or Typed
  Core ([IR boundaries](../00-ir-boundaries.md)).

## Algorithms

### Required-capability inference

```text
required = { reference_types: false, function_references: false,
             gc: false, multi_value: false }

for each defined type:
    Func(parameters, results):
        required.multi_value |= results.len() > 1
        mark each parameter and result value type
    Struct(fields) | Array(field):
        required.gc = true
        mark each field storage type

for each import:
    mark each parameter type and optional result type

for each function:
    mark each declared value type
    for each instruction: mark_instruction

mark_value_type | mark_storage_type:
    Ref { heap } => reference_types = true
        heap = Func                         => function_references = true
        heap in { Any, Eq, I31, Struct, Array, Index } => gc = true
        heap = Extern                      => (reference_types only)

mark_instruction:
    RefFunc | CallRef                      => reference_types, function_references
    ClosureNew | ClosureCall | ClosureGetCapture
        | I31New | I31GetS
        | StructNew | StructGet | StructSet
        | ArrayNew | ArrayGet | ArrayClone | ArraySet | ArrayLen => gc = true
    RefNull | RefIsNull                    => reference_types
    RefTest | RefCast                      => reference_types, gc

reject if any required flag is set but disabled in the target
```

### Validator feature set

```text
wasm_features(target):
    features = WasmFeatures::MVP
    for (feature, enabled) in [
        (MUTABLE_GLOBAL, target.mutable_globals),
        (SIGN_EXTENSION, target.sign_extension),
        (SATURATING_FLOAT_TO_INT, target.nontrapping_float_to_int),
        (MULTI_VALUE, target.multi_value),
        (BULK_MEMORY, target.bulk_memory),
        (REFERENCE_TYPES, target.reference_types),
        (FUNCTION_REFERENCES, target.function_references),
        (GC, target.gc),
        (SIMD, target.simd), (RELAXED_SIMD, target.relaxed_simd),
        (TAIL_CALL, target.tail_call),
        (MULTI_MEMORY, target.multi_memory), (MEMORY64, target.memory64),
        (EXCEPTIONS, target.exceptions), (EXTENDED_CONST, target.extended_const),
        (WIDE_ARITHMETIC, target.wide_arithmetic), (THREADS, target.threads),
        (COMPONENT_MODEL, target.component_model),
        (CM_ASYNC, target.component_async), (CM_MAP, target.component_map),
    ]:
        features.set(feature, enabled)
    return features
```

### Edge cases

- A module that uses no reference types at all still validates under the default
  profile; the required set is simply empty.
- `func`-typed references require both `reference_types` and
  `function_references`; the verifier marks both.
- A GC struct field of type `anyref` marks `gc` as well as `reference_types`;
  the same holds for a field of a defined struct type.
- An ABI resolution for a disabled package fails even if no call reaches it,
  because P9 validates every source-declared binding.

## Code map

The capability layer is two modules; [IR boundaries](../00-ir-boundaries.md)
fixes the crate-level tree. The implementation must conform to this
organization:

```text
backend/src/
  capability.rs        TargetCapabilities, the default profile, wasm_features
  mir/
    verify/
      capability.rs    RequiredCapabilities inference and the MIR gate
```

`capability.rs` owns the profile and must define:

- `TargetCapabilities` — the `Copy` struct of independent boolean gates from the
  Model section, one field per gate, with `Default` equal to
  `wasmtime_wasi_0_2()`. Enabling one proposal must not implicitly enable
  another.
- `TargetCapabilities::wasmtime_wasi_0_2() -> Self` — the stable default target.
- `TargetCapabilities::wasm_features(self) -> wasmparser::WasmFeatures` — starts
  from `WasmFeatures::MVP` and enables exactly the fields with a wasmparser
  counterpart. `component_implements` is recorded but has no separate flag.

`mir/verify/capability.rs` owns required-capability inference and the MIR gate.
It must provide:

```rust
pub fn validate_target_capabilities(
    module: &mir::Module,
    target: TargetCapabilities,
) -> Result<(), Vec<BackendError>>;
```

It derives the `RequiredCapabilities` set from the concrete type table and every
instruction, and rejects a module whose required set exceeds the target. It must
mark `func`-typed references as requiring both `reference_types` and
`function_references`, and defined aggregates, `i31`, and casts as requiring
`gc`.

The validator must be built from the same profile as the lowering.
`compile_with_target(.., target)` must require `component_model`, `wasi_p2`,
`wasi_cli`, `wasi_io`, `wasi_clocks`, and `wasi_random`, and must construct its
validator with `validator_for(target)`, whose feature set is
`target.wasm_features()`. No path may read the host runtime's features or fall
back to the dependency's defaults. WASI package gating during ABI resolution
consults the same profile through `wasi_interface_enabled(target, interface)` in
`abi.rs`.

## Invariants and verification

- `wasmtime_wasi_0_2()` enables GC, function references, reference types,
  multi-value, and the Component Model/WASI 0.2 families, and disables SIMD,
  tail calls, exceptions, memory64, threads, and Component async. A unit test
  asserts this against `wasm_features`.
- Lowering is rejected with a source-associated diagnostic whenever a required
  capability is disabled. The check runs in `verify_module_with_capabilities`,
  which P10 calls again before encoding, so no unverified MIR reaches the
  encoder.
- The validator used at P11 is built from the same profile, starting from MVP
  rather than the dependency's defaults, so a library upgrade cannot broaden the
  artifact contract.
- A feature is only `Implemented` with four pieces of evidence: a capability
  flag, lowering/validation coverage, a binary or WAT regression test, and a
  Wasmtime execution test where the behavior is observable. Runtime support
  without them remains `Available`, not `Implemented`.

## Worked example

A program that builds a closure requires `reference_types`,
`function_references`, and `gc`. Under the default profile it lowers and
validates. If a caller passes a profile with `gc: false` (and leaves reference
types on), `validate_target_capabilities` sees a `ClosureNew`/`call_ref` module
with `required.gc = true` and rejects it with the module span and a message that
the target does not support WebAssembly GC. No bytes are produced and no linear
heap fallback is attempted.

A program that declares `foreign import "wasi:filesystem/types#..." ...` is
rejected earlier: the default profile leaves `wasi_filesystem` disabled, so ABI
resolution records an `unsupported` reason and P9 reports it against the
declaration's span.

## Boundaries and interfaces

- **Input:** an explicit `TargetCapabilities` value supplied by the driver (or
  `Default::default()`), and the concrete MIR module plus its resolved bindings.
- **Output:** a permission decision per capability family, a configured
  `WasmFeatures` for the validator, and a resolved set of WASI packages for the
  ABI registry.
- **To MIR/P10/P11:** flags consulted by the ABI registry, `GcPlanner`, the MIR
  capability verifier, and the validator.
- **From frontend:** target features never appear above Typed Core
  ([IR boundaries](../00-ir-boundaries.md)).

## Open questions and future work

- **Tail calls.** [Control flow and tail calls](../fp/control-flow-and-tail-calls.md)
  proposes enabling `tail_call` in the stable profile once its lowering and
  execution evidence exist.
- **Memory64.** Revisited only when `wit-component` lifts 64-bit-memory modules
  and the WASI host supports them.
- **New WASI services.** Filesystem, sockets, HTTP, and TLS are enabled as their
  PureScript-facing libraries and ABI shapes land
  ([WASI platform library](wasi-platform-library.md)).
- **Wasmtime baseline.** Adopting a new runtime baseline is a deliberate
  revision of this profile and of [DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md),
  not an automatic consequence of an upgrade.

## Implementation notes

`multi_value`, `bulk_memory`, and `extended_const` are enabled in the profile
but no lowering currently emits multi-value function types or bulk-memory
instructions; MIR signatures and calls are single-result. `component_implements`
is recorded but has no wasmparser feature flag in the pinned release. These are
coverage gaps, not profile changes. The MIR verifier checks import signatures
as well as defined types and functions. It treats abstract `any` and `eq`
references as GC requirements alongside `i31`, aggregate, and indexed heap
types, and attributes capability failures to the MIR span and entry module.
Regressions cover independent wasmparser flag derivation, rejection by the
validator when SIMD is disabled, the four MIR-inferred gates, and independent
gating of each WASI 0.2 service package currently admitted by the component
world.

## References

- WebAssembly 3.0 specification; [WebAssembly/proposals](https://github.com/WebAssembly/proposals).
- WebAssembly Component Model and the Canonical ABI; WASI 0.2 interfaces.
- wasmparser `WasmFeatures`.
- [DEC-05 — Target wasmtime's WebAssembly Feature Set](../../../decision/DEC-05-wasmtime-feature-set.md),
  [DEC-06 — Runtime Interface via WASI and the Component Model](../../../decision/DEC-06-runtime-interface-via-wit.md),
  [DEC-09 — GC-Only Language Heap](../../../decision/DEC-09-gc-only-language-heap.md).
- [Wasm encoding](encoding-and-structuring.md),
  [canonical ABI and WIT](canonical-abi-and-wit.md),
  [linear memory boundary](linear-memory-and-canonical-abi-boundary.md).
