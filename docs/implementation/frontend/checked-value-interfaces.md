# Checked Value Interface Acceptance

**References:** [type inference](../../design/frontend/type-system/type-inference.md),
[D-01](../../design/D-01-frontend-and-ir-boundaries.md),
[D-04](../../design/D-04-suite-roadmap.md).

**Status:** Implemented and verified for #170 (2026-10-24). Broader Phase 5 remains incomplete.

## Contract

Successful P5 producers publish immutable, closed semantic value types before
dictionary erasure. Each type binder has a lexical identity, visibility and a
checked kind reference. Kinds close jointly through their own owner, preserving
sharing and nested kind quantifiers without producer solver identities. Class
constraints retain their order, class identity, arguments and source ranges.
Type constructors retain stable resolved identities; type-level literals and
structural rows/quantifiers remain structural.

The representation is opaque outside the type checker. Export conversion rejects
unbound inference variables and missing binder kinds. Instantiation allocates
fresh type and kind scopes and uses shared variable synthesis and constraint
application. Candidates are withheld until THIR verification succeeds. Source
foreign declarations use their signature span and explicitly lack a name span
when HIR supplies none; ordinary value declarations preserve both ranges.
Legacy single-module APIs accept caller-supplied HIR signatures as trusted
assumptions. Program compilation selects successful interfaces by resolved import
identities and checks modules in dependency order. Compiler-owned virtual modules
remain explicit trusted inputs. A failed dependency supplies no checked interface.

## Evidence matrix

| ID | Requirement | Evidence | State |
| --- | --- | --- | --- |
| CV-01 | Closed kinds preserve sharing, solutions, fresh uses and nested scopes | Four `psrs-kind::closed` tests | Verified |
| CV-02 | Ordinary, constrained, rank-N and polykinded inferred values cross module boundaries | Shared `support/inferred_imports.rs` cases, official differential and mandatory execution | Verified for seven executed source sets |
| CV-03 | Qualified, re-exported and transitive identities retain the producer type | Shared re-export and transitive cases, reversed-input lenient checks | Verified for focused cases |
| CV-04 | Dictionary order and visible binders survive publication | Ordered-constraint and visible-binder executions | Verified for focused cases |
| CV-05 | Failed producers and private values never become usable checked imports | Strict/lenient rejection and diagnostic-origin cases | Verified for focused cases |
| CV-06 | Official input closures compile, validate and execute | Six same-input diagnoses, complete runtime board and full-output purs/Node comparison | Verified for six recovered roots |
| CV-07 | Whole-workspace and frontend gates remain valid | Formatting, full workspace including doc-tests, Clippy and actual frontend boards | Verified |
| CV-08 | Foreign values, malformed metadata and cycle boundaries remain explicit | Malformed metadata native rejections, foreign acceptance with explicit missing target, official CycleInModules | Verified for focused cases |

## Baseline

The topic starts from `4bbbe574` with only pre-existing untracked `output/`.
Six complete corpus diagnoses use one-case exact root filters, the locked
library and 30-second timeout. Each first blocks in P5 with `global has no type
or intrinsic declaration`: `Import`, `ImportQualified`, `ModuleDeps`,
`ReExportQualified`, `TransitiveImport`, and `TransitiveImportUnnamedInstance`.
`/tmp/psrs-inferred-import-baselines.json` records the individual reports and
input fingerprints. These diagnoses were collected on the same production code
before its topic commit; the worktree fingerprints retain that distinction.

The driver previously collected only handwritten HIR signatures before checking
any module. Successful inferred schemes were never published to later importers.
The new boundary closes the producer's actual checked scheme rather than
reconstructing constraints from dictionary-erased THIR or inserting fabricated
source signatures.

## Final validation (2026-10-24)

The serial frozen runtime scoreboard reaches **308/340**, from 302/340, with
exactly the six baseline roots recovered and no lost agreement. All agreeing
programs exit zero. Of 365 roots, 25 FFI groups remain excluded and 74 support
sources remain compilation inputs. The remaining 32 comprise 18 P5 typecheck,
3 P5 kind check, 5 P3 resolve, 1 P8 closure conversion (`Stream`), 2 P0 lex
(DEC-16), the `TCOMutRec` assertion trap, the `2288` guest timeout, and
`BigFunction` without a compiler verdict after an allocation failure.

The harness is `/tmp/psrs-checked-interface-suite-final-v2`, SHA-256
`539850332eb2fa163cbf16eb9853518adb184ae98195b57c89c240c2e0439904`.
The measurement records compiler base `4bbbe574` plus this uncommitted topic;
its result directory is `/tmp/psrs-checked-interface-runtime-board-final-v2`.
Wasmtime 48.0.1 uses one worker, a 10-second guest timeout and a 524288-byte
Wasm stack. The library lock remains
`618bede18e4bf7fc57206c057781a12fcef2b3b4` /
`fnv1a64-v1:44b22a61d1b2b19e`. `BigFunction` has the unchanged compiler-only
8 GiB address-space bound and requested 90-second outer timeout; it exits -6
after 34.783 seconds. That resource failure is neither a successful compile nor
a semantic rejection. The engine wrapper removes that bound for Wasmtime.

The scoreboard samples only the first stdout line. A separate full-output
comparison builds all six original corpus input closures with both compilers:
four print `Done\n`, and the two transitive imports print `unit\nDone\n`.
Complete stdout, stderr and exit status match purs/Node exactly. Per-artifact
Wasm hashes and full observations are recorded in
`/tmp/psrs-checked-interface-full-output/results.json`; official observations
and input hashes are in `/tmp/psrs-checked-interface-purs/results.json`.

Focused verification passes five driver tests (seven mandatory exit-42 programs,
strict/reversed lenient checking, visibility/source ranges, failed producers and
foreign/cycle boundaries) and all 24 purs differential functions. The four closed
kind tests and two invalid-interface tests pass. Same-input before/after
comparisons for all six original official programs report compile recovery;
canonical artifact-content and expression-lineage comparisons remain unavailable.

`cargo fmt --all --check`, `cargo test --workspace` (including doc-tests:
2349 passed, zero failed, six ignored), and
`cargo clippy --workspace --all-targets -- -D warnings` pass. The five actual
frontend boards reconfirm L1 904/908 (four DEC-16 differences), L2 76/76 and
403/413 passing resolution, L3 50/50, L4 65/65, L5 101/101, and roles 12/12.
Final logs use the prefix `/tmp/psrs-checked-interface-` and suffix
`-final-v2.log` for workspace, focused, frontend, kind, Clippy and formatting.

A scope review reproduced a closure defect independently in
`/tmp/psrs-closed-kind-shadow-repro.rs`: resolving the entire kind first replaced
an inner lexical binder with an unrelated solved producer cell. The kind owner
now resolves variable chains under the current binder scope, including solved
templates. Regression tests cover that shadowing and free solutions referring
to an enclosing binder. Initial whole-workspace/runtime runs were deliberately
interrupted for this correction and are not final acceptance evidence.
