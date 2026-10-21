# Official Effect.Exception execution checklist

**Design:** [Official exceptions](../../design/backend/fp/effect-exception.md),
[DEC-24](../../decision/DEC-24-effect-exception-error-representation.md).\
**Status:** EX-01..EX-18 Verified, including the protected-self-call review correction.

## Requirement-to-evidence map

All execution rows require actual Wasmtime execution. Compiler tests use
PSRS_REQUIRE_WASMTIME=1; package commands never skip an unavailable runtime.

| ID | Requirement | Evidence | State |
| --- | --- | --- | --- |
| EX-01 | Complete official public signatures/exports and non-orphan Show instance | Pinned source diff; PublicApi; unchanged throw/try, Unsafe and MonadError instances | Verified |
| EX-02 | Abstract GC message representation with target justification | DEC-24, package adaptation, PublicApi Unicode/record results | Verified |
| EX-03 | Message, default name, stackless Show including empty message, Nothing stack | ShowNameStack; retained rank-2 stackImpl; seven official JS goldens | Verified |
| EX-04 | Exactly two new checked primitives; ordinary library wrappers | Native module and P9 inventory; no host imports in catch_discharge | Verified |
| EX-05 | Source and physical contracts reject malformed/unused declarations | bindings::runtime::exception tests; catch_discharge physical rejections; driver malformed_unused_source_exception_declarations_are_rejected_before_erasure | Verified |
| EX-06 | One source tag iff live throw/catch; checkThrows-only adds none | catch_discharge::unused_throw_declarations_are_checked_without_planning_a_source_tag and named-catch inspection | Verified |
| EX-07 | Diverging throw does not fabricate/read a normal destination | throw_destination_probe: 36 observed executions, scalar/reference results, branches, switches and protection, MIR optimized/bypassed | Verified |
| EX-08 | Named catch discharges and remaps the real String payload | catch_discharge::named_catch_receives_the_real_string_payload_after_type_remapping; ThrowCatch; probe3 Unicode exit 42 | Verified |
| EX-09 | Source catch excludes exhaustion, unrelated tags and traps | StackExhaustion exits 1; driver unrelated-tag payload exits 42; assertion trap test | Verified |
| EX-10 | Construction inert; handlers armed during interpretation; repeated effects/order preserved | Ordering exact stdout and reference values | Verified |
| EX-11 | Direct native errors compose with Control and typed Except without conversion | ControlCompose; existing CB-01..15 preserve generic exception token and lifecycle | Verified |
| EX-12 | Uncaught source errors exit 1 without fabricated success | Uncaught independent execution and driver regression | Verified |
| EX-13 | Disabled profile rejects live operations explicitly | catch_discharge physical test; source_exception_calls_reject_a_disabled_exception_profile | Verified |
| EX-14 | Official MonadThrow/MonadError Effect and Unsafe execute | MonadError, Unsafe; original official source retained | Verified |
| EX-15 | Independent pinned audit and source contract | 50 packages, 311 modules: 237 identical/42 modified/32 target, no absent module or direct self-recursion | Verified |
| EX-16 | Public conformance and independent JS oracle | Ten mandatory runtime observations, seven purs 0.15.16 JS goldens, 14 Node tool tests | Verified |
| EX-17 | Committed package selected by compiler lock | Package 618bede18e4bf7fc57206c057781a12fcef2b3b4, no development override; PublicApi compiles and exits 42, empty output; locked-reviewed-result.json | Verified |
| EX-18 | Workspace integration, serial board and truthful D-04 | Corrected fmt/workspace/Clippy pass; 2247 tests, five ignored boards; serial 365-root board 287/340, 25 excluded, unchanged failure/exit sets | Verified |

## Evidence locations and probe findings

