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
