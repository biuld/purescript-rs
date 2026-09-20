# DEC-04 — Frontend and Backend Feature Matrices

**Status:** Accepted  
**Date:** 2026-09-19

## Context

The official PureScript test suite is a useful compatibility oracle, but a
single error-code ladder is not a sufficient implementation roadmap. It mixes
two different kinds of work:

- **Frontend compatibility:** PureScript source syntax, modules, name
  resolution, kinds, type checking, type classes, and diagnostics.
- **Backend capability:** CC and MIR lowering, runtime representation, Wasm
  emission, WIT bindings, WASI integration, and execution.

The current roadmap also makes a parser-only feature look implemented and can
make a backend feature look complete when it only works for one scalar slice.
The project needs a feature inventory that answers both questions separately:
what does PureScript mean, and can the resulting program run on the selected
platform?

The official suite remains pinned to the PureScript `v0.15.16` corpus. Its
passing, failing, warning, optimize, and layout cases are the acceptance
oracle, but the feature matrices below are the primary planning artifact. A
feature's implementation status and its suite acceptance status are tracked
together; the former cannot silently replace the latter.

## Decision

Maintain two feature matrices and update them whenever a feature moves through
the compiler:

1. The **frontend matrix** tracks source-language support through Typed Core.
2. The **backend matrix** tracks Typed Core through CC/MIR, Wasm, WIT, and
   WASI execution.

A feature is not marked `Implemented` merely because its syntax parses, a type
exists in an IR, or a Wasm opcode can be emitted. `Implemented` means that the
feature works end to end at the boundary named in its row, has a regression
test, and has reached its official-suite gate when the suite exercises that
feature. `Partial` is used for parser-only work, a restricted type/runtime
slice, an incomplete suite gate, or a feature whose representation exists but
is not yet connected through the whole pipeline.

Backend infrastructure that the official suite cannot observe directly—such
as a MIR verifier, a WIT registry, or WAT printing—also requires dedicated
unit/integration tests. Those tests prove the infrastructure contract; they do
not make the language or runtime suite complete. The passing-suite runtime gate
remains the final evidence for end-to-end backend compatibility.

### Status legend

| Status | Meaning |
| --- | --- |
| Implemented | Supported at the stated boundary and covered by tests. |
| Partial | A restricted slice or an earlier compiler stage is implemented. |
| Planned | No supported implementation yet; the feature remains on the roadmap. |
| Excluded | Deliberately outside the current compatibility target. |

## Official-suite progress and landing gates

The following snapshot is part of this decision and must be updated with the
feature matrices. Counts and harness rules are maintained by
[D-04](../design/D-04-suite-roadmap.md); the rows below record what they mean
for matrix status.

| Gate | Official-suite scope | Current progress | `Implemented` threshold |
| --- | --- | --- | --- |
| L0 | Layout goldens | 13/15 agreement | 15/15, with all remaining layout cases covered by regression tests. |
| L1 | Non-excluded parse behavior | 905/907 agreement using the annotations oracle; `passing` 413/413, `failing` 413/413, `warning` 66/66, `layout` 13/15 | 100% agreement for the tracked corpus. |
| L2 | Module, import, export, and name resolution | 46/70 failing cases; 35/413 passing modules resolve | The mapped resolution cases and all required passing-module cases agree. |
| L3 | Kinds and higher-kinded types | 27/48 failing cases | 100% agreement for the mapped kind cases. |
| L4 | Core type checking | The type gate is not complete | 100% agreement for the mapped type cases. |
| L5 | Classes and instances | The class gate is not complete | 100% agreement for the mapped class cases. |
| L6/M7 | Runtime and standard library | 414 non-FFI passing files are in scope; full compile/run coverage is not complete | Every in-scope passing file for the feature compiles, validates, and runs with the expected result. |
| M8-W | Warnings | 68 warning files are in scope; full warning-code coverage is not complete | Warning-code agreement reaches 100% for the tracked warning corpus. |
| M8-O | Optimization | 10 optimize files are in scope; optimize agreement is not complete | Expected optimize/CoreFn output agrees for all tracked optimize files. |

