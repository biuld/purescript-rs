# Compilation Units and Runtime Linking

**Feature:** [F-02 — Build Portable Program Artifacts](../../../feature/F-02-portable-programs.md)\
**Status:** Draft\
**Prerequisites:** [IR boundaries](../00-ir-boundaries.md), [encoding](encoding-and-structuring.md), [linking and runtime](linking-and-runtime.md), and [canonical ABI and WIT](canonical-abi-and-wit.md).\
**Summary:** Separate source checking, codegen grouping, Core linking, and component deployment. Give compiled source and structured runtime units one checked provider graph, implemented by the compiler and `psrs-linker`. Assemble application code in Core, lower it to MIR, assign final Wasm indices, and compose its runtime Core Modules into a component without an external application object linker.

## Scope

This topic owns compilation-unit boundaries and the integration of compiled
source with runtime implementations for [issue #144](https://github.com/biuld/purescript-rs/issues/144).
The linker consumes target requirements and executable artifacts; it does not
parse or typecheck PureScript source. Backend lowering owns GC representations,
closure conventions, and source-to-target evidence. WIT owns external interface
contracts. The [allocator adapter](canonical-realloc-runtime-adapter.md) owns
allocator selection and canonical allocation behavior.

The design uses compiler-owned Core assembly and target linking. Application
objects, archives, relocations, and invocation of an external object linker are
out of scope. Runtime artifact production uses its language toolchain separately;
consuming a pinned runtime artifact does not require that toolchain at app build time.

## Background

The existing driver lowers source modules with `lower_module_unverified`, merges
them with `psrs_core::link`, selects/prunes the entry graph, and verifies the
linked result. The backend optimizes and emits one application Core Wasm module.
`psrs-linker` resolves WIT definitions, checks runtime providers and memory,
and composes libraries using `ComponentEncoder::library`. Its guest API composes
explicit component-interface providers separately.

Source assembly, library composition, and guest composition are implemented
mechanisms. Persistent checked-Core caching remains a design target. The runtime
unit graph below is implemented by the `psrs-runtime` catalog and
`psrs-linker::plan`: each live operation selects one compatible unit, required
operations join that closure, and conflicting state, growth, memory identity, or
instantiation order is a planning error. A resolved source name is not proof
that an executable runtime implementation exists.

| Operation | Input | Output |
| --- | --- | --- |
| Source/Core assembly | Lowered source modules and checked dependencies | Verified linked Core |
| Runtime Core library composition | Application plus verified runtime Core Modules | Component containing collaborating Core instances |
| Guest composition | Components and explicit interface bindings | Composed component |

WIT files are definitions, not executable providers. An ordinary language import
creates neither a WIT interface nor a canonical ABI boundary. Private runtime
Core imports have raw checked ABIs and may share memory without canonical lifting.

## Model

### Independent compilation units

| Unit | Owner | Contract |
| --- | --- | --- |
| Source module | Frontend/driver | Module identity, checked declarations, imports and semantic dependency evidence |
| Codegen unit | Compiler planner/backend | The assembled application closure; one application emission unit in this design |
| Application assembly/emission unit | Compiler Core assembler and backend | Root/export closure assembled into one verified Core module and emitted as one Core Wasm module |
| Component unit | Deployment configuration/linker | Selected WIT world, application and runtime providers |

### Minimum units and concrete outputs

The minimum independently checked source unit is one PureScript module, with its
imported semantic environment. Its compiler output is module Core plus dependency
contracts, not a dedicated Wasm `.o` and not WIT. A `.purs` path is not an
artifact identity. A declaration or function is a reachability/optimization item,
not an independently deployable unit.

The minimum application Wasm emission unit is one executable dependency closure:
selected entry/export roots plus all reachable source declarations and required
representation definitions, assembled into one verified Core module. A trivial
program can have a single source module in that closure; an importing program
includes the reachable definitions it needs. This is a logical dependency closure,
not a rule that every declaration in every imported source file must be emitted.
Until explicit library export roots are implemented, command entry roots remain
the supported application mode; do not treat an absent entry as an empty program.

A runtime implementation unit produces one independent Core Wasm module with
checked imports/exports, storage and initialization contracts. Stateful operations
that must share one instance stay in that unit. A component contains the application
Core module and selected runtime instances, exposed through one selected WIT world.
It can therefore contain several Core modules while the application has one emitter.

A complete application build produces application Core Wasm bytes, a final
component, and a link report identifying both artifacts and their input contracts.
Persistent source-module cache bytes are a separate future compiler-owned format.
There is no application object-file intermediate in this design.

Before publishing a source-module cache as checked, verify its local invariants
against explicit imported contracts, then recheck linked closure when composing
modules. The current unverified per-module lowering cannot directly become this
cache API. Cache keys must include source/package identities, compiler/IR schema,
semantic dependency evidence, target-sensitive assumptions, and policy versions.
Loading must remap session-local IDs through stable identities and preserve
source origins; serialized numeric IDs alone are not persistent identities.

### Runtime packages and units

Runtime catalog data remains owned by `psrs-runtime`; extend that catalog rather
than introduce competing backend and linker registries.

```text
RuntimePackage = identity + version + provenance + units
RuntimeUnit = {
  identity, semantic_contract_version,
  provided_operations, required_operations,
  artifact_variants,
  state_ownership, storage_constraints, initialization_contract
}
ArtifactVariant = {
  kind: CoreModule | Component,
  bytes_and_digest, build_recipe, required_features,
  exports_and_imports, calling_contracts
}
```

Units group operations that share implementation or state. They are neither
mandatorily monolithic nor one function each. Unit boundaries need not match
PureScript modules or WIT interfaces. Advertise only variants actually produced
and verified. Cross-kind alternatives require explicit semantic and state
compatibility; equal symbol names are insufficient.

A consumer requirement includes operation identity/version, origin, expected
ABI, and provider constraints. Primitive semantics stay in HIR/library contracts;
GC representation evidence stays backend-owned. The linker consumes target-only
checked descriptors and does not invent a second source or GC type system.
Provider metadata must be checked against independently produced consumer evidence.

```text
TargetProviderGraph = {
  roots: application requirements + generated helpers + retained initialization,
  selected_units_and_variants, resolved_edges,
  memory_and_state_constraints, initialization_dependencies,
  permitted_external_world, input_digests
}
```

Each live requirement has exactly one selected compatible provider. Runtime-unit
dependencies join the same graph as source-generated requirements. Duplicate
state owners, ambiguous providers, unsupported cycles, and missing metadata are
planning errors. Selection is explicit and deterministic; a failed route must
not silently choose another provider or artifact kind.

## Design

### Our linker and its execution routes

```mermaid
flowchart TD
    source[Source modules] --> core[Linked and verified Core]
    core --> lowering[Backend lowering and target requirements]
    catalog[Structured runtime catalog] --> graph[Checked target provider graph]
    lowering --> graph
    graph --> direct[Compiler-owned single application emission]
    direct --> app[Verified application Core Wasm]
    app --> libraries[Checked runtime Core library composition]
    graph --> libraries
    world[Selected WIT world] --> libraries
    libraries --> component[Application component]
    component --> guests[Optional guest-component composition]
    graph --> guests
```

| Provider variant | Route |
| --- | --- |
| Core Module | Existing checked runtime-library composition |
| Component | Explicit guest interface composition |
| Permitted host interface | Residual import allowed by the selected world and target policy |

`psrs_core::link` implements language-level assembly. `psrs-linker` implements
provider resolution, contract checking, memory planning, and artifact composition.
The backend implements representation lowering and final index assignment. These
are parts of our linker/compiler architecture, not delegation to an external
object linker. Existing `wasm_encoder`, `wasmparser`, and `ComponentEncoder`
provide encoding/validation machinery under these checked contracts.

Runtime artifact production may itself use Rust/LLVM linking. The application
build consumes pinned Core module bytes and needs no runtime-source build tools.
The target linker connects Core function imports to exports through component
aliases/instances; it does not concatenate binary sections or patch call indices
inside prelinked runtime modules.

### Source assembly and emission policy

Source resolution, type checking, Core assembly, and target linking are different
operations with different identities. Core assembly preserves resolved symbols
and rebases module-local type IDs; backend passes introduce their own valid IDs.
Final Wasm indices belong to encoding and are not semantic identities.

Whole-program optimization runs on verified linked Core before backend lowering.
A module cache avoids repeated frontend work; changing application dependencies
still rebuilds linked Core and application Wasm. The design does not promise
independent native-code reuse per source module.

### How linked Core becomes Wasm

1. **Assemble Core:** preserve resolved declaration symbols and source origins;
   rebase module-local Core type/variable IDs and retain imported evidence.
   Select entry/export roots, compute reachability, and verify the linked module.
2. **Optimize Core:** simplify, inline and specialize within checked budgets.
   Reverify the result; generated declarations retain parent origins.
3. **Lower to CC:** make evaluation order and closure captures explicit, retaining
   checked calling and representation evidence across module boundaries.
4. **Lower to MIR:** choose concrete scalar/GC layouts, recursive type dependencies,
   closure signatures, explicit target calls and ABI recovery/lifetime operations.
   MIR is the lowest compiler IR; verify it before encoding.
5. **Plan target providers:** close live MIR/helper/runtime requirements and plan
   storage. All imported callable signatures come from independently checked
   consumers and verified providers. Rebuild the plan if optimization changes them.
6. **Assign final indices:** order GC recursive type groups under the backend's
   representation rules; allocate imported functions before defined functions;
   map MIR function IDs, types, globals, tables and data to final Wasm indices.
   Resolve direct calls, `ref.func` and typed `call_ref` through these maps.
   Missing entries and incompatible shapes are errors, not inferred defaults.
7. **Structure and encode:** convert MIR CFGs to structured Wasm control flow,
   encode type/import/function/table/memory/global/export/code/data sections as
   required, and validate application bytes under the selected feature profile.
   There is no relocation pass after these indices are assigned.
8. **Compose:** `psrs-linker` checks emitted bytes against the immutable plan,
   binds private runtime imports, supplies shared memory and required globals,
   resolves supported shims, and builds canonical bindings for the selected world.
   Validate the component and its exact external imports before publication.

Binary validation establishes Wasm typing, not source-level layout semantics;
backend producer evidence and IR verifiers establish the latter. Runtime Core
modules keep their own index spaces; component instance bindings connect them.

### Runtime storage and initialization

The graph collects all selected runtime reservations before emission. One
`MemoryPlan` owns each shared memory's identity, static data, stacks, canonical
scratch, allocator state, heap boundary/alignment, and minimum/maximum limits.
The linker verifies declared reservations against artifact bytes and rejects
overlap, overflow, unsupported memory profiles, and conflicting growth owners.
Pointer equality across distinct component memories does not establish ownership.

The allocator provider imports the application's memory and a checked immutable
heap-boundary value. Its state, first-call initialization, and exclusive growth
protocol are specified by the adapter topic. Other runtime units must allocate
through that provider if they need the same heap; they cannot create independent
Rust global allocators over it.

Initialization dependencies distinguish passive instantiation, shim resolution,
and calls. Current unsupported executable starts remain rejected. Lazy provider
initialization must occur only after its memory/global bindings and shims are
resolved. No initialization may invoke an unresolved dependency. Future explicit
initializers require an extended checked execution plan before use.

### Why linking happens before application encoding

Core retains language types, declarations, dictionaries and semantic evidence
needed for valid cross-module composition. Linking those inputs before GC layout
and index assignment lets one backend establish representation consistency.
Runtime Core modules instead expose explicitly checked target ABIs; the linker
connects their instances without reinterpreting language declarations.

This choice is based on ownership and output contracts. It does not rely on an
unreproduced external-tool failure or claim that no external linker exists.
Application object-file linking is not part of this design.

## Algorithms

1. Resolve/check source dependencies, lower and assemble Core, verify the linked
   result, and optimize under the selected policy.
2. Lower checked operations into target requirements with consumer ABI evidence.
3. Resolve root requirements against runtime units, recursively adding provider
   dependencies and retained initialization needs. Reject ambiguous closure.
4. Verify exact artifact bytes/features/exports and establish state ownership.
5. Plan shared storage, growth ownership, and initialization/shim ordering.
6. Emit the linked application through the compiler-owned backend and validate
   its Core Wasm bytes under the selected target profile.
7. Inspect application conformance to the same checked provider/memory plan.
8. Compose runtime libraries and explicit guest providers. Verify the residual
   external world and final component before publishing a successful artifact.

Optimization that changes live requirements invalidates the old plan. Preserve
requirement origins, input/output digests, tool versions, and phase-specific
errors through every step. Never infer a new semantic binding from emitted names.

## Code map

| Owner | Intended API/responsibility |
| --- | --- |
| Driver/compiler planner | Checked module dependencies, Core assembly, grouping/cache policy |
| Backend | Core → CC → MIR, representation evidence, CFG structuring and final Wasm emission |
| `psrs-runtime` catalog | Runtime packages/units, pinned variants, ABI/state/storage metadata |
| `psrs-linker` target planner | `plan(context, input)` resolves the provider graph and immutable execution constraints |
| Target composer | Execute checked Core library bindings and initialization protocol |
| Guest planner/composer | Explicit WIT providers and component resource/instance identity |

These are design responsibilities. Extend existing APIs where ownership already
exists; introduce no placeholder modules merely to match this table.

## Invariants and verification

| Requirement | Required evidence |
| --- | --- |
| Source boundaries | Cross-module calls, imported types and captured closures execute after Core assembly; no WIT boundary per source import |
| Checked caches | Invalid imported contracts/IDs rejected; loading preserves origins and identity; dependency changes invalidate entries |
| Runtime closure | Source root reaches transitive runtime dependency; missing/ambiguous/incompatible providers rejected |
| State/storage | Duplicate allocator owners and overlapping stack/static/heap reservations rejected; growth preserves live values |
| Initialization | Dependencies bound before first call; unsupported starts and eager cycles rejected |
| Composition | Runtime Core calls and selected guest interfaces execute; private runtime imports close and external world is exact |
| Emission | Direct calls, recursive GC types, `ref.func`, typed closure calls and index maps validate and execute across source-module boundaries |
| Artifact lineage | Link report pins source/package contracts, runtime bytes, application bytes and composed component with exact digests |

Evidence records sources, exact commands, compiler/runtime versions, feature
profile, hashes, validator and execution results, including failures. A skipped
runtime case remains unverified. Negative tests cover missing symbols/contracts,
invalid MIR references, incompatible raw ABIs and unexpected imports. No completed
runtime-unit or cache acceptance is claimed by this draft.

## Worked example

An application imports a PureScript library that formats a Number and emits a
string through WASI. Source checking/Core assembly resolves the library's
language declarations without WIT per source module. Lowering produces checked
requirements for numeric formatting, ABI buffer allocation, and the WASI call.

The provider graph selects a numeric runtime unit, the allocator unit, and the
permitted host interface. It closes runtime dependencies and plans disjoint
storage. The current emitter builds one application Core Module with private
runtime function imports. Composition supplies both Core runtime instances and
shared memory bindings. The allocator initializes lazily after bindings resolve;
the application calls formatting and cleans up buffers through the same provider.
The resulting component exposes only the selected external world.

## Boundaries and interfaces

Core assembly links language declarations. Target planning links executable
requirements. Runtime composition connects Core instances. Component composition
connects explicit WIT interfaces. They share provenance and checked constraints,
not a universal symbol/type namespace.

MIR remains the lowest compiler IR. Encoded Wasm is a target artifact. Runtime
unit ABI metadata cannot replace source typing or backend representation evidence.
WIT definitions never supply executable implementations by themselves.

## Open questions and future work

The [canonical realloc adapter](canonical-realloc-runtime-adapter.md) replaces
the generated allocator. Its pinned `dlmalloc` provider and imported immutable
heap-boundary global are not implemented. Until that unit is selected, backend
demands name `psrs:allocator/generated` as the only growth owner. Planning
already accepts a selected runtime unit when the demand names that unit, and
rejects every other selected unit that grows the same memory. Establish a persistent
checked-module format and validation contract before implementing caching.
Explicit library export roots, additional memory profiles, and executable
initialization effects each need their own checked contract. The current
implementation status must remain separate from the design above.

## References

- [Issue #144](https://github.com/biuld/purescript-rs/issues/144)
- [Linking and runtime](linking-and-runtime.md)
- [Canonical realloc runtime adapter](canonical-realloc-runtime-adapter.md)
- [Wasm encoding and structuring](encoding-and-structuring.md)
- [Core and MIR boundaries](../00-ir-boundaries.md)
