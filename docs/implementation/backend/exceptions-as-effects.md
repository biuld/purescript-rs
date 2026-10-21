# Exceptions as Effects — Execution Checklist

**Design:** [Exceptions as library effects](../../design/backend/fp/exceptions-as-effects.md)\
**Status:** Verified, 2026-10-21. EE-01..EE-17 are complete against the final
validation below. This is the second consumer stage; it does not claim official
`Effect.Exception` support.

## Ownership and starting point

The implementation starts at compiler revision `58b2424c` (merged Control
exception bridge) and adds no production compiler change: the compiler delta is
consumer regressions, the package lock and documentation. The independent
`psrs-stdlib` package carries the implementation, its public cases, tools and
package documentation at commit
`a12e170d8d2922feef2c48a481472e0f2f43ea94`, locked from the compiler as
`fnv1a64-v1:1e15dfb7a7315afa`. The previous locks were
`728a82f94af6e7c54ba10124af54644e8c364d19` / `fnv1a64-v1:5dfa6f2ce6b3092c`
(exception adoption) and `e556bf2f8cb377c049d2ff0a5a97b484c7b36bc5` /
`fnv1a64-v1:7d96f1c44d8d52cc`.

The audit inspected unchanged `Run.Except`, `PSRS.Control`, `PSRS.Control.Row`,
the machine's `perform_scoped`, `enter_target`, `return_finish`, and fresh/shared
Handle entry. Source dispatch removal precedes source lifetime completion;
putting recovery in an outer Bind supplies the required completion cut.

The independent package owns `PSRS.Control.Except`, its public cases, tools and
package documentation. Reuse upstream `Run.Except.Except` and its instance; do
not rewrite official Run/Free algorithms. This repository owns consumer
regressions, compiler fixes if necessary, design and the package lock. There is
no compiler-name recognition for the library API.

## Requirements and evidence targets

Every row below is Verified by executed evidence named in its State column. The
observation names are the package-owned cases under
`conformance/control/except/`, run by `conformance/control-except.mjs`; the test
names are compiler regressions under `crates/psrs-driver/src/tests/`.