### Feature-to-gate crosswalk

This crosswalk makes the suite state part of each matrix row without repeating
the corpus counts in every row. A row with an open gate remains `Partial`, even
when its local implementation tests pass.

| Matrix rows | Required suite gate | Additional evidence |
| --- | --- | --- |
| FE-01 | L0 and L1 | Layout goldens and parser regression tests. |
| FE-02 | L2 | Resolution scoreboard and cross-module tests. |
| FE-03–FE-07 | L1–L2, then the relevant L4/L5 cases | CST/AST/HIR tests plus typed diagnostics. |
| FE-08–FE-13 | L3–L4 and the corresponding L6/M7 passing cases | Type/kind scoreboards and typed Core tests. |
| FE-14–FE-18 | L5 and the corresponding L6/M7 cases | Constraint, instance, and advanced-polymorphism tests. |
| FE-19 | Non-FFI suite cases plus WIT-specific tests | JavaScript FFI remains excluded. |
| FE-20 | L0–L5 diagnostics and M8-W | Error-code and warning-code scoreboards. |
| FE-21 | M8-O and backend Core/MIR tests | Optimize output and semantics-preservation tests. |
| BE-01–BE-11 | L6/M7 for source-visible behavior | CC/MIR lowering, representation, and execution tests. |
| BE-12 | M8-O | Core/MIR optimization and output comparison. |
| BE-13–BE-16 | L6/M7 for generated artifacts | Wasm feature-profile, validation, WAT, and runtime tests. |
| BE-17–BE-23 | L6/M7 for programs using the capability | WIT registry, canonical ABI, component, and WASI execution tests. |
| BE-24–BE-25 | No current acceptance gate | These are outside or later than the current target. |
| BE-26–BE-27 | L6/M7 | Module-loading and full passing-suite execution scoreboards. |
| BE-28 | No gate | JavaScript/Node.js FFI is excluded. |
| BC-01–BC-12 | No direct source-suite gate | Core Wasm feature tests, profile validation, WIT/component tests, and runtime compatibility tests. |

## Frontend feature matrix

The frontend boundary is Typed Core. Rows are intentionally phrased as
PureScript language features rather than crate or pass names. A syntax feature
that is currently accepted only by CST/AST is therefore `Partial` until it is
resolved, type checked, and represented in Typed Core as required.

