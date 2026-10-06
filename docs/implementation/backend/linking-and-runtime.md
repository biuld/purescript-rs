# Linking and Runtime Acceptance

**Design:** [Linking and Runtime](../../design/backend/wasm/linking-and-runtime.md).
**Decision:** [DEC-18 (Accepted)](../../decision/DEC-18-unified-target-linking.md).
**Status:** LK-01 through LK-15 verified for the supported stable synchronous
target contract, including source-build guest composition and locked Show.

**Recorded:** 2026-10-07; implementation evidence recorded 2026-10-07.

This is the acceptance contract for unified target linking. The core plan carries
requirements through artifact verification, Wasm emission and core component
assembly. The checked guest graph composes that produced component, preserving
its artifact lineage and the exact final host capability set.

`Verified` means the complete required evidence is established for the supported
stable target contract. Future formats or target proposals remain explicit
design extensions; this status does not grant support to rejected inputs.

| ID | Requirement | Required evidence | Status |
| --- | --- | --- | --- |
| LK-01 | A checked language intrinsic selects one catalog implementation; raw ABI does not replace its scheme | Accepted use plus malformed source/Core operand/result rejection; registry and implementation identities agree | Verified |
| LK-02 | Direct operations, generated helpers, artifact exports, and entry-generated imports participate in one requirement closure | Composed source/MIR examples, missing-provider rejection, unused implementation omission, initializer reachability | Verified |
| LK-03 | Artifact signatures, digests, features, imports, exports, tables, and initialization match declared contracts | Valid artifact plus independently malformed signature, stale digest, unexpected import/feature/table/start and missing-export rejection | Verified |
| LK-04 | One immutable checked plan drives emission and component assembly | API and trace evidence; deliberately mismatched emitted imports/memory rejected; no late import-name provider selection | Verified |
| LK-05 | Shared memory reservations and allocator boundaries cannot overlap or overflow | Malformed ranges/bounds rejected; runtime formatting interleaved with WASI allocation, output and memory growth | Verified |
| LK-06 | Formatter output is recovered into a GC String before temporary bytes are released | Retain earlier strings across many later formats and WASI calls, asserting exact contents; bounded buffer reuse evidence | Verified |
| LK-07 | Runtime stack use fits its declared region under supported calling behavior | Recorded static bound with stress evidence; unsupported reentrancy/initialization explicitly rejected | Verified |
| LK-08 | Instantiation closes function/memory dependencies before execution | Run a legitimate shim cycle; reject unresolved/eager initializer cycles; inspect that private runtime imports are absent from the final external world | Verified |
| LK-09 | WIT definition resolution and executable provider selection remain separate | A definition-only package does not satisfy a live import; pinned host/guest binding selection, version/provider conflicts and missing exports tested | Verified |
| LK-10 | Guest WIT composition preserves provider memory, canonical ownership and resource identity | Cross-component string/list/result execution, post-return/free behavior, resource constructor/method/destructor coherence; incompatible providers rejected | Verified |
| LK-11 | The external world contains exactly permitted residual host capabilities | Used/unused service tests, guest transitive host dependency tests, disallowed capability and silent-fallback rejection | Verified |
| LK-12 | Runtime artifact regeneration and compile lineage are reproducible | Pinned source/dependency/toolchain/recipe manifest, artifact hashes, repeat-build agreement, plan inputs and selected providers recorded in diagnosis | Verified |
| LK-13 | Public Show executes with official semantics and preserved pure source/API | Pinned official source audit and JS oracle for Int, Number, Char, String, arrays and callback order; mandatory Wasmtime with byte-exact outputs | Verified |
| LK-14 | Runtime owns one WIT/artifact catalog; linker owns definition resolution; backend owns source/ABI validation | Move pinned WIT/default-world assets without byte drift; ABI lookup and composition use one resolved context; reject pin drift; no compiler dependencies or formatter code in metadata-only consumption | Verified |
| LK-15 | Independent linker crate consumes target records without compiler IR dependencies | Dependency-graph audit; target-only plan/composition tests; backend request conversion and source-diagnostic attribution; no MIR/backend/HIR/Core types in linker APIs | Verified |

## Public Show behavior cases

The library-owned oracle must cover signed-i32 extrema; negative zero; finite,
NaN and infinite Number values; notation boundaries and neighboring values;
subnormal/minimum/maximum binary64 values; shortest-decimal tie choices; named
and decimal control escapes; decimal-escape termination before digits; quote and
backslash escaping; Unicode scalars/UTF-8; empty and nested arrays; and ordinary
callback order/multiplicity. Include mixed use with console output and retained
strings so a correct token alone cannot hide an incorrect memory boundary.

Use `psrs-stdlib/conformance/` and its public executable-based runner for official
JS comparisons. Compiler regression tests additionally cover malformed Core,
MIR, raw artifacts, plans, and component contracts. Source import acceptance,
Wasm validation, execution, and official oracle agreement are separate evidence.