| ID | Requirement | Required evidence | State |
| --- | --- | --- | --- |
| EE-01 | Complete public interface and ordinary types | Default/labelled throw, runExcept, catch, rethrow and fromResult; phantom answer at distinct types; polymorphic and aggregate errors; reject mismatched payload, residual row and recovery result | Verified. `PublicApi` covers all eleven exports, `_except`, the re-exported `Except(..)`/Functor, one polymorphic `forall a. Computation (except :: Except Failure) a` thrown into both `Either Failure Int` and `Either Failure { answer :: Int }`, and the aggregate `Failure { code, label }`. `reject-Payload`, `reject-Residual` and `reject-Recovery` each exit 1 in P5 with `TypesDoNotUnify`; `Residual` additionally reports the missing `outer` field. Test `complete_public_api_and_reference_run_agree`. |
| EE-02 | Inert construction, ordinary strict arguments | Construct and discard computations without leaf or recovery callbacks running; distinguish eager argument evaluation from delayed interpretation; observe callback counts | Verified. `Timing` builds `discarded`/`delayed` without running the leaf, shows `rethrow`'s argument strictly evaluated once before interpretation (`strictCount == 1`), and shows the delayed leaf run once per interpretation. `PublicApi` constructs `C.pure 42` and inert failures. |
| EE-03 | Selected failure is abortive | Pending Bind callbacks and source return clause do not run after throw; no dummy request answer or resumption; nested handler abandonment trace | Verified. `ReturnAndAbandonment` observes the source return clause's counter still 0 and the normal path's counter 1; `AbortAndSharing` observes the continuation that would write 99 never running; `ExternalOutput` never emits the failed branch's `unreachable`. |
| EE-04 | Normal and failing answers preserve values | Right/Left constructors and aggregate field values, error reference sharing, exactly one normal return mapping; no stringification or rebuilding of e | Verified. `PublicApi` checks both constructor directions and the aggregate fields; `AbortAndSharing` shares the `cell` reference carried in `e` and observes the write; `ForwardIdentity` observes the identity-carried answer array; `ReferenceRun` shows one normal mapping agreeing with `Run.extract (RE.runExcept (pure 42))`. |
| EE-05 | Label elimination and forwarding are truthful | Two labels with distinct e types; unrelated operation payload and answer identity; exact forwarding order/count; selected label absent and residual label retained by typechecking | Verified. `Labels` nests `inner :: Except String` inside `outer :: Except Int`; `Forward` forwards one residual `Ask` (count 1) and then reaches the selected failure; `ForwardIdentity` forwards twice with the request payload handed to its own answer; `reject-Residual` proves the selected label is absent and the residual row retained by typechecking. |
| EE-06 | Finish precedes recovery | Retained inner raw callback rejects when invoked in recovery; a Resume constructed before completion also rejects when interpreted there; independent outer checkpoint remains usable; recovery runs in the same live root | Verified. `ExpiredRaw` and `ExpiredResume` each trap `unreachable` inside the still-live root, with separate guest invocations; `OuterCheckpoint` resumes an independent outer `Ask` twice successfully. Test `completion_precedes_recovery_for_raw_and_preconstructed_resume`. |
| EE-07 | Recovery is outside the failed handler | Recovery performs a remaining outer-label failure; enclosing handler catches it exactly once; successful body skips recovery; explicit handler reinstatement rather than implicit self-catch | Verified. `Labels` recovers by performing the outer failure, which the enclosing `catchAt` catches exactly once. `PublicApi` reinstates a handler explicitly inside recovery and observes a successful body skip recovery; its delayed-failure case abandons the following `C.pure 99`. |
| EE-08 | Handler order defines pure state/output policy | State outside Except retains updates; State inside Except publishes no state on Left; explicit recovery initialization; analogous Writer nesting with exact returned logs | Verified. `HandlerOrder` checks state outside (1), state inside (no answer), recovery re-initialized at 0, and both Writer nestings. `Oracle` compares all seven policies against values generated by executing unchanged official `Run.Except`/`State`/`Writer` on JavaScript with purs 0.15.16. |
| EE-09 | No implied rollback of shared mutation or IO | Mutated shared cell still contains its write after failure; bytes emitted before failure remain visible; failed-source callbacks emit nothing afterward; recovery output order checked exactly | Verified. `AbortAndSharing` observes the retained write; `ExternalOutput` executes real console output and matches the exact bytes `before\nrecovery\n` with no `unreachable` line; `ReturnAndAbandonment` observes no failed-source callback emission. |
| EE-10 | Replay-local failures preserve other branches | Multi-shot residual handler observes Left in one branch and Right in another; replay-local alias expires at completion; still-live saved template and independent outer checkpoint remain valid until their actual boundary | Verified. `Replay` observes `Left 20` in one branch and `Right 22` in the other with count 2; `ReplayExpired` traps on the expired branch alias; `OuterCheckpoint` keeps the saved template usable. |
| EE-11 | One-shot ownership keeps its own state machine | Acquired callback returns an Except computation and becomes Consumed normally; alias/reuse rejects; unrelated Pending owner is not auto-cancelled; explicit cancel clears it; native callback throw still consumes through the bridge | Verified. `OwnerNormal` (normal Consumed acquisition), `OwnerPending` (unrelated Pending owner still resumable after an unrelated failure), `OwnerSpent` and `OwnerCancelled` (actual `unreachable` traps). The generic one-shot schema is unchanged and the existing bridge tests still cover native callback Consumed propagation. |
| EE-12 | Leaf translation is explicit and correctly timed | rethrow maps both constructors; fromResult invokes leaf only at interpretation, once per replay; returned failure bypasses remainder; strict precomputed Either is distinguished from delayed leaf | Verified. `Timing` distinguishes strict `rethrow` from delayed `fromResult` by observed evaluation counts; `LeafReplay` runs the leaf once per replayed branch; `PublicApi` maps both constructors and observes a returned failure bypassing its remainder. Test `complete_public_api_and_reference_run_agree`. |
| EE-13 | Native channel remains separate from e | Callback/leaf/recovery budget exceptions and unrelated tagged throws never become Left; outer native observer sees original tag/payload behavior; native catch inside callback still works; no Exn erasure | Verified. `NativeBudget` and `NativePositions` observe native budget exhaustion at the leaf, callback, recovery and inner-catch positions as `checkThrows` catches, never as `Left`. Test `native_exceptions_remain_outside_the_typed_failure_channel`, and `tagged::unrelated_native_tag_and_payload_cross_except_to_the_outer_native_observer`, which injects a distinct native tag with payload 42 at the fallible leaf of a real compiled Except program and observes the original tag and payload at the outer native observer. |
| EE-14 | Reject boundaries and separate native support remain honest | Expired/raw/preconstructed Resume and spent-owner uses actually trap; typed Except never recovers native errors or traps; the official native consumer has independent evidence | Verified. ExpiredRaw, ExpiredResume, OwnerSpent, OwnerCancelled and ReplayExpired actually trap. Stage 3 replaces the former unsupported-foreign regression with official_error_values_execute_alongside_typed_failure_effects; [EX-01..18](effect-exception.md) separately own the native Error surface. |
| EE-15 | Shared stage contracts and profile requirements survive | Source/target callable evidence and Finish ancestry retained before/after optimization; no exception-name special case or new intrinsic; disabled exception profile still rejects required runner bridge | Verified. Both the optimized and optimizer-bypassed pipelines execute the full 26-observation suite with identical results, and the bypass retains all regular lowering, machine checks, MIR verification and encoding. Test `exception_effect_rejects_a_profile_without_required_native_cleanup` observes `TargetCapabilities { exceptions: false }` rejected with an `exception-handling` diagnostic. The compiler has no production delta and no exception-name recognition. |
| EE-16 | Source fidelity and package consumption are separate claims | Unchanged pinned official modules, source audit, complete new public API cases, package-owned conformance reports, separately landed package revision and verified compiler lock fingerprint | Verified. 50 upstream packages are cloned at their exact pins and clean; the package-owned audit covers 310 modules (238 identical, 41 modified, 31 target additions, 0 absent upstream modules, 0 detected direct self-recursion); all 309 pre-existing module entries and all 50 pins are byte-identical to the previous lock; the one new module is an explicit `origin: target` addition with SHA-256 `f995fa80d4dadb18ebe7db98fafd68e9f2b4bb217a3f864606cbac7b56c6ae4c`. The package landed as `a12e170`, the stage-2 compiler lock pinned it, and the default consumer path compiles and runs the final `PublicApi` to exit 42. |
| EE-17 | Integration evidence is complete | Required Wasmtime execution before/after MIR optimization, applicable workspace checks, serial runtime board with failure-name/exit audit, and D-04 update from that new measurement; all omissions/skips recorded | Verified. 26 observations execute in both pipelines; the 8 driver regressions pass in the full workspace run and under the bypass build; formatting, `cargo test --workspace` (including 867 passed, 0 failed, 0 ignored in the driver lib target) and Clippy pass; the package's 13 npm tests pass; the serial 365-root board was remeasured and D-04 was updated from it. |