| ID | Feature | Current support | Status | Next landing |
| --- | --- | --- | --- | --- |
| FE-01 | Lexing, Unicode tokens, comments, literals, and layout | Lexer and layout processor work; the official layout goldens still have open cases. | Partial | Close the remaining layout goldens and lock token behavior. |
| FE-02 | Module headers, imports, exports, qualified names, aliases, and hiding | Module graph, stable module IDs, value/type/constructor/class imports and exports, aliases, and cycles work in a subset. | Partial | Complete operator/fixity aliases and all transitive export rules. |
| FE-03 | Value declarations, signatures, recursive groups, pattern bindings, and `where` | Named declarations, signatures, recursive local groups, and top-level SCC inference work; pattern declarations and `where` are not end-to-end. | Partial | Lower pattern declarations and local `where` blocks. |
| FE-04 | Declaration forms: `data`, `newtype`, `type`, `class`, `instance`, `derive`, `foreign`, roles, fixities, and kind signatures | Most forms parse and several enter AST/HIR; only data, newtype, type, and part of class/kind handling are connected to checking. | Partial | Add semantic checking and resolution for each declaration form. |
| FE-05 | Expressions: application, operators, lambdas, `if`, `let`, `case`, records, arrays, literals, sections, `do`, and `ado` | Application, operators in the bootstrap subset, lambdas, `if`, `let`, `case`, scalar arrays, records, and selected literals work; sections, `do`/`ado`, and several literal forms remain open. | Partial | Land desugaring and typing for `do`/`ado`, sections, and the remaining literals. |
| FE-06 | Patterns: variables, wildcards, constructors, records, literals, tuples, arrays, guards, and binders | Variable, wildcard, constructor, and restricted closed-record patterns work; guards, multiple scrutinees, literal/tuple/array patterns, exhaustiveness, and redundancy checks remain open. | Partial | Complete pattern typing, coverage checking, and lowering. |
| FE-07 | Operators, sections, fixity declarations, and type/value operators | Operator syntax and the current intrinsic operators work; complete fixity resolution, aliases, sections, and type operators are pending. | Partial | Implement one shared fixity and operator-resolution pass. |
| FE-08 | Primitive types and monomorphic inference | `Int`, `Number` (IEEE-754 binary64), `Boolean`, `Char` (Unicode scalar as `i32`), `String`, `Unit`, function types, unification, occurs check, and source-spanned primitive errors work in the compiler slice. | Partial | Reach the complete L4/L6 gate and add official-suite evidence for the expanded primitive set and remaining literal semantics. |
| FE-09 | Rank-1 polymorphism, generalization, instantiation, signatures, `forall`, and scoped variables | Local and top-level generalization, instantiation, rigid signature variables, outermost `forall`, and the first generic CC/Wasm representation work through THIR/Core. | Partial | Reach the corresponding type/runtime suite gate, then add dictionary passing and the remaining generic representations. |
| FE-10 | Type constructors, type application, type synonyms, and saturation | Constructor/application types, built-in and user constructors, and synonym substitution work in a restricted set. | Partial | Complete constructor environments, arity rules, recursive synonyms, and backend-independent acceptance. |
| FE-11 | Kinds, kind signatures, higher-kinded types, kind annotations, and kind variables | Dedicated kind inference/checking covers several declarations, annotations, records/rows, and official kind errors. | Partial | Complete cross-module environments, rows in kinds, and expression-level cases. |
| FE-12 | Algebraic data types, constructors, newtypes, and constructor typing | Data/newtype declarations, constructor schemes, constructor application, and basic case typing work. | Partial | Add full recursive/parameterized checking, exhaustiveness, and all pattern forms. |
| FE-13 | Records, row types, row polymorphism, and variants | Closed concrete records, field access/update, and restricted record patterns work; open rows and row-polymorphic inference do not. | Partial | Implement row unification, open records, and variants. |
| FE-14 | Constraints, type classes, superclasses, class members, and instances | Class and instance syntax is represented, but constraint solving and dictionary evidence are not implemented. | Planned | Add class environments, constraint schemes, and instance resolution. |
| FE-15 | Functional dependencies | Functional-dependency syntax is represented; improvement and consistency checking are not implemented. | Partial | Add dependency improvement and the associated diagnostics. |
| FE-16 | Deriving, roles, `Coercible`, and newtype-based derivation | Syntax is partially represented; deriving, role checking, coercions, and generated evidence are not supported. | Partial | Implement roles/coercions first, then deriving and generated instances. |
| FE-17 | Visible type application, typed binders, type wildcards, holes, and advanced annotations | Some type syntax and kinded binders parse; visible application, holes, and full annotation checking remain incomplete. | Partial | Add explicit type-application elaboration and hole/wildcard diagnostics. |
| FE-18 | Higher-rank types, subsumption, impredicativity, and higher-rank `forall` | Not implemented; the current checker is rank-1. | Planned | Add a separate higher-rank checking phase after classes and rows. |
| FE-19 | Foreign declarations and target-aware external names | Source-declared WIT bindings are resolved for the supported backend path; JavaScript FFI and foreign data are not general frontend targets. | Partial | Define the complete target-aware foreign declaration rules. |
| FE-20 | Warnings, holes, source spans, and official diagnostic codes | Source spans and several error-code mappings exist; warning coverage and complete diagnostic agreement do not. | Partial | Track warning-code agreement separately from acceptance errors. |
| FE-21 | Typed Core normalization and CoreFn/optimization compatibility | Typed Core lowering and verification work for the supported subset; official optimize output is not yet a target. | Partial | Add Core optimization passes and an explicit optimize compatibility track. |