`/tmp/opencode/stage3/` retains source-contract-tests.log,
catch-discharge-tests.log, driver-tests.log, npm-test.log, runtime/summary.json,
oracle/oracle.json and source-audit/inventory.json. The driver focused run
passed ten tests with mandatory Wasmtime. Package evidence records the exact
compiler/runtime/source/artifact digests and byte-exact output.

Probe 1 required normal exits to respect an already terminated block and local
throw handlers to use the complete shared region partition. Probe 2 retains
rank-2 stackImpl: probe2-after.json passes source diagnosis and probe2.wasm exits
42. Probe 3 passes a real Unicode String through the named catch, exiting 42;
the direct CC regression also validates remapped payload types before execution.
Probe 4 passed the full workspace run without golden rebaselining after the
trusted-module insertion. One source-layout check required moving the provider
module root into exception/mod.rs after adding its tests directory.

The package lock is fnv1a64-v1:44b22a61d1b2b19e. Its own docs deliberately do
not name their own revision. The final source audit in source-audit-final/inventory.json names that
landed package revision; counts remain unchanged from the development audit.

## Remaining boundaries

No source stack trace, JS Error object, JS TDZ behavior, Aff, engine-trap recovery,
rollback, automatic resource release or entry-level exception printing is claimed.
BE-21 and the runtime gate remain partial. The separate stage 1/2 review is
complete and recorded in its linked report.

Pre-review workspace validation (2026-10-21): cargo fmt --all --check, mandatory
Wasmtime cargo test --workspace, including doc-tests, and cargo clippy
--workspace --all-targets -- -D warnings all passed serially. The workspace
reports 2246 passed and five ignored corpus scoreboards; the ignored runtime
board is remeasured separately below. Logs: fmt-final.log, workspace-final.log
and clippy-final.log under /tmp/opencode/stage3/.

Review then found a protected-self-call tail-rewrite defect and strengthened
a legacy catch-path test oracle. The corrected validation uses
workspace-reviewed.log, clippy-reviewed.log and the fresh board-reviewed/
measurement. Those final corrected runs are complete as recorded below.

## Final corrected acceptance

All EX rows are Verified on 2026-10-21. Corrected logs are fmt-reviewed.log,
workspace-reviewed.log, clippy-reviewed.log and runtime-reviewed/summary.json.
The full workspace reports 2247 passed, zero failed, five ignored corpus
scoreboards; its doc-tests pass. The ten public package executions all pass
with psrs-reviewed, and no-override locked consumption again exits 42 with
empty output (locked-reviewed-result.json). Source audit and Node tools are
unchanged from the committed package evidence above. The public-API rank-2
stack probe and trusted insertion require no fallback or golden rebaseline.

The final release harness was rebuilt after the shared tail correction.
board-reviewed/measurement.json records its digest, profile, stack limit and
lock. Its serial 365-root run yields 287/340 with 25 excluded; every agreeing
exit is zero, and both name sets are unchanged from stage 2. The 53 remaining
cases decompose into 29 type errors, 13 kind errors, five resolution errors,
one closure-conversion failure, two lexing differences, TCOMutRec's assertion
trap, 2288's guest timeout and BigFunction's incomplete compilation.
BigFunction's compiler is limited to 8 GiB address space; the runtime wrapper
removes that compiler-only limit for engine reservations, so runner self-check
executes normally. Actual compiler allocation failure is retained as no verdict,
not a source rejection. The earlier board's discarded inherited-limit runner
failure is preserved separately and contributes no compilation evidence.
No stage-3 runtime observation was skipped. D-04 and README use this final run;
no broader gate is closed. The initial pre-review board remains separate in
board/ and is not the final corrected artifact measurement.

The [stage 1/2 review](stages-1-2-review-2026-10-21.md) records the two fixed
findings and complete typed-Except/official-JS revalidation.

The optional explicit frontend purs differential and the L1-L5 scoreboards were
not rerun; this topic adds no new PureScript syntax or language diagnostic.
Their measurements remain unchanged. The library JS oracles above are separate
executed evidence, not a frontend differential claim.
