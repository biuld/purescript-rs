# Linking and Runtime Acceptance

**Design:** [Linking and Runtime](../../design/backend/wasm/linking-and-runtime.md).  
**Decision:** [DEC-18 (Accepted)](../../decision/DEC-18-unified-target-linking.md).  
**Status:** Implemented for the core-Wasm formatter slice; guest WIT provider
composition remains unsupported.  
**Recorded:** 2026-10-07; implementation evidence recorded 2026-10-07.

This is the acceptance contract for unified target linking. The formatter slice
now carries one checked plan from requirement closure through artifact
verification, Wasm emission, and component assembly. Requirements marked
`Partial` have a recorded assumption or a narrower evidence scope; requirements
marked `Unverified` are not implemented.

| ID | Requirement | Required evidence | Status |
| --- | --- | --- | --- |
| LK-01 | A checked language intrinsic selects one catalog implementation; raw ABI does not replace its scheme | Accepted use plus malformed source/Core operand/result rejection; registry and implementation identities agree | Verified (formatter) |
| LK-02 | Direct operations, generated helpers, artifact exports, and entry-generated imports participate in one requirement closure | Composed source/MIR examples, missing-provider rejection, unused implementation omission, initializer reachability | Verified (formatter) |
| LK-03 | Artifact signatures, digests, features, imports, exports, tables, and initialization match declared contracts | Valid artifact plus independently malformed signature, stale digest, unexpected import/feature/table/start and missing-export rejection | Verified |
| LK-04 | One immutable checked plan drives emission and component assembly | API and trace evidence; deliberately mismatched emitted imports/memory rejected; no late import-name provider selection | Verified |
| LK-05 | Shared memory reservations and allocator boundaries cannot overlap or overflow | Malformed ranges/bounds rejected; runtime formatting interleaved with WASI allocation, output and memory growth | Verified (formatter); memory growth not exercised |
| LK-06 | Formatter output is recovered into a GC String before temporary bytes are released | Retain earlier strings across many later formats and WASI calls, asserting exact contents; bounded buffer reuse evidence | Verified (formatter) |
| LK-07 | Runtime stack use fits its declared region under supported calling behavior | Recorded static bound or reviewed pinned-build assumption with stress evidence; unsupported reentrancy/initialization explicitly rejected | Partial: reviewed bound and sequential stress recorded; reentrancy not rejected |
| LK-08 | Instantiation closes function/memory dependencies before execution | Run a legitimate shim cycle; reject unresolved/eager initializer cycles; inspect that private runtime imports are absent from the final external world | Verified (formatter) |
| LK-09 | WIT definition resolution and executable provider selection remain separate | A definition-only package does not satisfy a live import; pinned host/guest binding selection, version/provider conflicts and missing exports tested | Unverified: guest providers unsupported |
| LK-10 | Guest WIT composition preserves provider memory, canonical ownership and resource identity | Cross-component string/list/result execution, post-return/free behavior, resource constructor/method/destructor coherence; incompatible providers rejected | Unverified: guest providers unsupported |
| LK-11 | The external world contains exactly permitted residual host capabilities | Used/unused service tests, guest transitive host dependency tests, disallowed capability and silent-fallback rejection | Partial: world membership, target-profile gating, and private-import closure checked; exact planned-set equality and guest transitive deps pending |
| LK-12 | Runtime artifact regeneration and compile lineage are reproducible | Pinned source/dependency/toolchain/recipe manifest, artifact hashes, repeat-build agreement, plan inputs and selected providers recorded in diagnosis | Partial: provenance/digest and plan lineage recorded; repeat-build agreement pending |
| LK-13 | Public Show executes with official semantics and preserved pure source/API | Pinned official source audit and JS oracle for Int, Number, Char, String, arrays and callback order; mandatory Wasmtime with byte-exact outputs | Partial: Wasmtime byte-exact Show set verified; library JS oracle external |
| LK-14 | Runtime owns one WIT/artifact catalog; linker owns definition resolution; backend owns source/ABI validation | Move pinned WIT/default-world assets without byte drift; ABI lookup and composition use one resolved context; reject pin drift; no compiler dependencies or formatter code in metadata-only consumption | Verified |
| LK-15 | Independent linker crate consumes target records without compiler IR dependencies | Dependency-graph audit; target-only plan/composition tests; backend request conversion and source-diagnostic attribution; no MIR/backend/HIR/Core types in linker APIs | Verified |

## Formatter behavior cases

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
  verification (`verify::tests`), definition resolution (`definitions::tests`),
  provider closure/memory planning and target-policy gating (`tests/plan.rs`),
  and composition (`tests/compose.rs`).
- `psrs-runtime` formatter token tests and catalog digest.
- `psrs-backend` `mir::number_format_tests::formats_a_number_through_the_runtime_artifact`
  executes the formatter artifact through the checked plan under Wasmtime and
  asserts the initialized token length through the command exit code.
- `psrs-driver` `tests::show::show_renders_the_instances_the_corpus_prints`
  executes `Data.Show` under Wasmtime and asserts byte-exact stdout across Int,
  Number (including `1e+21` and `1e-5`), Char, String escapes, unit, booleans,
  and arrays, interleaving formatting with WASI output and retained strings.
  `tests::show::formats_many_numbers_without_exhausting_the_runtime_stack`
  formats a 64-element Number array through the same private stack region.
  Memory growth is not exercised; the application allocator is pre-sized by the
  plan.
- `psrs-driver` `tests::diagnosis_trace::target_plan_records_provider_and_memory_lineage`
  and `tests::show::formatter_plan_records_the_pinned_artifact_digest` assert the
  compile diagnosis records the selected providers, artifact digests, memory
  boundary, and planned external world.
- Workspace validation: `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, and `cargo test --workspace` (with
  `PSRS_STDLIB_ROOT` for the dirty stdlib checkout). One pre-existing
  `psrs-resolve` unit failure reproduces at `HEAD`.

## Current continuation state

The formatter slice is implemented and verified to the extent above. Remaining
work:

- A measured runtime stack bound or an explicit rejection of reentrant calling
  for the formatter (LK-07).
- Repeat-build agreement for the artifact provenance (LK-12).
- The library-owned official `Show` oracle and source/API audit (LK-13), in
  `psrs-stdlib`.
- Guest WIT provider loading, resource identity, and composition (LK-09..LK-11),
  including the shared allocator and explicit instantiation steps a second
  artifact would need.

Do not label the overall topic complete after only the formatter slice.
