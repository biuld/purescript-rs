# Runtime scoreboard program units

**Scope:** L6/M7 runtime measurement; frontend scoreboards retain their per-file
units. This corrects harness accounting, not compiler or library support.

## Contract

The pinned official runner groups a category-root `.purs` file with its
same-stem support directory. See `tests/TestUtils.hs::getTestFiles` and
`tests/TestCompiler.hs::assertCompiles` in the reference PureScript checkout.
The latter compiles the group, runs `Main.main`, and requires the final output
line to be `Done`.

The runtime board follows those program groups. It retains all support sources
as compilation inputs, but never runs them again as independent commands.
Selection does not depend on successful parsing or discovering `main`, so a
malformed root or a missing entry remains a failure. Filters and limits apply
to program roots, in that order. `PSRS_SUITE_PROGRESS=1` prints each root before
compiling it so a long run identifies its current program. A foreign declaration or sibling JavaScript
file in any program source excludes the whole program under the existing FFI
policy. Compiler-owned target bindings in the trusted library are not corpus
FFI exclusions.

The vendored `passing` directory contains 439 source files: 365 program roots
and 74 support files. These are inventory units, not interchangeable acceptance
denominators. Frontend boards continue to inspect individual source files.
Every support source in this pinned inventory has a same-stem program root;
there are no directory-only program groups.

This board still records output and exit status without requiring `Done` or
comparing output goldens. Compilation, Wasm validation, and completion without
a trap do not establish full observable agreement with upstream.

## Baseline and owner

Starting checkout: clean `master` at `fe6dd78f`. The historical per-file board
at `886cd82c` recorded 273/413 and 66 failures attributed to
`Data.Symbol.unsafeCoerce`. All 66 paths are support modules with no `main`.
Compiling those modules alone retains every library declaration because there
is no executable entry from which to prune unreachable imports.

The small baseline `2018/A.purs` fails at P8 library linking on the unchanged
compiler; the enclosing `2018.purs` supplies the actual command and its support
sources. Standalone `Data.Symbol.reifySymbol` target support remains unproved
and is not implemented by this correction. No library source, primitive
registry, reachability rule, or package lock changes.

## Verification

Regression coverage is in `suite/corpus/tests.rs`: a support module supplies a
value to its parent program without becoming a command; malformed and
entry-less roots remain selected; filtering precedes limiting; support foreign
declarations and JavaScript files exclude their program.

The first attempted full run was invalidated when the sibling standard-library
checkout changed branches. Its worker failures are not compiler measurements.
Subsequent runs use an archive of locked commit
`4c73aab52226e8f5e7b1f17052f1a19d717a472e`, selected explicitly with
`PSRS_STDLIB_ROOT`; the required content fingerprint remains
`fnv1a64-v1:ad19620ada1d2ecf`.

## Full remeasurement: 2026-10-17

Compiler: `fe6dd78f` plus the checked command-runner and program-grouping
changes. Package: `50c26d67a78a6b25a9915b28fef981bc178cd767`, fingerprint
`fnv1a64-v1:4b052f09c153208f`. Runtime: Wasmtime 49.0.3. The measured release
suite binary SHA-256 is
`34e755d1b36825cb5fe9b724afbb4c33ba5428bcec1c9114ecd78ca00efa7263`.
The debug full-workspace check is a separate validation run.

Two disjoint cohorts of 183 and 181 roots completed using the same source
bytes and support directories. The separately bounded `BigFunction.purs`
completes the 365-root inventory; the aggregation checks complete membership
without duplicates. There are **25 excluded FFI programs**, **340 in-scope
programs**, and **273 completions**, all with exit zero and no trap.

| Remaining outcome | Programs |
| --- | ---: |
| P5 typecheck | 31 |
| P5 kind check | 11 |
| P8 closure conversion | 13 |
| P3 resolve | 5 |
| P0 lex, DEC-16 intentional differences | 2 |
| Unsupported library foreign | 3 |
| Runtime timeout, 10 seconds | 1 |
| Compiler resource bound | 1 |

The foreign failures are `Record.Unsafe.unsafeGet` in `Deriving.purs` and
`TCOFloated.purs`, and `Test.Assert.checkThrows` in `TCOMutRec.purs`.
`2288.purs` times out during execution. BigFunction exceeds the 2 GiB RSS bound
after 17.54 seconds before returning a diagnostic or publishing a runnable artifact; it is an
unverified compilation, not a guest trap. No requirement is closed by stopping
that process. Its debug probe separately reached a 30-second compilation limit.

