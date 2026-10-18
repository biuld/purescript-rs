# Exception Handling and Stack Exhaustion Implementation Checklist

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Exception handling and stack exhaustion](../../design/backend/fp/exception-handling-and-stack-exhaustion.md)

**Progress:** EH-01 through EH-08 Verified (slice 1: capability gate and
encoding; slice 2: the MIR representation). EH-09 through EH-17 Unverified.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), row **BC-08** (the exceptions family) and the runtime board rows that reference `Test.Assert.checkThrows` (#95's blocker, the L6/M7 `P8 closure/linking` failure row for `TCOMutRec.purs`). Status claims in D-04 land with the evidence of the slices below, never from this checklist alone.

## Scope and dependencies

Implement the linked design: the exception channel (tags, `try_table`, `catch`/`catch_all`, `throw`) through MIR, structuring, and encoding, the compiler-managed stack-exhaustion budget with its watermark contract, and the `Test.Assert.checkThrows` primitive that consumes both. Follow the design's Code map; the decisions behind the design are [DEC-23](../../decision/DEC-23-wasm-exception-channel.md) (which amends [DEC-05](../../decision/DEC-05-wasmtime-feature-set.md)). Exceptions stay out of CST, AST, HIR, THIR, and Typed Core. Out of scope: stack switching, `exnref`/re-raise, the FFI group `4179.purs`, re-pointing `assert'` failures, and `Effect.Exception`'s full surface — each recorded as future work in the design.

The stdlib change (`Test/Assert.purs`) happens in the **independent `psrs-stdlib` repository** under [its vendoring contract](../../workflow/stdlib-vendoring.md); this checklist's slice gates on it and records the adaptation evidence here.

## Vertical slices

Implement in this order; each slice is one reviewable unit whose evidence makes its rows verifiable.

1. **Capability gate and encoding** (EH-01–EH-04): thin IR and encoding with hand-built fixtures and execution evidence, plus the disabled-profile rejection.
2. **MIR representation** (EH-05–EH-08): tags, terminators, verifier rules, capability inference.
3. **Structuring integration** (EH-09–EH-11): the try unit in the region plan, emission, and rejection boundaries.
4. **The budget mechanism** (EH-12–EH-15): weights, counter, watermark, entry catch_all, coordination fixture.
5. **The library surface** (EH-16): the `psrs:runtime-exception` provider, `checkThrows`, and the stdlib adaptation, ending with TCOMutRec execution.
6. **Profile flip and board remeasure** (EH-17): the final adoption revision, gated on all evidence above.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**;
record exact reproducible evidence for every Verified row.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| EH-01 | The thin IR models `Op::Try` with catch clauses, `TagDef`, and the `TagIndex` domain; leaf opcodes stay `wasm_encoder::Instruction`. | Backend unit tests construct/inspect the nodes; the thin-IR verifier accepts them and rejects out-of-range tag indices and unresolved clause label depths. | Verified |
| EH-02 | The encoder emits the tag section (id 13) between memory and global; tags are declared only when used. | Binary/WAT regression tests: a tag-using module encodes with the tag section in position; a module without tags emits none; wasmparser accepts under the enabled profile. | Verified |
| EH-03 | The validator is built from the profile so `exceptions` gates the encoding; no other feature is enabled implicitly. | wasmparser rejects a tag-using module built with `exceptions: false` and accepts it enabled; the independent-field test still passes. | Verified |
| EH-04 | Hand-built direct-MIR/wasm fixtures execute under wasmtime: `throw` + named `catch` with a payload, `throw` + `catch_all` payload-less, unwinding through nested call frames. | `PSRS_REQUIRE_WASMTIME=1` execution asserts caught values/results and exit codes. | Verified |
| EH-05 | MIR defines `Tag`/`TagId`, `Module::tags`, and the `Throw`/`Try` terminators with the documented fields. | Constructor inspection plus malformed fixtures: missing tag, wrong payload arity/type, unknown handler block. | Verified |
| EH-06 | The MIR verifier checks unwind edges: `Throw` handlers resolve to the innermost enclosing region's matching handler (or `None` outside every region); handlers have no normal predecessors and belong to exactly one `Try`; regions nest or are disjoint; no back edge targets a try block. | Negative fixtures for each rule, each diagnostic carrying the offending span. | Verified |
| EH-07 | No protected block ends in a forced `ReturnCall`/`ReturnCallRef`; `mark_tail` skips calls in protected blocks. | A protected-region fixture stays an ordinary call plus return under the enabled profile; a hand-written forced tail terminator in a protected region is rejected. | Verified |
| EH-08 | The MIR capability verifier requires `exceptions` when a tag exists or `Throw`/`Try` appears, and rejects a module requiring it under a disabled profile with a source-associated diagnostic. | Required-set inference tests; disabled-profile rejection diagnostic; `wasmtime_wasi_0_2()` unchanged in this slice. | Verified |
| EH-09 | `mir/lower/exception.rs` discharges a call to a `psrs:runtime-exception` binding into the inline try region; when the profile disables `exceptions` the call is rejected at the binding's span. | Direct-MIR discharge fixtures execute under wasmtime (catch returns the handler's value; normal path the protected value). | Unverified |
| EH-10 | The structurer's region plan gains `UnitPlan::Try` with the dominance carve-out; try regions nest or are disjoint; reducible try-free functions are unchanged. | Structuring fixtures: nested tries, try inside loop, loop inside protected region; no dispatcher local on reducible input; WAT inspection shows `try_table` at the expected nesting. | Unverified |
| EH-11 | A function whose CFG is irreducible and contains a try region is rejected with a diagnostic; try-free irreducible input still uses the dispatcher. | Direct-MIR fixture pair: the rejection and the unchanged dispatcher fixture. | Unverified |
| EH-12 | `mir/budget.rs` fixes the weight formula and constants; `Function::frame_weight` is recorded at P9. | Unit test for `frame_weight`; MIR inspection shows the weight on each function when the profile enables `exceptions`. | Unverified |
| EH-13 | Every lowered function gains the prologue check and the epilogue decrement; `return_call*` sites decrement; trapping blocks do not. | WAT inspection: prologue before the first block, decrement on each return/tail path; a trapping block has no decrement; deep tail recursion still exits 0 (no false throw). | Unverified |
| EH-14 | Every `try_table` is preceded by a watermark save and its catch path begins with the restore. | WAT inspection per try region; the leak fixture (a throw/catch loop followed by deep recursion) executes with the deep recursion reaching the budget point, proving no leak. | Unverified |
| EH-15 | Budget/limit coordination: with the engine stack limit pinned via `-W max-wasm-stack` (CLI) or `Config::max_wasm_stack` (Rust API), deep non-tail recursion under `checkThrows` yields the compiler's exception, never `call stack exhausted`. | The coordination fixture executes under the pinned engine and limit; the recorded values (budget, weights, limit, engine version) are written into this record and the capability-profile notes. Failure as a trap means the constants are wrong and the row stays unverified. | Unverified |
| EH-16 | `Test.Assert.checkThrows` is implemented end to end: checked `psrs:runtime-exception#checkThrows` contract, P9 discharge, stdlib adaptation with its recorded target-adaptation entry, and `TCOMutRec.purs` executing (all four `assertThrows` catches, the four depth-100 `assertEqual`s, exit 0). | The corpus case executes under `PSRS_REQUIRE_WASMTIME=1`; the stdlib adaptation record names the pinned upstream declaration and the diff. | Unverified |
| EH-17 | The stable default profile enables `exceptions` (with the budget) as the adoption's final revision; [DEC-05](../../decision/DEC-05-wasmtime-feature-set.md)'s amendment and the capability-profile default table are updated with the evidence. | The full L6/M7 runtime board is re-run by the scoreboard protocol and the new number recorded in D-04; the disabled-profile rejection evidence and the enabled execution evidence are both cited in the profile's implementation notes. | Unverified |

## Slice 1 record — capability gate and encoding (EH-01..EH-04)

The MIR lowering and the structurer do not produce try regions yet (EH-05+),
so every fixture constructs the thin Wasm IR directly. No MIR, structuring,
frontend, or stdlib change belongs to this slice; the thin-IR verifier's
clause checks also cover the tag-index and label-depth rules the design's
structured verifier names (`wasm/verify.rs`: tag index range, catch-clause
label depth resolution, and unwind-block result agreement with each clause's
payload).

```text
EH-01:
  Implementation: crates/psrs-backend/src/wasm/mod.rs (`Op::Try`, `CatchClause`,
    `TagDef`, `TagIndex`, `Module::tags`);
    crates/psrs-backend/src/wasm/verify/mod.rs (module-level tag checks,
    function-type resolution) and crates/psrs-backend/src/wasm/verify/body.rs
    (the structured-body walk: clause tag range, clause label depth, clause
    payload agreement, try-region internal label in branch depths).
  Tests: crates/psrs-backend/src/wasm/tests/exception/mod.rs —
    accepts_try_regions_with_resolved_catch_clauses,
    rejects_an_out_of_range_throw_tag_index,
    rejects_an_out_of_range_catch_clause_tag_index,
    rejects_an_unresolved_catch_clause_label_depth,
    rejects_a_catch_clause_label_result_that_mismatches_the_payload,
    counts_the_try_region_internal_label_in_branch_depths,
    rejects_malformed_tag_definitions.
  Input boundary: well-formed and malformed direct thin IR (no lowering).
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
    (subset: cargo test -p psrs-backend --lib wasm::tests::exception)
  Result: pass, 7/7 executed locally under the workspace run above.
  Revision: this slice's commit.
  Gaps: none for the row; the tag section is never emitted by the lowering
    until EH-05+, so no lowered module reaches these checks yet.

EH-02:
  Implementation: crates/psrs-backend/src/wasm/encode.rs — the tag section
    (id 13) between the memory and global sections, `Op::Try` emission
    (`try_table` with an empty block type, clauses, `end`).
  Tests: crates/psrs-backend/src/wasm/tests/exception/encoding.rs —
    encodes_the_tag_section_between_memory_and_global (wasmparser payload walk
    pins memory < tag < global and the enabled-profile validator accepts),
    encodes_no_tag_section_without_tags, prints_the_tagged_module_as_wat.
  Input boundary: direct thin IR with and without tags.
  Commands: as EH-01 (subset: cargo test -p psrs-backend --lib
    wasm::tests::exception::encoding).
  Result: pass.
  Revision: this slice's commit.
  Gaps: none.

EH-03:
  Implementation: the pipeline's validator is already built from the profile
    (`crates/psrs-backend/src/pipeline/emission.rs` calls
    `validator_for(target)` on the assembled binary); `capability.rs` already
    maps `exceptions` to `WasmFeatures::EXCEPTIONS`. This slice adds no
    implicit feature and leaves both untouched.
  Tests: crates/psrs-backend/src/wasm/tests/exception/encoding.rs —
    exceptions_gate_the_tag_using_module (validator_for with
    `exceptions: false` rejects the encoded tag-using module with
    "exceptions proposal not enabled"; with `exceptions: true` it accepts, and
    the feature-set difference is exactly `WasmFeatures::EXCEPTIONS`).
    crates/psrs-backend/src/capability.rs's
    wasm_feature_flags_follow_independent_profile_fields keeps passing
    unchanged.
  Input boundary: encoded direct thin IR under both profile settings.
  Commands: as EH-01.
  Result: pass; the default profile still has `exceptions: false`.
  Revision: this slice's commit.
  Gaps: none.

EH-04:
  Implementation: crates/psrs-driver/src/tests/exception_handling.rs —
    hand-built thin-IR fixtures encoded by `psrs_backend::wasm::encode_module`,
    validated by `wasmparser::Validator::new_with_features` over an explicitly
    exceptions-enabled profile, and executed by the wasmtime CLI
    (`wasmtime run --invoke run`).
  Tests: named_catch_delivers_the_tag_payload (exit 0, stdout `7`),
    catch_all_observes_a_payload_less_throw (exit 0, stdout `42`),
    throw_unwinds_through_nested_call_frames (a throw two call frames below
    the handler; exit 0, stdout `5`), traps_are_not_exceptions (a trap inside
    a protected region is not caught: non-zero exit, `unreachable` reported).
  Input boundary: hand-built direct thin IR; explicit profiles only (the
    default profile keeps `exceptions` disabled).
  Commands: PSRS_REQUIRE_WASMTIME=1 cargo test -p psrs-driver --lib
    tests::exception_handling; workspace run as EH-01.
  Result: pass, 4/4 fixtures executed under wasmtime 48.0.1
    (7bac2c277 2026-08-24); nothing skipped.
  Revision: this slice's commit.
  Gaps: none for the row; the fixtures exercise the channel only — no
    budget, watermark, or entry synthesis exists yet (EH-12+).
```

Validation of the slice: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`PSRS_STDLIB_ROOT=… PSRS_REQUIRE_WASMTIME=1 cargo test --workspace` all pass
(2151 passed, 0 failed, 5 ignored — the ignored ones are the scoreboard
suites, out of scope here). The `wasm/tests.rs` and `wasm/verify.rs` files
grew past the 500-line limit and are now module directories
(`wasm/tests/`, `wasm/verify/`) with the existing content preserved.

## Slice 2 record — MIR representation (EH-05..EH-08)

No producer creates `Try`/`Throw` terminators yet (the P9 discharge is
EH-09+), so every fixture is hand-built direct MIR verified by
`verify_module` / `verify_module_with_capabilities`. The tag payloads draw
from the existing MIR `ValueType` set; `TagId` is module-local and indexes
`Module::tags` by position, mirroring the thin IR's `TagIndex` rule.

One interpretation the implementation fixes (design-conformant, recorded for
slice 3): a `Throw`'s handler field names the handler of the innermost
enclosing region **that catches the tag** — a tag clause for the throw's tag,
else `catch_all` — walking outward through the containing regions, which nest
and therefore form a chain. A throw inside a *handler block* resolves from its
owning try's block, because the owning region's `try_table` is disarmed when
its catch path executes. A handler-less throw is rejected unless the throw
block lies outside every protected region (and outside every armed extent).
The "regions nest or are disjoint" check is structurally implied by dominance
over the normal CFG (dominance is a tree order), so it has no violating
fixture; it is kept as the partition's stated invariant check, and the
closest structural violations (shared handlers) are caught by the
single-owner rule.

```text
EH-05:
  Implementation: crates/psrs-backend/src/mir/mod.rs (`Terminator::Throw`,
    `Terminator::Try`, `Module::tags`), crates/psrs-backend/src/mir/tag.rs
    (`Tag`, `TagId`).
  Tests: crates/psrs-backend/src/mir/tests/exception/mod.rs —
    try_and_throw_terminators_expose_the_documented_fields (constructs the
    module, verifies it, and pattern-matches every documented field of both
    terminators and of `Tag`),
    rejects_a_throw_with_an_unknown_tag,
    rejects_a_throw_payload_that_differs_from_its_tag (arity, type, and
    agreement cases),
    rejects_a_multi_value_tag_payload,
    rejects_a_try_naming_a_missing_block (missing protected entry, missing
    clause handler).
  Input boundary: well-formed and malformed direct MIR (no lowering).
  Commands: PSRS_STDLIB_ROOT=… PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
    (subset: cargo test -p psrs-backend --lib mir::tests::exception)
  Result: pass, 17/17 executed locally under the workspace run above;
    nothing runtime-gated (no executable artifact in this slice).
  Revision: this slice's commit.
  Gaps: none for the row; tags are never emitted by the lowering until
    EH-09+, so no lowered module carries a tag table yet.

EH-06:
  Implementation: crates/psrs-backend/src/mir/try_region.rs (the shared
    try-region partition: dominance carve-out, handler exclusion, nesting
    check, back-edge check) and crates/psrs-backend/src/mir/verify/exception.rs
    (clause-tag resolution, handler parameter typing, normal-predecessor and
    single-owner rules, `Throw` handler resolution, wired through
    mir/verify/function.rs into `verify_module`).
  Tests: crates/psrs-backend/src/mir/tests/exception/unwind.rs —
    accepts_a_well_formed_try_region_and_nested_tries,
    rejects_a_throw_whose_handler_is_not_the_matching_innermost_handler,
    rejects_a_throw_tag_no_enclosing_region_catches,
    rejects_a_throw_outside_every_region_that_names_a_handler,
    rejects_a_throw_without_a_handler_inside_a_protected_region;
    crates/psrs-backend/src/mir/tests/exception/regions.rs —
    rejects_a_handler_with_a_normal_predecessor,
    rejects_a_handler_claimed_by_two_try_regions,
    rejects_a_back_edge_targeting_a_try_block.
  Input boundary: malformed direct MIR; every diagnostic asserts the
    offending terminator's span.
  Commands: as EH-05.
  Result: pass under the workspace run above. Gaps: the nesting check is
    structurally implied (see the interpretation note); no fixture can
    violate it.

EH-07:
  Implementation: crates/psrs-backend/src/mir/lower/tail.rs (`rewrite`
    computes the shared partition once and skips tail conversion for calls
    whose block is protected; self-call loopification stays active inside
    regions) and the verifier rule in mir/verify/exception.rs
    (`verify_no_forced_tail_calls`).
  Tests: mir/tests/exception/tail.rs —
    mark_tail_keeps_a_protected_region_call_an_ordinary_call (the protected
    call stays `Call` + jump-to-join while the handler's identical call
    becomes `ReturnCall`, under `tail_call: true, exceptions: true`);
    mir/tests/exception/regions.rs —
    rejects_a_forced_tail_call_in_a_protected_region (a hand-written
    `ReturnCall` in a protected block is rejected at its own span).
  Input boundary: hand-built direct MIR at the function level.
  Commands: as EH-05.
  Result: pass.
  Revision: this slice's commit.
  Gaps: none for the row.

EH-08:
  Implementation: crates/psrs-backend/src/mir/verify/capability.rs —
    `RequiredCapabilities.exceptions`, set when the module declares a tag or
    contains a `Throw`/`Try` terminator, rejected with
    "the selected target does not support the WebAssembly exception-handling
    proposal required by this module" at `module.span` (the same shape as the
    other proposal gates). `TargetCapabilities::wasmtime_wasi_0_2()` and
    `Default` are untouched.
  Tests: crates/psrs-backend/src/mir/tests/exception/capability.rs —
    exception_terminators_require_the_exceptions_capability,
    a_declared_tag_requires_the_exceptions_capability; the existing
    stable_profile_matches_the_documented_wasi_0_2_target (src/capability.rs)
    still pins `exceptions: false`. The capability tests moved out of
    mir/verify/capability.rs into mir/verify/tests/capability.rs for the
    500-line limit; all pre-existing assertions preserved.
  Input boundary: direct MIR under explicitly enabled and disabled profiles.
  Commands: as EH-05.
  Result: pass.
  Revision: this slice's commit.
  Gaps: none for the row; the stable default profile still disables
    `exceptions` (the flip is EH-17).
```

Slice-2 notes for the structuring slice (EH-10/EH-11):

- The protected-region partition lives once, in `mir/try_region.rs`
  (`plan(function, pass) -> TryRegions`): `regions` (outermost protected sets
  first, each with `try_block`, `entry`, `catches`, `catch_all`, `protected`,
  `span`), `owners` (handler → owning try block), `conflicts`, plus
  `protected_blocks()`/`handler_blocks()` and `handler_closure(function)` for
  passes that must retain handler blocks without the dominance work. The
  verifier, `mark_tail`, and the structurer all consume this one definition.
- Wasm lowering (`wasm/lower/structure/region.rs`, `structure/dispatcher.rs`)
  rejects a `Throw`/`Try` terminator with "MIR exception-channel terminators
  are not lowered to Wasm yet" at the terminator's span; no lowered path
  reaches it until EH-09/EH-10 produce try regions.
- Optimizer awareness: `prune_unreachable` keeps handler blocks (they are
  unreachable along normal edges); terminator simplification treats
  `Throw`/`Try` blocks as opaque.

Validation of the slice: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`PSRS_STDLIB_ROOT=… PSRS_REQUIRE_WASMTIME=1 cargo test --workspace` all pass
(2168 passed, 0 failed, 5 ignored — the ignored ones are the scoreboard
suites, out of scope here). The MIR verifier files stay under the
500-line limit: `mir/verify/capability.rs` shed its test module into
`mir/verify/tests/capability.rs`, and `mir/mod.rs` sits at exactly 500 lines.

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names, input boundary,
commands, environment (including the pinned engine stack limit where relevant),
runtime version, executed/skipped cases, revision, and gaps. Runtime cases use
`PSRS_REQUIRE_WASMTIME=1`; skipped cases cannot verify a row, and a passing
local test is not board evidence. After Rust edits run `cargo fmt --all
--check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`. Completion requires
every row Verified and a full present-tense design audit.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
EH-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / direct MIR / malformed MIR / stdlib package
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Completion evidence and board protocol

- The runtime board's expected acceptance for this topic is `TCOMutRec` moving
  out of its P8 blocker row: the recorded L6/M7 runtime board stands at 287 of
  340 program groups (D-04, remeasured 2026-10-18 serially with the 10-second
  guest wall); the adoption claim requires a fresh scoreboard run by that
  protocol, with the new number recorded in D-04 and the `Corpus cases` field
  updated. Never restate the old number as current.
- The 10-second guest wall on the measurement host still applies: the budget's
  per-frame overhead must keep TCOMutRec's deep cases inside the wall, and the
  wall is part of the protocol's conditions, not a number this checklist may
  restate from memory.
- The pinned wasmtime baseline is 48.x (CI installs 48.0.2 and sets
  `PSRS_REQUIRE_WASMTIME=1`); evidence records the exact engine version and,
  for budget-dependent rows, the pinned `max_wasm_stack` value.