EE-06 must observe the retained checkpoint's failure while the enclosing root is
still live; a trap after root exit alone cannot prove this completion cut.
Separate guest invocations are used for the intentional rejection traps and the
actual trap is inspected. EE-09 executes external output, not only a Writer
result.

## Vertical slices

1. **Library API and elimination (EE-01..05, EE-12).** Complete. The module and
   package-owned source cases use existing operations and the official Except
   datatype; no compiler path was added.
2. **Completion and composition (EE-06..11, EE-13..15).** Complete. Actual
   retained aliases, preconstructed resumes, state/output nesting, replay and
   owner boundaries all execute. No shared invariant required repair, so the
   compiler iteration SOP produced no compiler change.
3. **Consumption and acceptance (EE-16..17).** Complete. The package landed and
   was pinned, focused Rust integration regressions were maintained, the
   compiler/package pair was validated, and the runtime board was remeasured
   serially.

Complete means all required rows are Verified with named tests, checked public
interfaces and actual execution. The bridge's CB-01..CB-15 evidence and existing
Run State/Choose/Except fixtures remain prerequisites and regression coverage,
not acceptance of this new module. This checklist does not close BE-21 and does
not establish official `Effect.Exception`, native-to-effect conversion or
transactional resources.

## Validation protocol

The [package-owned Node tools](../../workflow/stdlib-conformance.md) provide
source fidelity and executable-level runtime comparisons. Compiler and source
hashes, the package fingerprint, expected exit/output/trap and the actual
runtime version are recorded for each run. Pure returned values and traces are
compared against unchanged pinned Run.Except where the public policies
correspond; machine lifetime observations add target-specific evidence and are
not properties of upstream Free itself.

