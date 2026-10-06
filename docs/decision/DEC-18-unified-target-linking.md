# DEC-18 — Unified Target Linking

**Status:** Accepted; implemented for the core-Wasm formatter slice. General
guest WIT provider composition remains unimplemented.
**Date:** 2026-10-07.

## Context and constraints

Language intrinsics, library foreign declarations, WIT interfaces, and executable
Wasm libraries meet at the target boundary. Giving each a separate name table,
dependency scan, or component attachment path obscures who provides a function,
which signature was checked, and which memory its pointers address.

`numberToString :: Number -> String` exposes this problem. Adding `ryu-js` to the
native compiler does not implement runtime formatting in generated programs.
An embedded implementation also needs checked raw calls, output recovery,
storage reservations, dependency closure, and safe instantiation ordering.
These are linking obligations, not incidental details of component encoding.

WIT packages supply interface definitions. Executable providers may be a host
or a guest component. A definition being available does not discharge an import.
Existing source typing, representation evidence, resource ownership, and the
WASI host policy must survive provider selection.

## Decision

Define one target-linking contract, specified in
[linking and runtime](../design/backend/wasm/linking-and-runtime.md).
Use a single checked link plan for implementation selection, artifact dependency
closure, memory layout, instantiation, and final import verification.

The owners remain distinct:

- HIR owns intrinsic identities and language schemes; Core checks their uses.
- The library owns PureScript wrappers and algorithms.
- The runtime package owns pinned WIT definitions, the default command world,
  executable artifacts, and their raw contracts/provenance. Its data API has no
  dependency on compiler IR or backend types. Move `psrs-backend/wit/` into
  `psrs-runtime/wit/`; both ABI resolution and componentization consume this
  single catalog.
- The target implementation catalog maps intrinsic identities to direct
  instructions, generated helpers, or artifact-backed implementations.
- `psrs-linker` owns WIT definition loading and resolved-world identity. The
  backend validates source/WIT types and lowers canonical calls using that same
  world context. Provider bindings select host or guest implementations explicitly.
- The independent target linker validates these contracts together and produces an immutable
  plan consumed by Wasm emission and component assembly.

Source-module linking remains with the driver. It passes checked declarations
and package provenance to the backend; it does not resolve target exports.
The link plan is target metadata alongside MIR, not another language IR or a
WIT type system inside Core.

WIT loading and target composition belong in `psrs-linker`; source/canonical
validation and language-value recovery remain in the backend. Runtime asset
ownership must not move compiler passes into target execution code. Metadata and
target-code build entry points are separate; consuming the catalog alone does
not link the formatter into the native compiler. Interface membership comes from
the resolved world rather than a second handwritten interface whitelist.

Use the selected `ryu-js` route as the first artifact-backed intrinsic. Its
implementation accepts binary64 and a caller-owned output buffer, writes an
ECMAScript token, and returns initialized byte length. MIR copies UTF-8 into a
GC String and frees the buffer. The library owns the official `Show` `.0` rule,
character/string escaping, and ordinary array callback traversal.

The formatter is a core-Wasm library with an explicit shared-memory contract.
Its Rust stack and static data are private execution storage, not a language
heap. The linker must validate their reservation and the allocator boundary
before emission; output recovery must preserve DEC-16 and DEC-10 ownership.
Guest components use their own canonical ABI memory and resource identities;
the linker must not impose the core helper's shared-memory model on them.

Embed pinned compiler-owned runtime artifacts, including source, dependency,
toolchain, recipe, and artifact hashes. Normal compiler builds do not compile
target Rust or download runtime libraries. WIT definitions and independently
supplied guest artifacts require equally explicit provenance and binding data.
There is no implicit dependency discovery, provider fallback, or network fetch.

## Consequences

The linker is an independent crate with a checked target API. The dependency
direction is `backend -> linker -> runtime`, with backend also consuming runtime
catalog data. The linker depends on WIT/Wasm tooling and target records, never
backend or compiler IR crates. The backend converts IR requirements and maps
structured linker errors to source diagnostics. Component encoding executes
the plan; it does not rediscover dependencies by import-name heuristics.
Each requirement has a provider, source origin, verified boundary, and lifetime.
Unused definitions do not automatically add imports, code, or capabilities.
Required initializer dependencies remain live even without ordinary call edges.

DEC-06 remains the host-interface decision: a private embedded helper import is
closed inside the component and never becomes a project-specific host ABI.
This proposal extends DEC-09/DEC-10 to admit explicitly reserved target-library
execution storage, while retaining Wasm GC as the only language heap and bounded
ownership for buffers carrying language values.

Arbitrary object-file relocation, runtime dynamic loading, ABI coercion between
incompatible versions, and shared GC objects across separately built libraries
are not implied by this decision. Artifact composition must respect each chosen
boundary. Additional supported input formats need their own conversion and
verification contracts.

The existing uncommitted Show/runtime prototype is not evidence that this
contract is implemented. Its late attachment, duplicated checks, and fixed
reservation must be reconciled with the checked plan before acceptance. General
guest WIT component linking remains unimplemented; the design preserves its
requirements rather than describing the prototype as a complete linker.