The frontend landing order is:

```text
FE-01 -> FE-02..FE-07 -> FE-08..FE-13 -> FE-14..FE-17 -> FE-18..FE-21
```

This is a dependency guide, not a requirement to finish every row in a block
before starting the next one. Each row must move from syntax/representation to
typed, source-spanned behavior before it is considered landed.

## Backend feature matrix

The backend starts from Typed Core. CC and MIR are the backend IR family;
Wasm is the target encoding, and WIT/WASI are the platform integration layers.

| ID | Feature | Current support | Status | Next landing |
| --- | --- | --- | --- | --- |
| BE-01 | ANF and explicit evaluation order | Direct-style CC/ANF lowering is implemented and tested for the bootstrap expression set. | Partial | Extend the lowering to every frontend expression form and pass the L6/M7 gate. |
| BE-02 | Closure conversion, captures, direct calls, and closure calls | Top-level functions, local lambdas, scalar and concrete aggregate captures, direct rank-1 generic calls, and explicit adapters between concrete and erased higher-order closures work. | Partial | Cover generic captures, partial applications, and the official runtime gate. |
| BE-03 | MIR/CFG, block parameters, terminators, and verification | Typed basic blocks, explicit instructions/terminators, runtime layouts, and MIR verification work. | Partial | Grow the instruction set with the remaining language/runtime constructs and pass the L6/M7 gate. |
| BE-04 | Primitive runtime representation and calling conventions | `Int`, `Number` as Wasm `f64`, `Boolean`, `Char` as an integer-valued scalar, `String`, `Unit`, direct calls, and the current closure ABI work in compiler/runtime tests. | Partial | Add official-suite evidence, richer values, and stable ABI coverage before changing the status. |
| BE-05 | Nullary ADT tags and case lowering | Nullary constructors lower to integer tags and execute under WASI. | Partial | Integrate with the complete pattern and exhaustiveness model and pass the L6/M7 gate. |
| BE-06 | Field-bearing ADTs and constructor-pattern lowering | Non-parameterized constructors use Wasm GC structs; nested constructor patterns work in a restricted form. | Partial | Complete recursive, polymorphic, and mixed-field layouts. |
| BE-07 | Newtype erasure | Single-field newtype construction and matching erase without allocation. | Partial | Connect erasure to coercions, roles, derived instances, and the relevant L6/M7 cases. |
| BE-08 | Parameterized ADT representation and erasure | Parameter-dependent `Int`/`Boolean`/`Number` fields use typed GC boxes and recover references through `eqref`; direct generic calls and generic higher-order adapters use the same erased protocol. | Partial | Generalize erased layouts to generic records/arrays and verify all instantiations. |
| BE-09 | Records and row values | Closed concrete records, field reads, updates, and restricted patterns use GC structs. | Partial | Add open rows, polymorphic records, variants, and generic field operations. |
| BE-10 | Arrays and aggregate values | Concrete scalar and aggregate arrays support literals, length, indexing, and updates through Wasm GC arrays, including nested arrays, records, ADTs, and `Number`. | Partial | Support polymorphic element representations and the official runtime gate. |
| BE-11 | Strings, linear memory, data segments, and allocation | String literals use length-prefixed UTF-8 data; a bump `cabi_realloc` supports returned byte lists/strings, including passing returned strings into another WIT import and repeated allocations. | Partial | Stabilize allocator ownership and returned aggregate handling. |
| BE-12 | Core optimization and MIR optimization | Optimization is not yet a compatibility target. | Planned | Add semantics-preserving passes after the unoptimized path is complete. |
| BE-13 | Structured Wasm encoding and binary emission | Thin structured control-flow encoding delegates leaf instructions to `wasm-encoder`. | Partial | Cover the remaining MIR instruction and control-flow forms and pass the L6/M7 gate. |
| BE-14 | Wasm validation and WAT output | Generated core modules are validated with `wasmparser` and printed with `wasmprinter`. | Partial | Make feature-profile validation part of every backend acceptance test and pass the L6/M7 gate. |
| BE-15 | Wasm GC, reference types, typed function references, and `call_ref` | GC/reference operations and `call_ref` are emitted for the current closure and aggregate slice; the profile gate now rejects disabled targets. | Partial | Add per-operation validation and execution coverage for the complete selected subset. |
| BE-16 | Wasm feature profile and pinned runtime | `TargetCapabilities` defines the stable profile and `wasmparser` validates from the same explicit flags; optional Wasmtime proposals are disabled by default. | Partial | Add fallback lowerings or keep each optional capability explicitly out of the target. |
| BE-17 | WIT vendoring, parsing, name resolution, and canonical signatures | Vendored WASI WIT is loaded into a registry and resolves interfaces, functions, resources, lists, and results. | Partial | Expand the accepted source and result type mapping and pass the L6/M7 capability gate. |
| BE-18 | Generic source-declared WIT imports | Compatible `Int`/`Boolean`/`Number` scalars, handles, and byte-list/string imports lower through the canonical ABI with signature validation. | Partial | Add aggregate WIT values, richer results, and user-library loading. |
| BE-19 | WIT aggregate values and resources | Resource handles and byte lists have a bootstrap path; lists of strings, tuples, and general aggregates are rejected. | Partial | Add aggregate layouts and ownership/lifetime rules. |
| BE-20 | Component Model packaging and capability-based imports | `wit-component` lifts the core module to a WASI 0.2 component and prunes unused imports. | Partial | Add component import/export regression cases beyond the CLI path and pass the L6/M7 gate. |
| BE-21 | WASI CLI entry, exit, stdout, and stderr | `wasi:cli/run`, exit codes, console output, and error output work in the component path. | Partial | Exercise the interfaces through source standard-library modules and pass the L6/M7 gate. |
| BE-22 | WASI clocks and randomness | Monotonic time and random bytes are wired through WASI and tested. | Partial | Expose the remaining clock/random library surface and pass the L6/M7 gate. |
| BE-23 | WASI arguments, environment, and filesystem | WIT descriptions are vendored, but the source library and aggregate lowering are not complete. | Planned | Add module loading and aggregate/list support, then expose these services. |
| BE-24 | WASI sockets and HTTP | Not part of the current synchronous portable-program target. | Excluded | Revisit as a separate platform scope after the core target is stable. |
| BE-25 | WASI 0.3 async streams and futures | The current compiler targets synchronous WASI 0.2. | Planned | Revisit only with an explicit platform decision and async language/library plan. |
| BE-26 | Embedded standard library and user module loading | The PureScript-facing library is embedded and linked like source modules; a filesystem module loader is missing. | Partial | Replace embedding with discoverable library/module loading. |
| BE-27 | Wasm/WASI execution and official passing-suite runtime coverage | Vertical execution tests pass for the bootstrap slice; full official passing-suite execution is not complete. | Partial | Track per-feature runtime cases and then expand the passing-suite scoreboard. |
| BE-28 | JavaScript/Node.js FFI compatibility | Not emitted or executed by this backend. | Excluded | No work planned under this decision. |

