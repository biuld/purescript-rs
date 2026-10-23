# Checked Support Environment Acceptance

**References:** [D-01](../../design/D-01-frontend-and-ir-boundaries.md),
[deriving](../../design/frontend/type-system/deriving.md),
[classes and evidence](../../design/frontend/type-system/classes-and-evidence.md),
[runtime program units](../runtime-program-scoreboard.md).

**Status:** Implemented and verified on 2026-10-24 for
[#173](https://github.com/biuld/purescript-rs/issues/173). The complete runtime
measurement is 312/340, recovering all four target roots with no lost agreement.

## Contract and observed boundary

At compiler `c9a1e64d`, four exact one-root diagnoses first fail in P5 with
`CannotDeriveInvalidConstructorArg`: `DerivingContravariant`,
`DerivingFunctorFromBi`, `DerivingFunctorFromPro`, and `DerivingProfunctor`.
Reports in `/tmp/psrs-deriving-cohort` use original corpus bytes, complete user
input sets, the locked package and a 30-second timeout. Their pass manifests
request no dumps; canonical artifact summaries and expression lineage remain
unavailable.

The official v0.15.16 [test runner](https://github.com/purescript/purescript/blob/v0.15.16/tests/TestUtils.hs)
uses `rebuildModule actions supportExterns` for a single root. Its
[Make implementation](https://github.com/purescript/purescript/blob/v0.15.16/src/Language/PureScript/Make.hs)
loads all supplied externs into the checking environment. Multiple-module cases
use ordinary dependency-ordered `make`. Direct `purs compile` rejects these same
four original roots under import closure; it does not exercise the single-root
suite environment. The defect is a harness environment mismatch, not a
demonstrated variance-analysis defect.

The explicit complete-support API checks all 309 locked support modules with
their ordinary imports. The root then depends on every support producer's
success. Exported instances use the existing traversal and candidate-module
rules; exported value types come only from successful immutable checked
publication. Source names still require imports. The runtime harness selects
this mode for single-source groups and retains ordinary compilation for groups
with several sources. No official library or corpus source is changed.

## Requirements and evidence

| ID | Requirement | Evidence | State |
| --- | --- | --- | --- |
| SE-01 | Single-root support scope is explicit and distinct from ordinary import closure and multi-module make | SupportEnvironment, complete-support API, group-size selection and ordinary-scope rejection test | Verified for covered cases |
| SE-02 | Only successful checked producers supply support metadata, preserving identities, kinds and calling conventions | Failed-producer and invalid-prefix tests; reuse of checked value publication and ordinary instance checking | Verified for covered cases |
| SE-03 | Generated terms use the shared deriving registry, instantiation and solver | Unimported Profunctor dictionary executes a derived nested-function adapter, returning 42 | Verified for covered case |
| SE-04 | Unimported names stay hidden; unrelated same-name classes and orphan dictionaries gain no authority | Hidden-name, unrelated Functor, canonical class reexport and orphan-instance tests | Verified for covered cases |
| SE-05 | Original four roots compile, validate and execute without source edits | Frozen release harness and retained Wasm executions print Done and exit zero | Verified for four roots |
| SE-06 | Derived maps, folds, traversals, variance and callback order execute correctly | Mandatory release acceptance and literal official TestUtils comparison both observe value 42 | Verified for covered cases |
| SE-07 | Frontend and complete runtime gates are remeasured with explicit resource outcomes | Five frontend boards pass; complete serial runtime board and 365-root acceptance audit pass | Verified |
| SE-08 | Original roots execute under the actual official single-root support-external environment | Unmodified TestUtils.compile on original absolute paths; all four Node executions succeed | Verified for four roots |

## Local validation (2026-10-24)

Seven native scope tests pass, including a mandatory Wasmtime nested-function
adapter. Formatting, source layout and full-workspace Clippy with warnings denied
pass. `cargo test --workspace`, with four test threads, passes including doc-tests:
**2356 passed, zero failed, seven ignored**. The expensive ignored acceptance test
actually executes separately; an ignore marker alone supplies no runtime evidence.
Its command is:

```sh
PSRS_REQUIRE_WASMTIME=1 cargo test --release -p psrs-driver --test complete_support -- --ignored --nocapture
```

That test passes in 58.14 seconds. It checks mapped and fixed values, both fold
directions, traversal callback order, predicate input transformation and
profunctor input/output transformation. The four existing official deriving
differential batteries also pass with the pinned reference checkout selected.
Logs are `/tmp/psrs-support-{workspace-final,clippy-final,layout,value-runtime-final,deriving-differential-final}.log`.
An initial workspace attempt stopped on two file-layout violations; both were
corrected before the successful full replacement run.

All five frontend boards pass in `/tmp/psrs-support-frontend.log`: L1 904/908
(four DEC-16 differences), L2 76/76 plus 403/413 passing resolution, L3 50/50,
L4 65/65, L5 101/101, and roles 12/12. Complete support currently checks every
support source for each program; approximately 49-second focused release times
include that work. This is not a checked-library cache or a performance claim.
A preliminary debug all-four probe hit its 240-second outer limit before its
first artifact; it supplies neither acceptance nor a semantic rejection.

## Literal official comparison

The temporary oracle calls unchanged `TestUtils.setupSupportModules` and
`compile (Just IsMain)` from official compiler commit
`4c66c05a59a08b698ba5c34b48e8bf7744ecb9f6` (v0.15.16). All 180 compiler-source
and TestUtils input files match the pinned archive. Its 281 support inputs,
original root hashes, wrapper and executable hashes are recorded in
`/tmp/psrs-purs-rebuild/oracle-measurement.json`.

It reads the four original absolute paths without rewriting their module headers,
imports or declarations. Each compiles and executes under Node with exactly
`Done\n`, empty stderr and exit zero. The value-sensitive fixture also compiles
and observes 42. Generated JS, externs, source copies and complete output are
retained in `/tmp/psrs-purs-rebuild/results`.

Five independent Wasm artifacts are retained in `/tmp/psrs-support-retained-wasm`.
Their source and artifact hashes and official comparisons are in
`/tmp/psrs-support-retained-comparison.json`. Wasmtime 48.0.1 uses the same
524288-byte stack and 10-second guest bound. The four command outputs and exit
statuses match Node exactly. For the pure Int fixture, Wasm returns command exit
42 with empty stdout, while Node prints the value 42: this compares the value
under the project's command protocol, not identical output protocols.

The oracle was built with GHC 9.8.4 and the official LTS 23.18 snapshot. The
standalone executable successfully repeated all five checks after the temporary
GHC, Stack and build caches were removed. It remains replayable without GHC.
Earlier PSCi declaration replay also passed, but renamed Main to $PSCI; it is
supplementary evidence, not a substitute for this literal comparison.

## Full runtime measurement (2026-10-24)

The frozen runtime artifact is `/tmp/psrs-support-suite-final`, SHA-256
`b75648d4018f05f080c38755d321a97f37184b93e8f4f891cbd5017cce0bb15f`.
Metadata, retained source hashes, final results and acceptance audit are in
`/tmp/psrs-support-runtime-board-final`. All 1153 recorded Rust source files
still match byte-for-byte after the two documented path-only moves, as recorded
by `source-layout-equivalence.json`. No running artifact has been overwritten.

The board retains one worker, a 10-second guest bound, a 524288-byte Wasm stack,
the locked package and the separate BigFunction compiler resource policy. Its
checking environment changes for single-source groups; any resulting movement
is attributed to the harness contract repair. The completed board records
**312/340**, all exiting zero, with all 365 roots accounted for, the same 25 FFI
exclusions and 74 support sources retained as inputs. The exact recoveries are
`DerivingContravariant`, `DerivingFunctorFromBi`, `DerivingFunctorFromPro` and
`DerivingProfunctor`; no prior agreement is lost. The four retained command
outputs also match the literal official runner as described above.

The remaining 28 are 14 P5 typecheck, 3 P5 kind check, 5 P3 resolve, 1 P8
closure conversion, 2 P0 lex, the `TCOMutRec` assertion trap, the `2288` guest
timeout and BigFunction's incomplete compiler verdict. The driver completes
successfully; `/tmp/psrs-support-final-audit.py` separately passes the exact
365-root inventory, exclusions, source-byte equivalence, artifact hashes,
recovery/loss sets, failure descriptions and retained execution comparisons.
The library lock remains `618bede18e4bf7fc57206c057781a12fcef2b3b4` /
`fnv1a64-v1:44b22a61d1b2b19e`. This does not complete the broader runtime gate.

The separate BigFunction observation has completed: compiler status -6 after
29.581 seconds, with an observed memory-allocation failure under the unchanged
8 GiB compiler address-space bound and requested 90-second outer timeout. It
supplies no semantic compiler verdict and is not a guest trap or acceptance.
The program remains in the complete 365-root inventory and the 340-case denominator.