The previous 273/413 is historical per-file accounting. All 66 Symbol failures
are support modules. Program-wide FFI exclusion additionally removes the
previously completing `ForeignKind.purs` because its support source declares
foreign types. The actual compiler/library repair executes unchanged
`Sequence.purs`; the unchanged numerator therefore does not mean unchanged
behavior. These are different cohorts, not 66 newly working programs.

Commands use `PSRS_REQUIRE_WASMTIME=1`,
`PSRS_STDLIB_ROOT=/tmp/psrs-command-runner-stdlib`, and the release suite built
by `cargo test --release -p psrs-driver --test suite --no-run`. Each cohort
selects its staged corpus with `PURESCRIPT_REPO` and executes
`suite --ignored --exact runtime::l6_runtime_scoreboard --nocapture`.
`PSRS_SUITE_PROGRESS=1` records roots before compilation. BigFunction uses an
isolated worker with a 30-second process limit and a 2 GiB RSS guard.

Local evidence: `/tmp/psrs-runtime-programs-release-0.log`,
`/tmp/psrs-runtime-programs-release-1.log`,
`/tmp/psrs-runtime-programs-release-big-function.json`, and
`/tmp/psrs-runtime-programs-acceptance/summary.json`. The adjacent source
manifest retains all 439 source hashes. Source hashes and suite identity in the
summary distinguish the measured compiler changes from their base.
Earlier incomplete debug cohorts were superseded and are not measurements.

The four frontend boards were also re-run with the release suite,
`PSRS_ORACLE=annotations`, and `--ignored --skip runtime::l6_runtime_scoreboard
--nocapture`. L1 remains 904/908, L2 72/72 and passing resolution 403/413,
L3 39/48, L4 39/50, and L5 77/97. The source-file denominators are unchanged.

The final debug workspace run with `PSRS_REQUIRE_WASMTIME=1` and the isolated
locked-content package passes: 2,034 passed, five ignored scoreboards (executed
separately above). Formatting, Clippy with warnings denied, and diff whitespace
checks pass. The focused corpus/runtime classifier tests report 14 passed.

## Symbol and contextual-checking integration recheck

The 2026-10-17 recheck uses package
`6fcef6ccbab1f3459af2573bbef4ae25b86852d4`, fingerprint
`fnv1a64-v1:5fcbb57525c63eb1`, and the same compiler base plus the
[contextual application check](frontend/contextual-application-results.md).
The release suite SHA-256 is
`f98043ae777bdaf544f2250f58a5c6c74f979a80f74110a8daf86700a4b0227a`.
Both disjoint cohorts finish: 136/170 and 137/169, excluding 13 and 12 FFI
programs. BigFunction again exceeds the 2 GiB RSS guard before a diagnostic,
after 20.25 seconds (60-second outer limit). The total remains 273/340, all
exiting zero, with the exact same 67 failure paths and reasons as above.
All 439 source hashes and all 365 program roots are checked during aggregation.

Evidence is `/tmp/psrs-symbol-runtime-{0,1}.log`,
`/tmp/psrs-symbol-runtime-big-function.json` and
`/tmp/psrs-symbol-runtime-acceptance/summary.json`. All L1–L5 boards were also
rerun on this package and checker; their numbers remain unchanged. This new
package pin adds independently executed Symbol API coverage, with no claim of
recovering a program from the existing runtime corpus.

## Open-record read/transport checkpoint recheck: 2026-10-17

Compiler `fe6dd78f` plus the open-record read/transport and
control-pair boundary changes recorded in
[the same-day handoff](backend/open-records-handoff-2026-10-17.md) and committed
with it, package
`6fcef6cc` / `fnv1a64-v1:5fcbb57525c63eb1`, Wasmtime 49.0.3, release suite
binary SHA-256
`61d4dea7488511191630dcf2adb060f2de9d62701786893434b6b6f955212b7e`. Every
program root was measured with its own `PSRS_SUITE_FILTER` worker
(`PSRS_REQUIRE_WASMTIME=1`), three at a time; BigFunction ran under the
isolated 2 GiB RSS guard and stopped before a diagnostic.

