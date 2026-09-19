# DEC-06 — Runtime Interface via WIT and the Component Model

**Status:** Accepted  
**Date:** 2026-09-19

## Context

[D-02](../design/D-02-wasm-lowering.md) separates three layers: a
PureScript-facing library, a compiler runtime ABI, and the WASI host interface.
The bootstrap collapsed the last two: the backend synthesizes `ps_rt_log`, hand
codes the WASI Preview 1 `fd_write` convention, and lays out an iovec and
`nwritten` pointer in linear memory. That bakes a WASI version, a string
representation, and a host calling convention into the compiler, and it
conflicts with the WASI 0.2 target fixed by
[DEC-05](DEC-05-wasmtime-feature-set.md) and
[D-05](../design/D-05-backend-capability.md).

The Component Model defines host interfaces in WIT and passes values through the
canonical ABI, so the compiler can describe the runtime interface instead of
hard-coding one host ABI. `wit-parser` and `wit-component` parse WIT and turn a
core module plus adapters into a component, and `wasmtime` runs the result.

## Decision

The compiler's runtime interface is defined in WIT and emitted as a WebAssembly
component targeting WASI 0.2, not a hand-coded Preview 1 core module.

- No host ABI is written by hand. The backend emits a core module that calls the
  runtime ABI at the canonical ABI level, and `wit-component` lifts it into a
  component; adapters map the runtime ABI onto WASI where needed.
- The runtime ABI is a WIT package (`psrs:runtime`) whose operations are
  expressed in WIT (for example `log: func(msg: string)`). Canonically, a
  `string` crosses as a linear-memory `(ptr, len)` pair.
- MIR declares runtime imports in an import table (import module, name, and
  canonical signature) and calls them like any other function; it also gains the
  linear-memory load/store needed to adapt values to the canonical ABI. WIT
  types never appear in Typed Core, THIR, or MIR.
- The Wasm emitter reads a runtime ABI registry derived from the WIT package
  instead of special-casing a language symbol.
- `ps_rt_log` and the Preview 1 emitter are bootstrap-only and are removed once
  the component path replaces them.

## Consequences

- The artifact becomes a component. Exit semantics change: WASI 0.2
  `wasi:cli/exit` reports only success or failure, so an integer `main` result
  can no longer be the process exit code. The project must decide whether
  `main` keeps an integer result for core-module artifacts or maps to
  success/failure for components.
- The build depends on WIT tooling: `wit-parser` for the registry and
  `wit-component` for componentization, plus a runtime adapter. Host-to-guest
  strings may require `cabi_realloc`.
- The compiler gains an explicit runtime ABI registry and MIR import table,
  which removes host-ABI knowledge from the emitter.
- Rejected: continuing to hand-code WASI Preview 1 `fd_write` and iovecs, and
  hard-coding imports in the Wasm lowering.
