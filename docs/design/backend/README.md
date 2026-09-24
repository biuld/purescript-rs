# Backend Design

The backend does exactly two jobs. Every document in this directory serves one
of them; `00-ir-boundaries.md` is the cross-cutting contract between them.

1. **Functional semantics** (`fp/`) — represent and execute a typed functional
   core: functions and closures, algebraic data types and pattern matching,
   records and arrays, polymorphism and erasure, recursion and control flow, and
   later type classes and effects.
2. **The Wasm/WASI target** (`wasm/`) — lower those representations to Wasm GC
   and the WASI 0.2 Component Model: encoding and structuring, the capability
   profile, the canonical ABI and WIT, the linear-memory boundary, and the WASI
   platform library.

## Pipeline

```mermaid
flowchart LR
    core["Typed Core"] --> cc["CC IR<br/>ANF + closure conversion"]
    cc --> mir["MIR<br/>SSA/CFG, representation fixed"]
    mir --> wasm["structured Wasm encoding"]
    wasm --> artifact["WASI component artifact"]
```

Each arrow is a pass; the topic tables below link the documents for each stage.

Representation decisions belong to MIR; the Wasm layer only structures and
encodes. `00-ir-boundaries.md` states the ownership and verification contract.

## Topics

### Cross-cutting

| Document | Owns |
| --- | --- |
| [00-ir-boundaries.md](00-ir-boundaries.md) | Pass pipeline, IR ownership, boundary verification, P10/P11 |

### Functional (`fp/`)

| Document | Owns | Depends on |
| --- | --- | --- |
| [functional-core.md](fp/functional-core.md) | The typed core calculus the backend targets | — |
| [cc-ir.md](fp/cc-ir.md) | ANF, closure conversion, CC operations and verifier | functional-core |
| [mir.md](fp/mir.md) | SSA/CFG model, representation planning, MIR verifier | cc-ir |
| [polymorphism-and-erasure.md](fp/polymorphism-and-erasure.md) | Rank-1 polymorphism, erased representation, adapters | mir |
| [scalars-and-primitives.md](fp/scalars-and-primitives.md) | Scalar values and numeric operations | mir |
| [data-representation.md](fp/data-representation.md) | ADTs, records, arrays, closures; GC layouts and evidence | mir |
| [pattern-matching.md](fp/pattern-matching.md) | Pattern decision trees and exhaustiveness | cc-ir |
| [control-flow-and-tail-calls.md](fp/control-flow-and-tail-calls.md) | CFG structuring, `Switch`, tail calls | mir |
| [type-classes-and-dictionaries.md](fp/type-classes-and-dictionaries.md) | Dictionary passing and instance evidence | functional-core |
| [effects.md](fp/effects.md) | Effect representation and the platform boundary | functional-core |

### Wasm/WASI (`wasm/`)

| Document | Owns | Depends on |
| --- | --- | --- |
| [encoding-and-structuring.md](wasm/encoding-and-structuring.md) | Structured Wasm encoding and binary emission | 00-ir-boundaries |
| [capability-profile.md](wasm/capability-profile.md) | Target capability profile and gating | encoding-and-structuring |
| [canonical-abi-and-wit.md](wasm/canonical-abi-and-wit.md) | WIT bindings and canonical ABI adaptation | encoding-and-structuring |
| [linear-memory-and-canonical-abi-boundary.md](wasm/linear-memory-and-canonical-abi-boundary.md) | Linear memory as the ABI boundary | canonical-abi-and-wit |
| [wasi-platform-library.md](wasm/wasi-platform-library.md) | Component packaging and WASI services | canonical-abi-and-wit |

## Ordering

The functional concern is built bottom-up: functional core, then CC and MIR,
then representation topics (erasure, scalars, data, patterns, control flow),
then dictionaries and effects. The Wasm/WASI concern is independent of the
functional topic order and is gated by the capability profile.

## Writing

Every topic document follows the template in [`AGENTS.md`](../../../AGENTS.md):
Scope, Background, Model, Design, Algorithms, Code map, Invariants and
verification, Worked example, Boundaries and interfaces, Open questions,
References. Each document is a complete design and also a learning resource for
readers with a functional-programming, WebAssembly, and compilers background.