## Final validation evidence

Records are under `/tmp/opencode/stage2/`. Every run below is a real execution
with `PSRS_REQUIRE_WASMTIME=1`; nothing is skipped.

| Claim | Command or artifact | Result |
| --- | --- | --- |
| Compiler workspace checks | `cargo fmt --all --check`; `PSRS_STDLIB_ROOT=/home/biu/Projects/psrs-stdlib PSRS_REQUIRE_WASMTIME=1 cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings` | `workspace/`: format PASS, test PASS (867 passed, 0 failed, 0 ignored in the driver lib target, including all 8 `exceptions_as_effects` regressions), Clippy PASS. |
| Public suite, optimized | `conformance/control-except.mjs` with `/tmp/opencode/stage2/psrs-optimized` (SHA-256 `4c2e2c602b7e9195b517e146382f102f6f91f3545efc8247febdcf24f8a5b9eb`) | `optimized-final/`: 26 observations accepted at fingerprint `fnv1a64-v1:1e15dfb7a7315afa` — 23 executed programs, 5 of them intentional `unreachable` traps, and 3 P5 rejections. |
| Public suite, optimizer bypassed | The same suite with `/tmp/opencode/stage2/psrs-bypass` (SHA-256 `ac145bdefe6949889f47a35fa8cf0b90dbd3652dc957fa3a4b71982a3d207924`), built from a snapshot whose only Rust difference is `crates/psrs-backend/src/mir/opt/mod.rs` returning its verified input unchanged | `bypass-final/`: the same 26 observations, byte-identical runtime observations to the optimized pipeline. |
| Suite audit | `audit-public-final.py` | Every accepted row re-checked: compilation completed, exit 0 for the 18 non-trap programs, exit 42 with empty stderr and only `ExternalOutput` producing exactly `before\nrecovery\n`, `unreachable` traps for `OwnerSpent`, `OwnerCancelled`, `ExpiredRaw`, `ExpiredResume` and `ReplayExpired`, unchanged inputs and package fingerprint. |
| Official oracle | `conformance/control-except-oracle.mjs` with purs 0.15.16 (SHA-256 `8b7eb6529363b88d3d9ed913fbd969399290a29c65a394aead433b12d74a0932`), Node v26.8.1 | `oracle/`: unchanged official Run.Except/State/Writer compiled and executed on JavaScript; 7 observations (`normal`, `failure`, `recovery`, `stateOutside`, `stateSuppressed`, `writerOutside`, `writerSuppressed`) supply the `Oracle` case's goldens. |
| Source fidelity | `conformance.mjs audit` over `lib/` and `upstream-lock.json` against all 50 pinned upstreams, run against the committed package revision | `source-audit-final/` (`vendor_revision` `a12e170`): 310 modules — 238 identical, 41 modified, 31 target additions, 0 absent upstream modules, 0 detected direct self-recursion. `source-fidelity-audit.json` records all 309 old module entries and all 50 pins byte-identical and the single new target module. |
| Package tooling | `PSRS_PRELUDE_ROOT=… npm test` | `npm-test-final.log`: 13 passed, 0 failed, 0 skipped. |
| Locked consumption | `locked-consumption.py` with `PSRS_STDLIB_ROOT` unset | `locked/result.json`: the lock selects `a12e170`, the compiler builds the final `PublicApi` and the guest exits 42 with empty output. |
| Runtime board | `runtime-board.py`, one root at a time, 10-second guest wall, Wasmtime 48.0.1 (`7bac2c277`, 2026-08-24), stack 524288, harness SHA-256 `752e1e344c51ebf33113eb00adbc20d5ca94a315f5f5bda42bd5e797fa6ddebb` | `board/`: 287 of 340 non-FFI programs agree out of 365 roots, 25 FFI groups excluded, every agreeing exit 0, and agreeing/failing name sets identical to the previous board. |

