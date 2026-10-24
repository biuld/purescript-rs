# Console timer execution checklist

Tracking: [issue #160](https://github.com/biuld/purescript-rs/issues/160).
Governing contracts: [WASI platform library](../../design/backend/wasm/wasi-platform-library.md),
[canonical ABI](../../design/backend/wasm/canonical-abi-and-wit.md),
[primitive FFI](../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md), and
[scalar storage](../../decision/DEC-16-scalar-strings-and-utf8-storage.md).

Status: local implementation and acceptance complete; publication and merge pending.
The broader runtime gate remains Partial.

## Baseline

Compiler `515e6a3003994e0a61e7d88e9c7c1b48a5b88f85`, locked library
`618bede18e4bf7fc57206c057781a12fcef2b3b4` (`fnv1a64-v1:44b22a61d1b2b19e`).
The timer reproducer first stops at P8 library linking: `Effect.Console.time`
has no target implementation. The existing `Int` clock ABI retains only the
low 32 bits of nanoseconds. A top-level `unsafePerformEffect (Ref.new 0)` is
re-evaluated at each global reference: writing 42 and reading through another
reference returns 0. Timer storage must not assume source global memoization.
Baseline reports, executable and reproducer inputs are retained under
`/tmp/psrs-console-timers/baseline/`; these local artifacts are investigation
records, not portable acceptance evidence.

## Required vertical slices

- [x] Exact `{ high :: Int, low :: Int }` carrier for both canonical s64/u64,
  alongside the existing Int ABI. Exercise direct, indirect, aggregate, erased
  record storage and list paths, in both directions, against independent hosts.
- [x] Closed monomorphic per-binding static cell. Validate before erasure,
  preserve array identity, reject polymorphic or inconsistent declarations,
  verify physical storage, preserve effects during optimization, isolate
  bindings and component instances, and reject missing target capabilities.
- [x] Ordinary library timer registry and word arithmetic over checked
  primitives. Constructing an action is inert; repeated execution and separate
  native interpreter runs share the intended registry.
- [x] Official public timer signatures and lifecycle: duplicate starts retain
  the original timestamp, log retains a timer, end removes it, missing labels
  warn, restart succeeds. Stream-only clear emits nothing and preserves timers.
- [x] Library-owned pinned JavaScript/Wasmtime conformance. Retain actual
  outputs; compare labels, streams and lifecycle separately from elapsed values.
  Include Unicode, empty and prototype-like labels, deterministic word-borrow
  arithmetic, and a real duration exceeding one 32-bit nanosecond rollover.
- [x] Source-fidelity audit, tool tests, compiler lock, required workspace
  validation, full affected runtime scoreboard, and consistent D-04 evidence.

The static primitive is not a general source-global memoization mechanism.
Effect.Aff and broader BE-21 obligations remain outside this slice and open.


## Focused execution evidence

The library is committed locally as
a revision recorded in `stdlib.lock.json`, with fingerprint
`fnv1a64-v1:2889a7ea7d8b0dc2`. Its four-case library-owned engine accepted five
actual Wasmtime executions against pinned Console.js v6.0.0
(`3b83d7b792d03872afeea5e62b4f686ab0f09842`), using Node 26.8.1 and
Wasmtime 48.0.1. The final report is
`/tmp/psrs-console-timers/conformance-frozen-final/report.json`, with compiler executable
SHA-256 `e4ca9005866a9ddd2dc3924f221eea947bbfb58997e8636d09f297b203f04637`.
The long case observes 6004.982494 ms at log and 6005.427939 ms at end, including
an intervening duplicate start; both exceed one low-word rollover. The engine
also requires the elapsed values to fit its actual process timeout.

The source audit covers 50 pinned packages and 312 modules: 237 identical,
42 modified and 33 target additions, with no absent upstream modules and no
direct self recursion. Console retains all nine foreign-slot signatures as
nonrecursive implementations; no original code outside value-FFI slots changed.
All 19 library tool tests pass. Audit artifacts are retained under
`/tmp/psrs-console-timers/source-audit-final/`.

Compiler regressions execute exact-bit scalar round trips (both s64/u64 and
sign/precision edges), indirect parameters, variant payloads, typed arrays and
arrays of labelled erased records. The list round trip also verifies that
sequential store/load/free operations reset their shared loop index. Static
storage tests execute shared and independent bindings and reject invalid unused
source contracts, incompatible physical storage, conflicting binding layouts
and missing mutable-global support. Broader locked-package workspace and
scoreboard validation is recorded below.

The exact clock wrapper is owned by `WASI.Clock.nowWords`; the timer module
contains no raw WASI import. The service-module export guard and a live program
mixing compatibility `now` with `nowWords` pass, confirming that both source
views compose over the same canonical WIT import without exposing private
foreign leaves.


## Workspace validation

`cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D
warnings` pass. `PSRS_REQUIRE_WASMTIME=1 cargo test --workspace` completes with
77 targets, 2385 passed, zero failed and seven ignored suite boards. This is
the full workspace suite with execution skips promoted to failures; a separate
non-mandatory repeat is not needed. The final library revision only refreshes
measurement documentation relative to the tested `9948f80`; `lib`,
`conformance`, `manifest.json` and `upstream-lock.json` are byte-identical, with
fingerprint `fnv1a64-v1:2889a7ea7d8b0dc2`.

The final CLI is frozen at `/tmp/psrs-console-timers/psrs-closed-carrier-final`,
SHA-256 `e4ca9005866a9ddd2dc3924f221eea947bbfb58997e8636d09f297b203f04637`.
The runtime harness is `/tmp/psrs-console-timers/suite-final`, SHA-256
`3af130b4d6c550ad082de12f2b00023c4b5e20ffecefdced73ee716ef90803d3`.
The runtime runner's two operational-failure tests pass. Full gate remeasurement
is recorded below.


## Full gate acceptance

The frozen four-worker runner exits zero. Its complete retained-artifact audit
also exits zero: all 365 roots, 439 corpus sources and 1197 archived build inputs
match their recorded hashes. Agreement is 315/340, all exiting zero; the exact
agreement and 25-name FFI-exclusion sets match the preceding annotation/compaction
measurement. There are no gained or lost cases. The complete run takes
1979.763 seconds, with the same checking scopes, ten-second guest limit,
524288-byte stack, 110-second normal outer limit and compiler-only BigFunction
90-second/8-GiB bounds.

The remaining 25 are 11 P5 typecheck, 3 P5 kind check, 5 P3 resolve,
1 P8 closure conversion, 2 P0 lex, the TCOMutRec assertion trap, the 2288 guest
timeout and BigFunction's incomplete compiler result. BigFunction observes an
allocation failure after 34.612 seconds (status -6), not a successful compile
or a measured runtime result. These gaps remain open.

Artifacts: `/tmp/psrs-console-timers/runtime-board-final/{measurement,results,
summary,acceptance-audit}.json`; terminal log:
`/tmp/psrs-console-timers/runtime-board-final.log`; independent audit:
`/tmp/psrs-console-timers/audit-runtime.py`. The previous baseline is
`/tmp/psrs-suite-perf-runtime-board-final-v2/results.json`.
No passing corpus root imports the timer/clear names, so #160's corpus count
remains zero and its new behavior is established by focused library conformance.
Effect.Aff and broader BE-21/BE-27 acceptance remain Partial/open.
