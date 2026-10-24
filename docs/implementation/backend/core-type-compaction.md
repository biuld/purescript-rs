# Core type-table compaction acceptance

The runtime validation follow-up to [#175](https://github.com/biuld/purescript-rs/issues/175)
is governed by [Core optimization](../../design/backend/opt/core.md). Complete
support checking must retain its full 309-module environment. Official sources,
the library lock, verification requirements and guest limits do not change.

The representative `1110` program reached the backend with 1,119,969 Core type
nodes after reachability pruning left only 24 declarations. The type table
still contained the unused library types. Instrumented stage observations in
`/tmp/psrs-suite-perf-profile-1110.log` separate resolution (0.436 s), type
checking (17.254 s), Core lowering (0.577 s), linking/pruning/verification
(9.598 s) and backend compilation (28.074 s). These are diagnostic timings,
not an execution gate measurement.

`Module::compact_types` verifies the complete input before finding type roots
and their transitive graph, preserves the original order of retained nodes,
remaps all references, and verifies the output. The program driver invokes it
after declaration reachability pruning. The operation does not reuse solver
state or cache checked interfaces. Remaining repeated support checking is still
part of every complete-support compilation.

| ID | Requirement | Evidence | State |
| --- | --- | --- | --- |
| CT-01 | Unused malformed types remain errors | Dangling-reference and cycle tests compare full original diagnostics and require unchanged rejected input | Verified |
| CT-02 | Retained quantified graphs preserve lexical identities | A shifted polymorphic identity retains its quantifiers, expression binder, constructor fields, symbol and spans; second compaction is identical | Verified |
| CT-03 | Source-wide external obligations survive without executable uses | An unused checked foreign signature remains a root and remains required by verification | Verified |
| CT-04 | All expression, binder, pattern, cast and metadata references compose with lowering | Full mandatory workspace: 77 targets, 2373 passed, zero failed, seven ignored | Verified |
| CT-05 | Single-program compilation improves without reducing checks | Frozen baseline `1110`: 57.229 s; final uninstrumented candidate: 24.307 s, both exit zero with `Done` | Verified for measured case |
| CT-06 | Bounded concurrency preserves each program's observations | Four-case serial 96.154 s versus four-worker 27.864 s; identical agreement, exclusions, stages, stdout observations and exit codes | Verified for measured cohort |
| CT-07 | Full official inventory retains prior agreement | Completed 365-root audit: 315/340, exact three recoveries from 312/340, no lost agreement, 25 identical FFI exclusions | Verified for measured inventory |

The complete-support value/order test also executes successfully with all 309
locked modules (`/tmp/psrs-suite-perf-support.log`). Fresh public-CLI builds of
`2941`, `CheckTypeClass` and `ScopedTypeVariables` execute with exact `Done\n`,
empty stderr and exit zero, matching the unchanged official JS under Node.
Their source/component/JS hashes and retained streams are in
`/tmp/psrs-suite-perf-original-execution/comparison.json`.

The four-case cohort is `1110`, `2941`, `CheckTypeClass` and
`ScopedTypeVariables`. Artifacts and exact executable/source hashes are retained
under `/tmp/psrs-suite-perf-cohort-{before,serial,parallel}`; their comparison is
`/tmp/psrs-suite-perf-cohort-comparison.json`. An instrumented candidate separately
observes backend time 0.159 s in
`/tmp/psrs-suite-perf-profile-compact-1110.log`. Timing changes are measurements
on this host, not performance guarantees for arbitrary programs.

The maintained runtime runner is
`crates/psrs-driver/tests/suite/run_runtime.py`; see the
[compiler SOP](../../workflow/compiler-iteration-sop.md#bounded-runtime-measurements).
It uses at most four worker processes by default, independent temporary
directories, retained logs and immutable build-input snapshots. The two Python
runner tests exercise complete-inventory publication and stopping scheduling
after an operational failure. `BigFunction` runs separately after worker exit
under the unchanged 90-second/8-GiB compiler bound; the guest engine remains
outside that address-space bound.

Formatting, full Clippy, all 104 Core tests and the mandatory full workspace
(2373 passed, zero failed, seven ignored) pass. The first replacement
workspace/runtime attempt exhausted the temporary filesystem; its artifacts
are retained as an operational failure and establish no acceptance. After
removing only the completed annotation checkout's debug build cache, replacement
logs use `/tmp/psrs-suite-perf-{workspace-v2,runtime-board-final-v2}.log`.
The replacement driver exits zero and its executed inventory audit passes:
**315/340**, all exits zero, 25 identical FFI exclusions. Exactly `2941`,
`CheckTypeClass` and `ScopedTypeVariables` recover from 312/340, with no lost
agreement. The four-worker run completes in **2022.533 seconds (33m43s)**.
The frozen executable `/tmp/psrs-suite-perf-suite-final` has SHA-256
`032d7275b03bceade76e5fb77e66a0900f6698c201b810e8f3e4c1edcb33debd`.
The complete audit verifies 1183 archived build inputs and all 439 corpus
sources, the 365-root inventory, exact name sets and failure-stage totals.
The other 25 roots are 11 P5 typecheck, 3 P5 kind check, 5 P3 resolve,
1 P8 closure conversion, 2 P0 lex, the TCOMutRec trap, the 2288 guest timeout,
and BigFunction's incomplete allocation verdict (status -6 after 33.805 seconds
under its compiler-only 8 GiB bound). Full evidence is retained in
`/tmp/psrs-suite-perf-runtime-board-final-v2/{measurement,results,summary,acceptance-audit}.json`.
This establishes the measured compaction/runner coverage; the wider L6 runtime
and higher-rank topics remain incomplete.

The final runner also removes an inherited `PURESCRIPT_REPO` override, so a
vendored-corpus measurement cannot silently select another source tree. The
full measurement used the vendored corpus with no override; its exact runner
revision is archived with the evidence. A final two-case run with an explicitly
invalid inherited override uses the same executable, build sources and corpus
hashes and matches the completed full run's observations. Its comparison is
`runner-normalization-comparison.json` beside the complete audit, and both
runner tests also exercise override removal.