The runtime board is regression evidence for this library, not proof of the
Control machine's interactions; [CB-01..CB-15](control-exception-bridge.md)
separately verify those.

## Recorded environment facts and limitations

- **The corpus board records one environment termination.** `BigFunction.purs`
  again has no compiler verdict: this run's compilation was terminated by the
  kernel OOM killer after 88.3 seconds of wall time, having reached 38.7 GB
  resident memory on a 46 GB machine. That is an environment outcome, not a
  compiler rejection, and it is recorded as such rather than as a timeout
  duration. The earlier bridge run instead hit its requested 90-second outer
  timeout with 165.085 seconds total harness wall; that duration belongs to that
  run.
- **A stale shared build directory was found and removed before the accepted
  workspace run.** The pre-existing `target/debug` contained artifacts compiled
  from a deleted snapshot path, so one `psrs-syntax` test binary resolved the
  vendored layout corpus relative to a directory that no longer existed and
  failed with a file-not-found panic. The local crate sources were rebuilt, and
  the accepted run above is the result. This was a build-hygiene defect, not a
  compiler or test defect. The release harness was separately rebuilt from the
  final tree and produced a byte-identical binary, so the board's binary
  predates the last test-only edit by construction and not by assumption.
- **No production compiler code changed.** `psrs-backend`, `psrs-core` and the
  typechecker are byte-identical to `58b2424c`; the compiler delta is the
  consumer regressions, the lock and documentation. No compiler workaround was
  introduced for the reduced State/Writer fixture signature: official purs
  0.15.16 rejects the abstract `Row.Cons` form as well (`row-official.json`), so
  the fixture states the row extension directly.
- **No frontend board was rerun.** No frontend semantics changed; new syntax and
  new diagnostics are the cases that require official-suite evidence, and this
  change adds neither.

## Design probes

Temporary prototype sources under `/tmp/opencode/exception-effect-design/`
tested the proposed equations against the bridge compiler, without installing a
module into either repository. The initial abort/recovery prototype compiled
and returned 42 under Wasmtime 48.0.1 with the pinned 524288-byte stack limit.
The source uses the real PSRS.Control.Row, VariantF and unchanged Run.Except
datatype; it did not exercise a published PSRS.Control.Except package API.

On 2026-10-20 the package-owned Node `conformance.mjs run` command executed six
additional prototype cases serially through a temporary release-library driver:

| Prototype | Required observation | Observed |
| --- | --- | --- |
| `normal.purs` | Pure source becomes Right 42 | exit 42, empty stdout/stderr |
| `abort.purs` | Throw abandons the callback returning 99; recovery returns 42 | exit 42, empty stdout/stderr |
| `recovery-outer-label.purs` | Inner String failure recovers by performing outer Int failure 42 | exit 42, empty stdout/stderr |
| `forward.purs` | One residual Ask forwards, answers 20 and then reaches the selected failure | exit 42, checked handler count 1, empty stdout/stderr |
| `native-not-converted.purs` | Native recursion exhaustion escapes Except elimination to an outer native observer | exit 42, empty stdout/stderr |
| `expired-before-recovery.purs` | Recovery in the same run attempts a saved inner continuation after source failure | exit 134 with actual `unreachable` trap, accepted by the runner's expected-trap check |