The board is **276/340** at the read/transport checkpoint: all completions exit
zero, 25 FFI groups are excluded, and the 64 non-agreeing roots carry the same
reasons as the pre-repair measurement besides the three newly passing programs
(`1807.purs`, `NestedRecordUpdate.purs`, `ObjectGetter.purs`). Their joint
reason profile is 31 P5 typecheck, 11 P5 kind check, 13 P8 closure conversion,
5 P3 resolve, 2 P0 lex (DEC-16), 3 missing library foreigns, one 10-second
runtime timeout, and the bounded BigFunction compilation. The read/transport
release suite binary is
`61d4dea7488511191630dcf2adb060f2de9d62701786893434b6b6f955212b7e`; the open-row
record-pattern slice then lifts the board to **279/340** with exactly the three
pun-blocking roots (`FieldPuns.purs`, `FieldConsPuns.purs`,
`ReservedWords.purs`) newly passing and no other reason changed, on release
suite binary
`7c9f8839dbd7cf73d63322e90ca2eefd5a904551c63cf73b76bb77b64673e2eb`, whose
decomposition is 31 P5 typecheck, 11 P5 kind check, 10 P8 closure conversion,
5 P3 resolve, 2 P0 lex, 3 missing foreigns, one timeout, and the bounded
BigFunction compilation. Evidence:
`/tmp/psrs-open-record-2026-10-17-runtime/summary.json` per program and
`/tmp/psrs-open-record-2026-10-17-boards.log` for the L1–L5 boards, whose
numbers are unchanged. The full debug workspace run at the pattern-slice tree
is 2,056 passed, 0 failed, 5 ignored
(`/tmp/psrs-open-record-2026-10-17-pattern-workspace.log`).

## Open-row immutable-update recheck: 2026-10-17

Compiler `fe6dd78f` plus the open-record read/transport, record-pattern and
open-row immutable-update slices recorded in
[the same-day handoff](backend/open-records-handoff-2026-10-17.md) and
committed with it — package `6fcef6cc` / `fnv1a64-v1:5fcbb57525c63eb1`,
Wasmtime 49.0.3, release suite binary SHA-256
`8585a60b24a8a02eb8b5c132c596359422492e224f6199a133fefd2364476337` — measured
with the same per-root `PSRS_SUITE_FILTER` worker arrangement, three workers,
`PSRS_REQUIRE_WASMTIME=1`, and the isolated 2 GiB RSS guard for BigFunction.

The board is **282/340**: all completions exit zero, 25 FFI groups are
excluded, and the 58 non-agreeing roots carry the previous reasons besides the
three newly passing ones (`ObjectUpdate.purs` and
`NestedRecordUpdateWildcards.purs`, both previously rejected by absent
canonical record-update labels, and `2616.purs`, whose derived
`Functor (F r)` map writes its open-row field). The reason profile is 31 P5
typecheck, 11 P5 kind check, 7 P8 closure conversion within the closure-linking
family (3 closure capture, 1 recursive local binding, 3 missing library
foreigns `Record.Unsafe.unsafeGet` 2 and `Test.Assert.checkThrows` 1), 5 P3
resolve, 2 P0 lex (DEC-16), one 10-second runtime timeout, and the bounded
BigFunction compilation. One failing program (`RowPolyInstanceContext.purs`)
printed its undetermined-variable pair in both orders across repeated runs of
the same binary; the presentation order is run-to-run noise and the case fails
typechecking either way. Frontend boards were not rerun; the slice changed
only backend lowering and its tests. Evidence:
`/tmp/psrs-open-record-2026-10-17-runtime-281/summary.json` per program and
`/tmp/psrs-open-record-2026-10-17-update-workspace.log` for the full debug
workspace (2,062 passed, 0 failed, 5 ignored).

## Checked record-storage recheck: 2026-10-17

Compiler `fe6dd78f` plus the earlier open-record slices and the checked
record-storage runtime primitives with official library record wrappers
recorded in [the open-records checklist](backend/open-records.md) and
[the same-day handoff](backend/open-records-handoff-2026-10-17.md) — package
`e556bf2f` / `fnv1a64-v1:7d96f1c44d8d52cc`, Wasmtime 49.0.3, release suite
binary SHA-256
`37bcdd007466f2ed91f2cbd06962c584bcdd00caef10f73465a5070f36d1f4ee` — measured
with the same per-root `PSRS_SUITE_FILTER` worker arrangement, three workers,
`PSRS_REQUIRE_WASMTIME=1`, and a 90-second cap with an explicit RSS guard for
BigFunction.

The board is **284/340**: all completions exit zero, 25 FFI groups are
excluded, and exactly the two roots blocked by the missing
`Record.Unsafe.unsafeGet` foreign (`Deriving.purs` and `TCOFloated.purs`) join
the agreeing set; no other failure changed state. The reason profile is 31 P5
typecheck, 11 P5 kind check, 5 P8 closure/linking (3 closure capture,
1 recursive local binding, and `TCOMutRec.purs`, still blocked by the missing
`Test.Assert.checkThrows` foreign), 5 P3 resolve, 2 P0 lex (DEC-16), one
10-second runtime timeout, and the bounded BigFunction compilation, which
reached the measurement cap near the 2 GiB guard before a diagnostic. Evidence:
`/tmp/psrs-open-record-or08-or09-runtime/summary.json` per program and
`/tmp/psrs-or08-final-tree-workspace.log` for the full debug workspace at the
same tree (2,084 passed, 0 failed, 5 ignored, with
`PSRS_REQUIRE_WASMTIME=1`).