The backend landing order is:

```text
BE-01..BE-04 -> BE-05..BE-11 -> BE-13..BE-16 -> BE-17..BE-23 -> BE-12, BE-25..BE-27
```

The backend capability checklist is tracked at a smaller granularity than the
language-facing rows above. `BC` rows describe the target contract and must not
be read as claims that every opcode in an enabled proposal is already emitted.

## Backend target capability matrix

| ID | Capability slice | Current support | Status | Next landing |
| --- | --- | --- | --- | --- |
| BC-01 | Wasm MVP values, function types, locals, imports/exports, memory, code, and data sections | The encoder covers the sections needed by the current component path; tables, globals, start, passive segments, and custom sections are not modeled. | Partial | Add explicit module-section records and section-level encode/validate tests. |
| BC-02 | MVP calls, structured control, locals, numeric operations, and memory operations | Direct calls, `if`, integer/floating scalar locals, arithmetic, comparisons, `i32` loads/stores, `memory.size`, and `memory.grow` are emitted; block/loop/br-table and the full numeric/memory families are not. | Partial | Split control-flow and opcode coverage into independently tested lowering slices. |
| BC-03 | Linear memory and data-segment ABI | Active data segments and the byte-oriented WASI allocator work with one wasm32 memory. | Partial | Add passive segments/bulk operations and make pointer width a target choice. |
| BC-04 | Tier-1 scalar proposals: mutable globals, sign extension, saturating float-to-int, and extended const | The target profile exposes these capabilities, but the MIR/emitter does not yet have dedicated nodes or end-to-end tests for all of them. | Partial | Add explicit MIR operations, constant folding, and validator tests. |
| BC-05 | Multi-value function/block signatures | The thin encoder can carry multiple function results, but MIR functions and structured regions currently have one result. | Partial | Extend MIR signatures, block parameters/results, stack typing, and tuple lowering. |
| BC-06 | Bulk memory and passive element/data segments | Not emitted by the current lowering. | Planned | Add passive segment ownership and `memory.init/copy/fill` lowering. |
| BC-07 | Reference types, typed function references, and Wasm GC | GC structs/arrays, nullable references, casts, `i31`, `ref.func`, and `call_ref` support the current closure/aggregate representation. | Partial | Complete subtype validation, recursive groups, and per-instruction Wasmtime tests. |
| BC-08 | SIMD, relaxed SIMD, tail calls, exceptions, multi-memory, memory64, and wide arithmetic | Runtime-supported proposal families are explicitly disabled in the stable profile and have no lowering. | Planned | Adopt each independently with a flag, fallback or rejection behavior, and execution evidence. |
| BC-09 | Threads, shared memory, and stack switching | Not part of the single-threaded runtime or language ABI. | Planned | Design a concurrency/effect model before enabling Core or WASI threading. |
| BC-10 | Component Model MVP, WIT, canonical lift/lower, resources, and realloc/post-return | WASI 0.2 command componentization, WIT registry lookup, scalar/handle/byte-list mappings, and `cabi_realloc` work in a restricted slice. | Partial | Add records, tuples, variants, option/result, resource lifetime, and component round trips. |
| BC-11 | WASI version targets | WASI 0.2 Component Model is the stable target; Preview 1 is excluded and WASI 0.3 async is deferred. | Partial | Keep version selection in target capabilities and add an explicit Preview 1 compatibility target only if needed. |
| BC-12 | WASI service capabilities | CLI exit/stdout/stderr, monotonic clocks, and random imports are wired; filesystem, sockets, HTTP, TLS, and async streams are not. | Partial | Add one capability/interface family at a time with source library, canonical ABI, sandbox, and runtime tests. |