## Evidence

- `psrs-linker` target-only unit and integration tests: artifact contract
  verification and the measured stack bound (`verify::tests`, `stack::tests`),
  definition resolution (`definitions::tests`), provider closure, target-policy
  gating, version-pin drift and memory planning (`tests/plan.rs`), and
  composition (`tests/compose.rs`). The composition boundary rejects different
  WIT contexts, actual signature/memory drift, unowned data and incorrect
  allocator initialization. `closure.rs` projects actual WIT call signatures
  through the same encoder used by final composition: `get-stdout` retains its
  `streams` resource owner, without expanding unused `error`/`poll` methods.
- `psrs-linker` `tests/guest/` validates pinned whole-interface binding graphs,
  transitive guest/host closure, unused candidates, conflicts, missing exports,
  incompatible function shapes, definition-only packages, dependency cycles and
  target feature restrictions. Mandatory Wasmtime execution passes UTF-8 strings
  twice across distinct memories, checks earlier values after provider
  post-return clears its buffer, verifies a byte-list result and three
  post-return calls, then checks resource constructor/method/destructor coherence.
- `psrs-cli` `tests/link.rs` verifies manifest-relative provider paths, selected
  pins, exact host closure and report/output digest agreement; stale bytes and
  WASI interfaces outside the target profile fail before files are emitted.
  [Explicit Component Linking](../../workflow/component-linking.md) specifies
  this entry point. Source build reports join diagnosis and graph lineage.
- `psrs-runtime` formatter token tests, catalog digest, and
  `tools/check-reproducible.sh`, which rebuilds the artifact and requires it to
  reproduce the committed bytes.
- `psrs-backend` `mir::number_format_tests::formats_a_number_through_the_runtime_artifact`
  executes the formatter artifact through the checked plan under Wasmtime and
  asserts the initialized token length through the command exit code.
- `psrs-driver` `tests::show::show_renders_the_instances_the_corpus_prints`
  executes `Data.Show` under Wasmtime and asserts byte-exact stdout across Int,
  Number (including `1e+21` and `1e-5`), Char, String escapes, unit, booleans,
  and arrays, interleaving formatting with WASI output and retained strings.
  `tests::show::show_covers_number_and_aggregate_boundaries` adds NaN,
  ±Infinity, negative zero, the `1e-6`/`1e-7` and `1e20` notation boundaries,
  the minimum subnormal, the empty array, and a nested array.
  `tests::show::formats_many_numbers_without_exhausting_the_runtime_stack`
  formats a 64-element Number array through the same private stack region.
  Memory growth is not exercised; the application allocator is pre-sized by the
  plan.
- `psrs-driver` `tests::diagnosis_trace::target_plan_records_provider_and_memory_lineage`
  and `tests::show::formatter_plan_records_the_pinned_artifact_digest` assert the
  compile diagnosis records the selected providers, artifact digests, memory
  boundary, stack bounds, and planned external world.
- Validation for this continuation uses focused linker, CLI, backend formatter
  and source-level Show/diagnosis tests, required Wasmtime execution, formatting,
  workspace Clippy and byte-for-byte runtime regeneration. Full workspace tests
  are omitted under the user's explicit validation scope; this record does not
  claim a new workspace-wide acceptance measurement.

### Continuation validation

Measured with mandatory Wasmtime execution and `PSRS_STDLIB_ROOT` pointing to
the development stdlib checkout where source-level tests require it:

| Command or focused filter | Result |
| --- | --- |
| `cargo test -p psrs-linker` | 42 passed: 15 unit, 6 core composition, 9 guest composition, 12 plan |
| `cargo test -p psrs-cli --test link --test source_layout` | 2 passed |
| `cargo test -p psrs-backend --lib mir::number_format_tests` | 1 passed |
| `cargo test -p psrs-backend --lib mir::gc_tests` | 22 passed |
| `cargo test -p psrs-driver --lib tests::show::` | 5 passed |
| `cargo test -p psrs-driver --lib tests::wasi::wrappers::` | 19 passed |
| `cargo test -p psrs-driver --lib tests::diagnosis_trace::target_plan_records_provider_and_memory_lineage` | 1 passed |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `sh crates/psrs-runtime/tools/check-reproducible.sh` | Committed artifact reproduced byte-for-byte |

The guest execution test requires Wasmtime unconditionally; backend and driver
execution runs set `PSRS_REQUIRE_WASMTIME=1`. The full workspace test suite and
new corpus scoreboard measurements are outside this continuation's validation
scope. No new source syntax or official source diagnostic is introduced.

## Supported-contract closure

The acceptance boundary is the stable synchronous target stated in the design:
checked direct primitives and generated helpers, the complete pinned core
runtime catalog, explicit whole-interface guest providers, and exact residual
host capabilities. Unsupported relocatable/dynamic loading, async/WASI 0.3,
partial-method bindings, version adapters, reentrant runtime libraries and eager
provider initialization remain explicit errors or future design extensions.
They are not silently accepted and are not implemented by this verification.
The normative design's general invariants remain unchanged.