## Shared coverage and Partial recheck: 2026-10-17

Compiler `7dd45e4b` plus the shared coverage/checker `Partial` slice and the
transparent-newtype-cycle repair, with the same clean package `e556bf2f` /
`fnv1a64-v1:7d96f1c44d8d52cc`, required Wasmtime 49.0.3 and release suite
SHA-256 `bab10d4c7480c7db64fce9dacba8c96c035f9e760a810679a9d2a750a59233a3`, measures **287/340**.
All successful programs exit zero. The 365 roots include 25 excluded FFI
groups; 74 support sources remain inputs. Relative to the preceding 284/340
measurement, exactly `Let.purs`, `ShadowedTCOLet.purs`, and `Where.purs` recover,
and no agreeing program regresses.

The remaining 53 are 31 P5 typecheck, 11 P5 kind check, 2 P8 closure/linking
(one recursive local binding and `TCOMutRec.purs`'s missing
`Test.Assert.checkThrows` foreign), 5 P3 resolve, 2 P0 lex, the existing
10-second `2288.purs` runtime timeout, and the bounded BigFunction compilation.
BigFunction times out at 90.89 seconds with peak RSS 1,485,792 KiB under the
90-second cap and 2 GiB guard. The three-worker run completed 364 roots; its
sandbox blocked `ps` when starting BigFunction's RSS check, so that unfinished
worker was stopped and BigFunction was rerun separately with the guard enabled.
The final manifest combines those 364 completed rows with that guarded result.

Evidence: `/tmp/psrs-partial-final-runtime/summary.json`, `results.jsonl`, and
`big-function-checked.json`. The frontend boards were also rerun at this slice:
L1 904/908, L2 72/72 (passing resolution 403/413), L3 39/48, L4 39/50,
L5 **81/97** (`NoInstanceFound` **52/53**); see
`/tmp/psrs-partial-frontboards-final.log`. These are compile/code agreement and
exit-code measurements; they do not establish output-golden conformance.

The same slice's full debug workspace passes 2,093 tests, zero failures and five
ignored scoreboards (run separately as above), with required Wasmtime and the
same package: `/tmp/psrs-partial-workspace-final.log`. Formatting, source layout,
and strict full-workspace Clippy also pass.

## Generic local rows recheck: 2026-10-18

Compiler `7dd45e4b` plus the preserved shared-coverage slice and the removal of
P8's mandatory local-row specialization, using the same clean package
`e556bf2f` / `fnv1a64-v1:7d96f1c44d8d52cc` and required Wasmtime 49.0.3,
remeasures **287/340**. The agreeing set is unchanged: no regressions and no
recoveries. All successful programs exit zero. The same 365 roots, 74 support
inputs and 25 excluded FFI groups are retained. The release suite SHA-256 is
`52a9b77e706fd2030ed8c235c75f3a313ee118a18de0b49e4be21eb375e03df1`.

The ordinary three-worker run completes 364 roots. BigFunction is separately
bounded by the same 90-second cap and 2 GiB RSS guard; it reaches the RSS guard
in 27.89 seconds, with observed peak 2,141,824 KiB. This differs from the previous
90.89-second timeout at 1,485,792 KiB, but both are incomplete compilation and
neither counts as runtime agreement. The other failures retain their prior
31 P5 typecheck, 11 P5 kind check, 2 P8, 5 P3 resolve, 2 P0 lex and one runtime
timeout split. No claim about BigFunction performance is made from the changed
preparation path.

Evidence: `/tmp/psrs-or07-runtime/summary.json`, `results.jsonl` and
`big-function-checked.json`. The current library-owned `Record.Unsafe` JS/Wasm
oracle also accepts its normal value program and three expected-trap programs
(`/tmp/psrs-or07-record-oracle/record-unsafe.json`); Node conformance passes
13/13. The frontend boards remain L1 904/908, L2 72/72 (passing resolution
403/413), L3 39/48, L4 39/50 and L5 81/97 (`NoInstanceFound` 52/53):
`/tmp/psrs-or07-frontboards.log`.