### CC/MIR stage crosswalk

The BC rows above are capability families. This crosswalk is the smaller
implementation unit used when deciding whether a row can move from `Partial`
to `Implemented`.

| Stage | Owns today | Does not own yet | Gate for a capability claim |
| --- | --- | --- | --- |
| CC | Evaluation order, symbolic representation/signature handles, closure captures, direct versus indirect calls, current erased-value adapters, and expression-level `if`. | General control flow, multi-result values, and complete operation-level shape verification. | Every operation has a type/shape verifier and remains independent of the selected P9 planner. |
| MIR | Typed CFG, P9 concrete layout planning, block parameters for current merge diamonds, canonical import calls, GC/reference operations, and the current wasm32 memory boundary. | Loops/multi-way branches, multi-value signatures, tables/globals, bulk memory, and proposal-specific instructions. | The verifier checks dominance, exact call/reference signatures, aggregate reference compatibility, and the lowering has binary plus execution evidence. |
| WIT/ABI lowering | WASI WIT lookup and the scalar/handle/byte-list canonical ABI subset. | General records, variants, options, results, resources, ownership, and version-polymorphic ABI. | Source signature validation, canonical lift/lower, component metadata, and runtime tests agree for the selected service family. |

The former CC type-table pass-through has been removed: P9's layout planner
constructs the concrete MIR `RecGroup` and resolves every abstract handle. The
`BE-03`, `BE-15`, and `BC-07` rows remain `Partial` until CC verification is
complete and a second representation planner consumes the same CC module.