### Requirement audit

- LK-01/02: language schemes remain HIR/Core-owned. Raw artifact signatures
  cannot establish a Number-to-String use. The runtime implementation descriptor
  is shared by intrinsic lowering and requirement selection. Optimized command
  closure starts at the entry, retaining direct calls, tail calls, function
  references and closure construction; modules without an entry retain all
  definitions. Generated codecs/allocators and command exit remain explicit
  roots. Unused functions no longer keep foreign service imports alive.
- LK-03/04/08: encoded application imports, memory and active initialization
  are independently checked against the plan. Even a scalar-only application
  emits the plan's minimum memory. Source and synthesized command exit reuse
  one checked import; execution distinguishes stored/forced actions by exit
  codes 0/99. Core artifact eager starts and guest
  executable starts are rejected; typed component instantiation closes guest
  dependencies before command invocation. Backend fixtures use their real
  explicit requirements instead of an empty-plan composition bypass.
- LK-05/06/07: the allocator checks block capacity overflow and grows memory
  before writing fresh metadata. Its Wasmtime fixture ends one allocation at
  exactly 65536, allocates beyond that boundary, observes two pages, checks old
  bytes and subsequent reuse, and rejects wrapped free-list sizes. A source
  program retains `show 1e21`, performs a 70,000-byte WASI random allocation,
  formats the minimum subnormal, and prints/checks the retained String again.
  Growth preserves reserved addresses and the fixed runtime stack region;
  measured stack analysis and byte-reproducibility remain mandatory.
- LK-09/10/11: executable guest selection is explicit and pinned, never inferred
  from definition packages or raw-signature coincidence. Shared validator type
  identity and typed graph connections preserve canonical ownership and
  resources. Source `WASI.Random.insecureSeed` is bound to a guest returning
  `(5,37)` and executes to exit 42; its unused random services disappear from
  the encoded outer world. Transitive host/guest, conflicts, type/feature/version
  rejection, post-return and destructor evidence remains in linker tests.
- LK-12/14/15: `build --manifest --report` records source/pass/artifact lineage,
  the exact produced component artifact/digest, manifest digest, checked graph
  pins/edges/profile, residual host imports and final digest. Rejected guest plans retain the source lineage and diagnostic
  without a successful output digest. Standalone `link`
  still requires the root pin. Runtime owns the immutable WIT/artifact catalog;
  the independent linker consumes target-only records, with no compiler IR
  dependencies. Both input and output byte identities join the two plan stages. The final
  dependency metadata audit confirms that linker/runtime depend on no
  HIR/Core/backend/driver crates.
- LK-13: the library-owned oracle verifies the exact five typed foreign-slot
  delegates against clean pinned Prelude source, preserving every other source
  declaration. The official JS FFI supplies 333 observations: 8 Int, 174 Number
  (Show plus raw token), 41 Char, 105 String and 5 array cases. Actual Wasmtime
  execution returns 42 with exact callback stdout `3\n1\n2\n` and empty stderr.
  Evidence is in `psrs-stdlib/docs/evidence/show/`; its reproduction contract is
  `psrs-stdlib/docs/show.md`. The compiler lock pins that completed library
  revision and content fingerprint; this does not promote the entire stdlib.

### Final validation

All execution commands below ran with `PSRS_REQUIRE_WASMTIME=1` where applicable.
Driver/CLI final runs use the locked package, without `PSRS_STDLIB_ROOT`.
The library oracle additionally uses its explicit public development-package
runner; a separate lock-checked build runs the same 333 checks.

| Command | Result |
| --- | --- |
| `cargo test -p psrs-backend` | 400 passed |
| `cargo test -p psrs-linker -p psrs-runtime --features psrs-runtime/formatter` | 43 linker and 1 runtime test passed |
| `cargo test -p psrs-cli --test link --test source_layout` | 3 passed |
| `cargo test -p psrs-driver --lib tests::wasi::` | 146 passed |
| `cargo test -p psrs-driver --lib tests::show::` | 6 passed |
| `cargo test -p psrs-driver --lib loads_the_standard_library_from_disk_in_trusted_order` | 1 passed |
| Library `conformance/show.mjs` and public `run` | 333 official checks accepted, exit 42, exact stdout and empty stderr |
| Lock-checked `cargo run -- build Target.purs Main.purs` and Wasmtime | 333 checks, exit 42, exact stdout and empty stderr |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `sh crates/psrs-runtime/tools/check-reproducible.sh` | Byte-for-byte agreement |

[Locked Show evidence](linking-evidence/locked-show.json) records package pins,
compiler and Wasm digests, and execution observations. Source/API evidence and
the library-owned oracle are committed with the pinned library revision. The
full workspace suite and new corpus scoreboards were not run under the explicit
validation scope; no source syntax or diagnostic behavior changed.
