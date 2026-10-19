# Control Exception Bridge — Execution Checklist

**Design:** [Control and native exception composition](../../design/backend/fp/control-exception-bridge.md)\
**Tracking:** [#156](https://github.com/biuld/purescript-rs/issues/156), Phase 5\
**Status:** CB-01..CB-15 Verified in the current checkout, 2026-10-20. Workspace
validation and the serial runtime-board audit are complete. Existing EH-01..17
evidence establishes the transport/budget adoption; the evidence below separately
establishes this bridge.

## Baseline and ownership audit

Before compiler edits follow the [compiler iteration SOP](../../workflow/compiler-iteration-sop.md).
Record a focused diagnosis baseline and the current revision, stdlib pin,
engine version and stack limit. Inspect the first blocker, diagnostic origin
and last completed stage; test fixtures must not silently bypass those inputs.

The design audit inspected `wasm/lower/budget.rs` (native prologue/return/tail
accounting), `cc/lower/control/plan/driver.rs` (heap Resume), `helpers.rs`
(ancestor liveness checks and cancel), `run.rs` (normal-only root close), and
`handle.rs` (normal-only acquired-owner completion). This is static design
evidence, not a runtime measurement. Saved queues own no native budget charge.

## Requirements and evidence targets

The evidence map below names implemented tests. The final validation record
separates focused results from the full-workspace and corpus runs.

| ID | Requirement | Required evidence | State |
| --- | --- | --- | --- |
| CB-01 | Exception token is disjoint from ordinary erased payloads | CC/MIR positive token transport; reject Any/Eq cast, payload erasure, numeric/ABI use and nullable ThrowRef operand | Verified |
| CB-02 | Catch-all-ref and ThrowRef retain original exception | direct MIR and encoded Wasm; outer named catch observes original tag and reference payload identity after nested propagation | Verified |
| CB-03 | Generic CC protection has truthful exits and binder scope | reject leaked exception binder, body-local handler read, wrong result shape, forged Yield after propagation; permit diverging handler without dummy result | Verified |
| CB-04 | One shared unwind relation owns unknown-tag propagation | nested named/all catches, shadowing and handler rethrow; availability, reachability, constants and structuring consume the same relation | Verified |
| CB-05 | Runner root closes on exceptional exit | source callback throws after capture; outer native guard continues; invoking retained raw continuation rejects through expired ancestry | Verified |
| CB-06 | Normal runner behavior and child lifetime remain valid | normal results, deep/shallow handlers, abandoned child, replay/coalescing and long request sequence; existing AE/AC cases still execute | Verified |
| CB-07 | Acquired owner terminates exceptionally | source callback throws; original token propagates, owner is Consumed, callback slot remains cleared; later resume/cancel rejects | Verified |
| CB-08 | Acquisition rejection does not change status | reentrant Running use and Pending/Consumed/Cancelled matrix; guard failure is the existing trap, no fabricated exception | Verified |
| CB-09 | Native charges do not attach to saved queues | capture/replay/multi-shot/cancel/abandon loops followed by non-tail recursion; same calibrated exhaustion boundary, no counter leak or underflow | Verified |
| CB-10 | Catch restore precedes non-throwing cleanup | prologue-budget throw during callback; direct private-cell stores before propagation, no allocation/call; near-limit execution and malformed-plan mutations | Verified |
| CB-11 | Dynamic catch scope survives composition truthfully | callback catches before runner escape and runner continues; native catch used only while constructing Resume has expired at interpretation; runner guard remains active | Verified |
| CB-12 | Nested runs isolate exceptional lifetime | inner abort caught by outer callback expires inner checkpoints only; propagation through both closes both roots; current-run replay ancestry checked independently of saved checkpoint | Verified |
| CB-13 | Machine construction and emitted body agree | independent model traces normal/exceptional prefixes; remove/reorder close, clear, completion or propagation and require rejection; mutate same-shaped root/owner identities | Verified |
| CB-14 | Capability, CFG and tail constraints remain explicit | disabled-profile rejection for bridge runner, catch-ref and Exn type; malformed catch arity/order/ordinary entries; no tail transfer removes a live guard; flat executor loop remains constant stack | Verified |
| CB-15 | Optimized source execution and board evidence remain separate | execute source bridge cases before/after MIR optimization; serial affected runtime board with failure-set comparison and exit audit; record all skipped cases as unverified | Verified |

For one-shot reuse and expiry tests that deliberately trap, use separate guest
invocations and inspect their actual trap outcomes. Do not claim a native catch
recovers Reject. Test-private bindings may create/probe channel throws without
pretending official `Effect.Exception` is implemented. Observe exception identity
through original tag/payload behavior; the opaque Exn token is not an Eq value.

## Vertical slices

1. **CB-01..04: reference transport.** Generic CC regions, MIR Exn/ordered
   clauses/ThrowRef, shared CFG relation, thin encoding and validators. Include
   positive and malformed inputs at each boundary; no machine lifecycle claim yet.
2. **CB-05..08, CB-10, CB-13: lifecycle composition.** Extend the named schema,
   action registry, independent plan model and boundary checker; guard Run and
   acquired callbacks with direct cleanup stores. No successful-looking Abort
   result or change to Reject.
3. **CB-09, CB-11..12, CB-14..15: integration.** Execute source composition,
   accounting, ancestry and dynamic-scope cases with optimization and pinned
   runtime; audit the full requirement matrix and affected corpus measurements.

## Validation protocol

For Rust changes run formatting, the full workspace suite and Clippy:

```sh
cargo fmt --all --check
PSRS_STDLIB_ROOT=/home/biu/Projects/psrs-stdlib PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Use the pinned engine stack setting `-W max-wasm-stack=524288` for
budget-dependent evidence and record the actual engine version. A test probe
may observe the private counter only in test artifacts; production keeps it
unexported. Boundary equality assertions complement, not replace, execution of
non-tail exhaustion after cancellation and repeated throws.

Follow the existing serial per-root board protocol, with the 10-second guest
wall and annotations oracle where applicable. Do not run runtime boards in
parallel. Remeasure before changing D-04; do not restate the handoff's existing
numbers as new measurements. No frontend syntax/diagnostic work is implied.

## Evidence and completion rule

For every row record owning functions, exact implemented test names, source or
direct-IR boundary, command/environment, runtime version, executed/skipped
cases, tested revision and gaps. Complete means every required row is Verified;
existing transport tests, structural validation and model agreement alone do
not establish lifecycle or native execution correctness.

Bridge completion does not close BE-21, implement official Error/throw/catch,
prove exception-as-effect recovery, or establish engine-trap/foreign-resource
cleanup. Those remain separate consumers/contracts with their own evidence.

## Implementation and focused evidence

All paths below are in `crates/psrs-backend/src/` unless a driver path is given.
The starting revision is `93a5d31ca03a67f6b7173599f65bf452537b08b3` on `master`.
The worktree already contained the preceding bridge design documents; there were
no pre-existing Rust edits. The stdlib pin remains `728a82f94af6e7c54ba10124af54644e8c364d19`
/ `fnv1a64-v1:5dfa6f2ce6b3092c`. No official library source changed.

| Requirements | Implementation | Tests and observed boundary |
| --- | --- | --- |
| CB-01, CB-03 | `cc/assignment.rs`, `cc/representation.rs`, CC region/shape/conversion checks; P9 Exn mapping | `cc::verify::tests::exception`: diverging handler accepted without dummy result; leaked binder, body-only definition, wrong Yield, and erased-token cast rejected. MIR `rejects_nullable_or_aggregate_exception_operands`, capability-only type rejection, and ordinary scalar/ABI validation retain disjointness. |
| CB-02, CB-04 | ordered MIR clauses; shared `mir/try_region.rs` and `mir/unwind.rs`; catch-all-ref/throw-ref encoding | `mir::tests::exception::references::propagates_original_tag_and_payload_through_nested_reference_catch` returns 37; `forwarding_preserves_the_identity_of_a_mutable_reference_payload` writes through the forwarded reference and reads 42 from its original array after optimization; `reference_throw_edges_include_outer_handlers_and_caller_propagation` checks unknown tags and catch-all closure. |
| CB-05, CB-07, CB-13 | checked protected operations in Run and owned callback acquisition; RunAbort/AcquiredCallbackExit registry; independent actual-CC boundary walk | plan model `channel_abort_closes_the_runner_root_and_consumes_acquired_owners` compares independent reference semantics, original exception, cleared callback, Consumed status and root closure. Driver `escaped_raw_continuation_expires_after_exceptional_runner_exit` catches the initial exhaustion then observes a real rejection trap on retained raw use. Driver `exceptional_cleanup_cannot_be_removed_reordered_or_fabricate_an_answer` rejects both Run and owner path mutations. |
| CB-06, CB-08 | normal actions retain their contracts; normal Close can call its helper, exceptional Close expands inline | existing action/status-prefix model tests and the migrated `control_boundary` / `control_regressions` retain ordering, wrong-root, missing-close, status/capture, and normal-transform rejection coverage. Mandatory `control_costs` cases execute long/deep/mixed queues and post-GC ancestry checks. |
| CB-09 | saved queues have zero native charge; cancel never changes depth directly | source `repeated_capture_cancel_abandon_and_throw_do_not_leak_native_budget` completes repeated cancellation/abandonment/exhaustion and subsequent recursion; `saved_queues_and_cancellation_leave_the_same_native_exhaustion_depth` compares entry counts of the same non-tail recursive probe before and after captured-queue cancellation. |
| CB-10 | generic catch restore before direct private-cell cleanup; no guest helper call in exceptional cleanup | direct MIR `near_limit_prologue_throw_can_restore_and_clean_up_without_another_call` leaves 16 budget units, throws from a callee prologue, writes the private status array and returns 42. This fixture deliberately sets synthetic weights to isolate the cleanup boundary; it is not a new calibration measurement. Actual source callback/owned-callback exhaustion cases separately exercise compiler-computed weights. |
| CB-11, CB-12 | one guard per runner; independent root ancestry and local owner guards | source `caught_callback_exception_does_not_abort_the_runner`, `native_catch_during_resume_construction_has_expired_at_interpretation`, `nested_run_abort_can_be_caught_without_expiring_the_outer_checkpoint`, and `replay_in_an_inner_runner_preserves_the_live_outer_saved_checkpoint` all return 42. |
| CB-14 | capability inference from Exn and ThrowRef; region-tail exclusion; one-result catch labels | reference tests reject malformed clauses/arity, nullable tokens and disabled profiles; source `bridge_runners_reject_a_profile_without_native_exceptions` rejects required guards. `rejects_a_missing_or_internal_normal_continuation` checks scope metadata. Source `repeated_owned_resumes_keep_the_executor_loop_outside_callback_protection` executes 100,000 owned resumptions; explicit Try normal exits keep the next executor step outside protection. Existing forced-tail, try/loop, disabled-tail long-control and external-target checks remain required in the workspace run. |
| CB-15 | ordinary optimization retains protection, cleanup and exception operands | source probes use the real frontend and optimized backend; direct MIR forwarding tests optimize before execution. Full workspace and serial corpus evidence is recorded below. |

The source tests are in
`crates/psrs-driver/src/tests/control_exception_bridge/mod.rs`. Their test-only
`caught` declaration uses the already checked `checkThrows` binding; recursion
produces real compiler budget exceptions. No fake successful foreign or new
public Error/throw/catch API is introduced.

### Comparable compile diagnosis

```sh
PSRS_STDLIB_ROOT=/home/biu/Projects/psrs-stdlib PSRS_REQUIRE_WASMTIME=1 \
  cargo run -p psrs-cli -- diagnose --corpus passing --filter TCOMutRec \
  --limit 1 --trace --out /tmp/opencode/bridge-before.json --timeout 30
```

The baseline contains one accepted program, no diagnostic/first blocker, and
complete pass capture through component production, validation and WAT printing.
Node lineage and canonical artifact comparison remain unavailable in the trace
contract. This is compile acceptance only: TCOMutRec's upstream-specific runtime
assertion remains a separately measured failure. The final validation reran the
same cohort and compared it with this baseline.

### Initial implementation validation (2026-10-20)

The serial validation job writes full evidence under `/tmp/opencode/bridge-final/`.
The serial run passed `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings` with the validation
environment above. Backend unit tests: 602 passed; driver unit tests: 856 passed.
The source bridge tests and all runtime requirements used required Wasmtime;
no bridge runtime row was verified by a skip. Workspace integration tests,
including the pinned Run/labelled-row reference cases and doctests, also passed.

All 11 source bridge tests also passed before MIR optimization on 2026-10-20
(0 failed, 0 ignored). A validation-only source snapshot at
`/tmp/opencode/bridge-unoptimized` differed in exactly one Rust file:
`mir/opt/mod.rs` retained its input verification and returned the input module,
bypassing every MIR optimization transformation. Core optimization and normal
P9/P10 lowering, capability checks, encoding, validation and componentization
remained active. The production checkout was unchanged. The command was:

```sh
PSRS_STDLIB_ROOT=/home/biu/Projects/psrs-stdlib PSRS_REQUIRE_WASMTIME=1 \
  CARGO_TARGET_DIR=/home/biu/Projects/purescript-rs/target \
  cargo test --manifest-path /tmp/opencode/bridge-unoptimized/Cargo.toml \
  -p psrs-driver --lib control_exception_bridge -- --nocapture
```

The evidence directory retains `unoptimized-source.log`,
`unoptimized-source.patch` and `unoptimized-source-evidence.json`, including a
source-difference audit and log hash. The ordinary optimized source tests passed
in the full-workspace run above; the snapshot is supplementary differential
execution evidence, not the production configuration.

The comparable diagnosis reports `compatible_cohort: true`, identical observed
environment values, unchanged compile acceptance, and no observed pass-sequence
difference. Canonical artifact comparison and embedded build-toolchain identity
remain unavailable; no artifact-equivalence claim is made. The after compiler's
binary fingerprint is `fnv1a64:1ff1df9653d6cd99`, and its worktree fingerprint is
`fnv1a64:8e4228add9606848` (the implementation plus documentation at diagnosis).

The serial release runtime board completed on 2026-10-20: **287/340** non-FFI
programs agree, all with exit code 0; 25 FFI roots are excluded from 365 roots.
The agreeing and failing name sets are identical to `/tmp/opencode/eh17/final.raw`.
The 53 non-agreeing roots comprise 29 P5 typecheck, 13 P5 kind check, 5 P3
resolve, 2 P0 lex, 1 P8 closure conversion, the existing TCOMutRec assertion
trap, the 10-second 2288 guest timeout, and incomplete BigFunction compilation.
BigFunction received SIGKILL after 76.19 seconds, before its 90-second outer cap;
the cause is unconfirmed. It has no compiler verdict and remains unverified.
The raw aggregation omitted that missing-summary row; `audited-summary.json`
explicitly counts it in the 340 denominator without inventing a diagnostic or
timeout. All 365 roots are accounted for; no other harness invocation failed.
This corpus agreement is regression evidence, separate from the bridge's
executed source and direct-IR cases.

The harness was built with `cargo test -p psrs-driver --release --test suite
--no-run`; each root then ran `runtime::l6_runtime_scoreboard --exact --ignored
--nocapture` with `PSRS_SUITE_FILTER=passing/<root>.purs`,
`PSRS_ORACLE=annotations` and the required stdlib/Wasmtime environment above.
One root ran at a time, with the unchanged 10-second guest wall and pinned
524288-byte engine stack. Wasmtime was 48.0.1 (`7bac2c277`, 2026-08-24).
The release harness SHA-256 was
`752e1e344c51ebf33113eb00adbc20d5ca94a315f5f5bda42bd5e797fa6ddebb`.
Per-root logs, `results.json`, `measurement.json`, `audited-summary.json`,
workspace logs and the diagnosis comparison remain under the evidence directory
above. L1–L5 boards were not rerun: this changes backend transport and lifecycle,
not frontend syntax or diagnostics. No official `Effect.Exception` acceptance,
engine-trap recovery, IO rollback or exception-as-effect handler is claimed.

### Issue closure audit (2026-10-20)

The closing audit adds three source/guest tests in
`crates/psrs-driver/src/tests/control_exception_bridge/lifecycle.rs`:

- `exceptional_owner_is_consumed_and_no_longer_retains_its_callback` appends
  read-only observations to the ordinary source function after its outer native
  catch. Machine helpers and exceptional cleanup are unchanged. The guest reads
  the actual owner status 2, observes that the erased callback slot contains a
  status-array reference rather than the callback closure, and returns 84
  (status 2 times the source's caught answer 42). A still-Running owner would
  return 42, so a later rejection alone cannot satisfy this observation.
- `exceptional_owner_rejects_both_resume_and_cancel_in_a_fresh_runner` executes
  each illegal later action in a separate guest and checks its actual
  `unreachable` trap. The owner callback captures no raw checkpoint, so an
  expired source continuation cannot substitute for the owner-state rejection.
- `an_exception_crossing_two_runners_expires_both_saved_checkpoints` retains a
  raw checkpoint in each of two nested runs, lets one channel exception cross
  both guards, then probes each retained checkpoint in a fresh guest. Both
  attempts actually trap. The existing inner-caught/outer-live test separately
  checks that local recovery does not expire the outer checkpoint.

These strengthen CB-07 and CB-12 beyond the independent model and construction
checks. CB-08 also retains the full `control_owned_runtime` integration cases,
including `reentrant_interpretation_rejects_a_running_owner` and
`reentrant_cancellation_rejects_a_running_owner`; normal and prebuilt checkpoint
expiry remains covered by `control_lifetime`. The module root moved to `mod.rs`
to satisfy the repository's source-layout check. No production backend code,
standard-library source or package pin changed during this closing audit.

The closing run passed `cargo fmt --all --check`, required-Wasmtime
`cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`.
Backend unit tests: 602 passed; driver unit tests: 859 passed. All workspace
integration tests and doctests passed. The 14 bridge source tests also passed
with MIR optimization bypassed (0 failed, 0 ignored). The updated validation
snapshot differs in exactly the optimizer entry already described above;
new tests and the module move are copied unchanged from the candidate.

The closing run's logs, snapshot-difference audit and per-root runtime reports
are under `/tmp/opencode/issue156-evidence/`. Its release board reuses the exact
harness SHA-256 recorded above: no production backend code or stdlib changed.
The closing serial board again measured **287/340**, with 25 FFI roots excluded
from 365 roots and every agreeing exit code 0. Agreeing/failing name sets and the
stage decomposition are unchanged from the preceding measurement: 29 P5 type,
13 P5 kind, 5 P3 resolve, 2 P0 lex, 1 P8 closure conversion, TCOMutRec's assertion
trap, 2288's 10-second guest timeout, and BigFunction without a compiler verdict.
The closing harness requested a 90-second outer timeout for BigFunction and
observed 165.085 seconds total wall, including timeout termination. The timeout
is an outer harness outcome, not a compiler diagnostic or successful compilation.
The earlier SIGKILL after 76.19 seconds remains a separate historical observation.
No missing-summary row is dropped from the 340 denominator. `summary.json` and
`results.json` retain all 365 roots, exit audits and failure-set comparisons.
All CB-01..CB-15 requirements now have the focused/model/IR evidence above and the
completed closing validation; neither downstream consumer API is claimed.