The capability landing order is:

```text
BC-01..BC-03 -> BC-04..BC-07 -> BC-10..BC-12 -> BC-08..BC-09
```

This matrix is intentionally aligned with the external Wasmtime checklist:
Core Wasm proposals, Component Model gates, WASI version strategy, and WASI
service families are separate rows. A Wasmtime stability tier is evidence about
the host runtime, not a compiler implementation status.

The frontend and backend are developed in parallel, but a runtime feature is
not counted as an official passing case until the frontend can produce the
required Typed Core and the backend can validate and execute the resulting
artifact.

## Official suite and scoreboard

The suite is the acceptance oracle, not the feature inventory. The scoreboard
reports frontend and backend progress separately:

| Track | Measures | Primary evidence |
| --- | --- | --- |
| Frontend | Layout, parse, resolve, kinds, type, class, warning, and diagnostic agreement with `purs`. | `layout`, `passing`, `failing`, and `warning` cases classified by official `errorCode`. |
| Backend | CC/MIR verifier results, Wasm validation, WIT signature agreement, component construction, and observable execution. | Backend unit tests, WAT/validation tests, WIT ABI tests, WASI runtime tests, and the executable subset of `passing`. |
| End to end | A source feature plus its runtime representation behaves like the oracle. | Per-file compile/run agreement for supported non-FFI cases. |

The existing error-code layers remain useful as scoreboard dimensions:

```text
L0 layout -> L1 parse -> L2 resolve -> L3 kinds -> L4 types -> L5 classes -> L6 runtime
```

They no longer define the implementation roadmap by themselves. A feature row
in the matrices links to the relevant suite cases and may contribute to more
than one layer.

The pinned corpus currently contains layout goldens, passing programs, failing
programs with expected `errorCode`s, warning cases, and optimize cases. Exact
counts and harness behavior belong in [D-04](../design/D-04-suite-roadmap.md),
which is the operational design for classification and scoreboards.

### Exclusions

JavaScript and Node.js FFI are outside the current target. Suite files that
declare JavaScript `foreign import`s, ship a JavaScript FFI implementation, or
expect an FFI-specific diagnostic are reported separately and count as neither
agreement nor gaps. A compatible source-declared WIT import and programs using
the project's PureScript-facing WASI libraries remain in scope.

The default workspace test suite must remain self-contained. Tests requiring
`purs`, a PureScript checkout, or a WASI runtime are opt-in or skip when the
external dependency is unavailable.

## Consequences

- Frontend work is planned like a language-compatibility project: each syntax,
  type-system, and diagnostic feature is landed and tracked independently.
- Backend work is planned like a target-integration project: CC/MIR invariants,
  Wasm capabilities, WIT canonical ABI, and WASI services have separate
  acceptance evidence.
- The same PureScript feature can appear in both matrices. For example,
  parameterized ADTs need frontend type checking and backend erased layouts;
  neither side can claim the feature alone is complete.
- D-03 remains the source of truth for type representations and type-system
  sequencing. D-02, D-05, D-06, and D-07 remain the source of truth for backend,
  Wasm, WIT, and WASI implementation details.
- Official `errorCode` agreement remains mandatory for frontend diagnostics,
  but message text and golden `.out` formatting are not compatibility criteria.
- The matrices make partial support explicit and prevent a parser-only feature,
  a single backend vertical slice, or a runtime-only test from being mistaken
  for full language support.