All six reports record `accepted: true`, completed compilation, unchanged inputs
and unchanged stdlib fingerprint. The lifecycle probe captures an inner Save
handler's continuation, then fails through the enclosing Except boundary;
recovery tries to interpret that retained continuation under a new elimination.
Its trap occurs inside the still-live root, rather than after root completion.

Reports are `/tmp/opencode/exception-effect-design/release-<case>/run.json` and
`release-summary.json`; prototype equations are in `Except.purs`. The compiler
entry in `compiler.rs` calls the public
`psrs_driver::compile_program_sources_with_prelude` API and writes the resulting
component. It was linked to the existing bridge release rlib, without changing
any production source or optimizer. Its SHA-256 is
`de9a557fa1be5a17384561a2e6ce04b573604c119cad9130f1888eb3ae8836b9`;
the consumed `libpsrs_driver-977d1d15f5a6c4a0.rlib` hash is
`28764d2e854b25e26d095deda601d77b8065adb6050673c8d631468441af6200`.
The temporary Wasmtime wrapper uses `/usr/bin/wasmtime run
-W max-wasm-stack=524288`; the actual version is 48.0.1 (`7bac2c277`, 2026-08-24).
The wrapper and its hash are retained alongside the reports. For example:

```sh
node /home/biu/Projects/psrs-stdlib/tools/conformance.mjs run \
  --compiler /tmp/opencode/exception-effect-design/compiler-release \
  --stdlib-root /home/biu/Projects/psrs-stdlib \
  --wasmtime /tmp/opencode/exception-effect-design/wasmtime-pinned \
  --input /tmp/opencode/exception-effect-design/Except.purs \
  --input /tmp/opencode/exception-effect-design/expired-before-recovery.purs \
  --expected-trap unreachable \
  --out /tmp/opencode/exception-effect-design/release-expired-before-recovery
```

These probes are the design's feasibility history and are not EE acceptance;
the accepted evidence is the final validation above. Initial sandbox Node spawn
failures were infrastructure failures, not compilation verdicts.

## Implementation progress

The independent package implements the complete public module, including both
label policies, the re-exported upstream Except, and the delayed leaf adapter.
Compiler consumer regressions load the package-owned cases at runtime rather
than copying their source, so the executed cases and the published cases cannot
drift apart. No production compiler or official library source changed.

Package-owned `conformance/control-except.mjs` exercises API, timing,
forwarding, recovery completion, sharing, State/Writer nesting, output, replay
and owner use. `control-except-oracle.mjs` compiled and executed unchanged
official Run policy using purs 0.15.16; its seven observations supply target
goldens. The complete pinned audit covers 50 packages and 310 modules: 238
identical, 41 modified, 31 target additions, no absent upstream module and no
detected direct self-recursion. Existing adaptation classifications are not new
approvals.

The shared pre-existing Cargo target directory could reuse an optimizer-bypassed
backend artifact from bridge validation, so the normal backend and CLI were
forcibly rebuilt from current production sources and copied to fixed
executables before the accepted runs. Both accepted pipelines therefore used a
fixed executable that no concurrent build could alter.

## Not established here

- Official `Effect.Exception` (`Error`, throw/catch foreign bindings,
  diagnostics, native-to-source decoding) remains unimplemented and is a
  separate consumer with its own source-fidelity and behavior evidence.
- No generic effect row, effect inference or new `Prim.Control` operation is
  added; the library is ordinary source over existing operations.
- Failure is not a transaction: shared mutation and already-performed IO are not
  rolled back, and no automatic resource release or cancellation is implied.
- BE-21 and the L6/M7 corpus gate stay Partial; the runtime board does not
  compare output goldens and its failure set is unchanged.

Stage 3 update (2026-10-21): official native Error/throw/catch now have the
[separate implementation contract](../../design/backend/fp/effect-exception.md).
Historical unsupported claims above describe the stage-2 acceptance boundary,
not the current native API. Typed Except still performs no implicit native conversion.
