# DEC-06 — Runtime Interface via WASI and the Component Model

**Status:** Accepted  
**Date:** 2026-09-19

## Context

[wasm encoding](../design/backend/wasm/encoding-and-structuring.md) separates three layers: a
PureScript-facing library, a compiler runtime ABI, and the WASI host interface.
The bootstrap collapsed the last two: the backend synthesizes `ps_rt_log`, hand
codes the WASI Preview 1 `fd_write` convention, and lays out an iovec and
`nwritten` pointer in linear memory. That bakes a WASI version, a string
representation, and a host calling convention into the compiler.

WASI already defines the host ABI in WIT and passes values through the
canonical ABI. Defining a second, project-specific ABI (`psrs:runtime`) would
duplicate that role and add a layer to design, version, and adapt.

## Decision

The runtime interface is **WASI itself**, and the PureScript-facing standard
library is built on top of WASI; the project does not define its own host ABI.

- The compiler targets **WASI 0.2** through the Component Model. Vendored WASI
  0.2.12 WIT is the source of host interfaces; the compiler lowers to their
  canonical ABI and never hand-codes a host ABI.
- A pure-PureScript (or small runtime) **standard library** implements
  higher-level operations on WASI: for example, a `print`/`log` writes to
  `wasi:cli/stdout` through the `wasi:io/streams` `output-stream`, and program
  exit uses `wasi:cli/exit`.
- No project-specific WIT package (`psrs:runtime`) or synthetic runtime symbol
  (`ps_rt_log`) is a host interface. The compiler's runtime table describes
  **WASI imports**, not a new ABI.
- The artifact is a component whose world exports `wasi:cli/run@0.2.12` and
  imports only the WASI interfaces the program uses. Because WASI is provided
  by the host, no custom adapter is needed for the interfaces we target.
- WIT and canonical ABI details never appear in Typed Core or THIR. MIR may
  carry target reference and memory types; the ABI lowering owns WASI import
  names and signatures.

## Consequences

- The compiler must lower to WASI's canonical ABI, including resources
  (`output-stream`), lists (`list<u8>`), and result types. This is more work
  than a bespoke `log(string)`, but it removes a project ABI and a fragile
  Preview 1 emitter.
- Imports are capabilities: vendor all WASI WIT for name resolution, but a
  component imports only what the program uses, so it stays minimal and
  instantiable under a constrained host.
- `psrs:runtime`, `ps_rt_log`, and the Preview 1 emitter are removed.
- Rejected: defining a project-specific runtime ABI in WIT and adapting it, and
  hand-coding WASI Preview 1 `fd_write`.