The matching final debug workspace passes **2,100 / 0 failed / 5 ignored**,
with required Wasmtime and the same development package
(`/tmp/psrs-or07-workspace.log`); the five ignored scoreboard groups execute
separately above. Formatting, source layout and strict workspace Clippy pass.
The seven new regressions cover generic local-row lowering, composed row
transport, per-operation CC/runtime rejection and actual nested ABI values.
This remains separate from whole-topic acceptance.

## Integrated open-record review recheck: 2026-10-18

Commit `e331f9ba` with only `cfg(test)`-only driver tests and documentation
added since, the same clean package
`e556bf2f` / `fnv1a64-v1:7d96f1c44d8d52cc` and required Wasmtime 49.0.3,
re-measures the board with the worker pattern above: **287/340**. The release
suite binary is unchanged at SHA-256
`52a9b77e706fd2030ed8c235c75f3a313ee118a18de0b49e4be21eb375e03df1` because no
compiler or suite source differs; the identity is re-established rather than
assumed. All 364 ordinary roots complete; 25 FFI groups remain excluded.

The failing set is identical to the previous checkpoint — the same 31 P5
typecheck, 11 P5 kind check, 2 P8, 5 P3 resolve, 2 P0 lex and one ten-second
runtime timeout in `2288.purs` — with no regressions and no recoveries.
BigFunction is separately bounded by the RSS guard again: it stops at
33.39 seconds with observed peak 2,137,584 KiB on this run, against
27.89 seconds / 2,141,824 KiB previously on the identical compiler. The
difference is run-to-run wall-clock variation, and no claim about BigFunction
performance is made from either run.

Evidence: `/tmp/psrs-or0405-runtime/summary.json` and `results.jsonl`. The
debug workspace passes **2,107 / 0 failed / 5 ignored**
(`/tmp/psrs-or0405-workspace-full.log`); the seven additional tests are the
open-row interaction regressions recorded in the
[open-record checklist](open-records.md#integrated-topic-acceptance-review-2026-10-18).
With every requirement row re-executed and the board re-measured, the open-record
checklist is complete at commit `e331f9ba` plus this review.

## Serial rerun after the instance and diagnostic slices: 2026-10-18

Compiler master after the five instance/diagnostic slice merges (`54d69f92`
instance-head kinds, `56fe3cda` annotation-position codes, `4a078b95`
instance heads and cycles, `298cc585` the orphan rule, `fd7bf88a` duplicate
type classes, `9a4c2e04` inclusion-minimal covering sets), package
`e556bf2f` / `fnv1a64-v1:7d96f1c44d8d52cc` unchanged. The measurement host
differs from the earlier close-outs: Wasmtime **CLI 48.0.1**, no `purs`
installed. Protocol: **one worker per root, serially** (`xargs -P1`), each
root filtered by `passing/<basename>` with a 110-second compile cap, all 365
roots including `BigFunction.purs`; suite binary
`target/release/deps/suite-461c5a8e1563a05b`, SHA-256
`90da2757e8e2e2a362a7d8cfdd7f0f686d6655c502f006809385a7457439fa00`.

The board is **287/340**: 287 completions exit zero, 25 FFI groups excluded,
52 named non-agreements and the resource-bounded `BigFunction.purs`. The
failing set is identical to the 2026-10-18 review's; the rows' stage
attribution moved with the newly mapped kind and diagnostic codes (three rows
now report `P5 kind check [ExpectedType]`: `4500.purs`, `4535.purs`,
`TypeSynonymsInKinds.purs`, and three `P5 kind check [UndefinedTypeVariable]`).

**Measurement caveat, recorded rather than claimed:** two rows
(`NonConflictingExports.purs`, `NonOrphanInstanceFunDepExtra.purs`) time out
at the board's fixed 10-second guest wall under concurrent measurement load
(two full parallel board runs ran simultaneously when this was first seen)
and **pass when re-measured individually** on the identical binary — `1/1,
stdout: Done`, verified twice (2026-10-18, sequential). The number 287/340 is
stable across the parallel and serial protocols; the caveat documents that
these rows are load-sensitive, not that the success set grew. Raw per-root
rows: `/tmp/opencode/s1l6/final.raw` (this tree) and `/tmp/opencode/s1l6/base.raw`
(the 54-name pre-slice comparison at `cbdfceb2`, suite SHA-256
`befcb214322a19f5ea61c23e3fdb6cf49954e9a84a7a681499e41895e2fcf318`).
`BigFunction.purs` again did not finish inside a 90-second cap on this host
(`/tmp/opencode/s1l6/bigfunc.verdict`); its resource-bounded row is unchanged
in kind, and no compile acceptance is claimed.
