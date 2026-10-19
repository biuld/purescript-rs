# Exception Handling and Stack Exhaustion Implementation Checklist

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Exception handling and stack exhaustion](../../design/backend/fp/exception-handling-and-stack-exhaustion.md)

**Progress:** EH-01 through EH-16 Verified (slice 1: capability gate and
encoding; slice 2: the MIR representation; slice 3: the structuring
integration; slice 4: the budget mechanism; slice 5: the library surface;
slice 6: the profile flip and the 2026-10-19 ownership decision — see the
slice 6 record for EH-16's revised corpus acceptance). EH-17 Verified with
the board remeasure below.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), row **BC-08** (the exceptions family) and the runtime board rows that reference `Test.Assert.checkThrows` (#95's blocker; the L6/M7 failure row for `TCOMutRec.purs`, re-attributed from P8 closure/linking to a runtime trap by the 2026-10-19 ownership decision — see the slice 6 record). Status claims in D-04 land with the evidence of the slices below, never from this checklist alone.

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
| EH-09 | `mir/lower/exception.rs` discharges a call to a `psrs:runtime-exception` binding into the inline try region; when the profile disables `exceptions` the call is rejected at the binding's span. | Direct-MIR discharge fixtures execute under wasmtime (catch returns the handler's value; normal path the protected value). | Verified |
| EH-10 | The structurer's region plan gains `UnitPlan::Try` with the dominance carve-out; try regions nest or are disjoint; reducible try-free functions are unchanged. | Structuring fixtures: nested tries, try inside loop, loop inside protected region; no dispatcher local on reducible input; WAT inspection shows `try_table` at the expected nesting. | Verified |
| EH-11 | A function whose CFG is irreducible and contains a try region is rejected with a diagnostic; try-free irreducible input still uses the dispatcher. | Direct-MIR fixture pair: the rejection and the unchanged dispatcher fixture. | Verified |
| EH-12 | `mir/budget.rs` fixes the weight formula and constants; `Function::frame_weight` is recorded at P9. | Unit test for `frame_weight`; MIR inspection shows the weight on each function when the profile enables `exceptions`. | Verified |
| EH-13 | Every lowered function gains the prologue check and the epilogue decrement; `return_call*` sites decrement; trapping blocks do not. | WAT inspection: prologue before the first block, decrement on each return/tail path; a trapping block has no decrement; deep tail recursion still exits 0 (no false throw). | Verified |
| EH-14 | Every `try_table` is preceded by a watermark save and its catch path begins with the restore. | WAT inspection per try region; the leak fixture (a throw/catch loop followed by deep recursion) executes with the deep recursion reaching the budget point, proving no leak. | Verified |
| EH-15 | Budget/limit coordination: with the engine stack limit pinned via `-W max-wasm-stack` (CLI) or `Config::max_wasm_stack` (Rust API), deep non-tail recursion under `checkThrows` yields the compiler's exception, never `call stack exhausted`. | The coordination fixture executes under the pinned engine and limit; the recorded values (budget, weights, limit, engine version) are written into this record and the capability-profile notes. Failure as a trap means the constants are wrong and the row stays unverified. | Verified |
| EH-16 | `Test.Assert.checkThrows` is implemented end to end: checked `psrs:runtime-exception#checkThrows` contract, P9 discharge, stdlib adaptation with its recorded target-adaptation entry, and `TCOMutRec.purs` executing (all four `assertThrows` catches, the four depth-100 `assertEqual`s, exit 0). *The acceptance's corpus half is revised by the 2026-10-19 ownership decision (DEC-23's amendment): the channel and library surface are the verified obligations; TCOMutRec's `assertThrows` assertions encode upstream's tail-call classification and stay a recorded failure row, re-attributed to a runtime trap.* | The direct-MIR and driver guard fixtures execute under `PSRS_REQUIRE_WASMTIME=1` (a genuinely non-tail depth-100000 recursion under the guard exhausts and catches); the stdlib adaptation record names the pinned upstream declaration and the diff; the corpus case's revised outcome is recorded by the board remeasure of EH-17. | Verified |
| EH-17 | The stable default profile enables `exceptions` (with the budget) as the adoption's final revision; [DEC-05](../../decision/DEC-05-wasmtime-feature-set.md)'s amendment and the capability-profile default table are updated with the evidence. | The full L6/M7 runtime board is re-run by the scoreboard protocol and the new number recorded in D-04; the disabled-profile rejection evidence and the enabled execution evidence are both cited in the profile's implementation notes. | Verified |

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

## Slice 3 record — structuring integration (EH-09..EH-11)

One correction this slice had to make to the shared partition, with the
design's own wording as the authority: the invariant section requires
"Dominance is rechecked on the CFG that includes unwind edges", and the
partition (`mir/try_region.rs`) now computes exactly that — `mir/cfg.rs`
gained `unwind_successors` (a `Try` additionally targets its clause handlers,
a `Throw` its named handler) and `dominators_with_unwind_edges`, and
`try_region::plan` consumes the latter. The normal-CFG view alone claims a
join shared by a protected block and a handler block (the handler edge is
invisible to it), which both corrupts the protected set at P9 and misplaces
the join at P10; the unwind-edge view keeps such joins out of every region.
Slice 2's note above ("structurally implied by dominance over the normal
CFG") is superseded on that one point: the nesting implication still holds,
but over the unwind-edge CFG.

One emission deviation from the design's "Emitting a try unit" sketch, made
in code with its justification: the sketch writes `br` to the normal join
after the `try_table` ("try_table completing normally"); the emitted body
instead ends in `unreachable`. Every protected block's terminator transfers
control out of the region, so the `try_table` never completes normally, and
the dead path carries none of the join's per-path argument values a `br`
would need to push. The `unreachable` also discharges the unwind block's
payload-typed result on that path for the validator. The `catch_all` case
needs neither, but the shape is uniform.

One structuring rule the design's single unwind block implies, checked at
P10 (`TryContext::new` in `wasm/lower/structure/cfg/try_context.rs`): all
clauses of one region must agree on the payload and the handler they deliver
to, since one synthesized unwind block cannot distinguish them; a region
violating it is rejected with a diagnostic naming the gap. The context also
fixes the "try inside loop" shape the region plan must support: a try whose
try block sits inside a natural loop is emitted inside the loop's body, and
its claim may reach past the loop's blocks (blocks its back edge cannot
reach); a region overlapping a loop without either nesting is rejected.
Claims are closed transitively over nested tries, so a nested try unit's
inline emission is never emitted twice.

```text
EH-09:
  Implementation: crates/psrs-backend/src/bindings/runtime/exception.rs (the
    checked source contract: `ExceptionOperation::CheckThrows`,
    `validate_source`, `checked_provider` rechecking the CC call shape, and
    `canonical_signature` for CC publication); bindings/runtime/mod.rs
    (validate admits the provider); bindings/runtime/call/mod.rs
    (canonical-signature dispatch); mir/runtime/context.rs
    (`RuntimeContext::exception_providers`, collected only for live direct
    calls; `plan_import` plans no import because the provider has no host
    ABI); mir/lower/exception.rs (the discharge: Try { protected, catch_all }
    with the guarded `ClosureCall` in the protected block, fabricated `Unit`
    operand, `false`/`true` constants joined through a block whose parameter
    is the assignment's destination); mir/lower/assignments.rs (dispatch) and
    mir/lower/mod.rs (the lowerer carries the runtime context and the target
    profile).
  Tests: crates/psrs-backend/src/mir/tests/exception/discharge.rs —
    discharges_a_check_throws_call_into_the_inline_try_region (inspects the
    discharged MIR: entry Try with catch_all only, protected block applies
    the callable, handler supplies `true`, shared join's parameter is the
    destination; the module verifies under the enabled profile),
    the_discharged_catch_path_returns_the_handler_s_value (a payload-less
    channel throw in the guarded callee; exit 1 = handler's `true`),
    the_discharged_normal_path_returns_the_protected_value (exit 0 = the
    protected path's `false`),
    rejects_a_check_throws_call_when_the_profile_disables_exceptions (the
    diagnostic carries the binding's own span, distinct from the call site).
  Input boundary: hand-built CC module with the checked binding, lowered
    through `lower_module_with_bindings` (so P8 validation runs), then
    executed under wasmtime with an explicitly enabled target.
  Commands: PSRS_STDLIB_ROOT=… PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
    (subset: cargo test -p psrs-backend --lib mir::tests::exception::discharge)
  Result: pass, 3/3 runtime fixtures executed under wasmtime 48.0.1
    (7bac2c277 2026-08-24), nothing skipped; the inspection and rejection
    tests are local.
  Revision: this slice's commit.
  Gaps: none for the row; the discharge produces a `catch_all` region only —
    named-catch behavior is covered by the direct-MIR structuring fixtures
    (EH-10) and the guard contract needs only `catch_all`.

EH-10:
  Implementation: crates/psrs-backend/src/wasm/lower/structure/cfg/
    try_context.rs (`TryContext`: clause uniformity, transitive claim
    closure, try/loop nesting validation), cfg/build.rs (`UnitPlan::Try`
    placement: the try block in the region's topological order, the
    protected sub-region built from the raw dominated set, claim bubbling so
    nested units' inline emission is claimed exactly once), cfg/mod.rs
    (`ControlFlowPlan::build` wires the context; the region plan's
    reachability walk follows unwind edges so catch-path blocks are planned),
    wasm/lower/structure/region/try_unit.rs (`emit_try_unit`: try block's
    instructions, the synthesized unwind `Op::Block` whose result is the
    caught payload, the `try_table` with every clause targeting that block at
    depth 0, the payload copied into the handler's parameter locals in
    reverse like jump arguments, then the handler's instructions and
    terminator), region/mod.rs (`Label::Unwind` and `Label::TryTable` — the
    two anonymous entries pushed for the protected sub-region's emission, no
    branch targets them), wasm/lower/mod.rs (each tag's function type
    appended and its `TagDef` recorded; the module's tag section filled),
    wasm/lower/function/mod.rs and structure/mod.rs (the tags reach the
    structurer). The watermark save/restore points are marked in
    emit_try_unit for EH-14; nothing budget-related exists yet.
  Tests: crates/psrs-backend/src/mir/tests/exception/structuring/ —
    mod.rs: structures_nested_tries_with_the_inner_try_inside_the_outer
      (WAT: two `try_table`s, outer < inner < throw; the inner handler
      catches: exit 10),
      delivers_a_named_catch_payload_to_the_handler_parameter (WAT contains
      `try_table` and `throw`; exit 7 = the payload the handler parameter
      received);
    loops.rs: re_arms_a_try_table_across_loop_iterations (WAT: one
      `try_table` after the `loop` label; exit 99 = the catch on the third
      iteration),
      keeps_a_loop_inside_the_protected_region (WAT: the `loop` label after
      the `try_table`; exit 42 = the mid-loop throw's handler);
    dispatcher_local.rs: does_not_allocate_a_dispatcher_local_on_reducible_input
      (the structured function's locals are exactly the lowered values' —
      three, with no dispatcher state local).
  Input boundary: hand-built direct MIR, verified by
    `verify_module_with_capabilities`, lowered with an explicitly enabled
    target; WAT via `wasmprinter` over the encoded binary; executed under
    wasmtime.
  Commands: PSRS_STDLIB_ROOT=… PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
    (subset: cargo test -p psrs-backend --lib mir::tests::exception::structuring)
  Result: pass, 4/4 runtime fixtures executed under wasmtime 48.0.1
    (7bac2c277 2026-08-24), nothing skipped; the WAT and local-count tests
    are local.
  Revision: this slice's commit.
  Gaps: none for the row; reducible try-free functions are unchanged (the
    whole pre-existing structuring test suite still passes; no dispatcher
    local), and the watermark points are markers only until EH-14.

EH-11:
  Implementation: crates/psrs-backend/src/wasm/lower/structure/cfg/mod.rs —
    `ControlFlowPlan::build` rejects an irreducible CFG that contains a try
    region ("MIR function combines irreducible control flow with a try
    region; the dispatcher cannot arm a try_table") at the try region's span
    before the dispatcher path is taken; try-free irreducible input reaches
    `ControlFlowPlan::Dispatcher` unchanged.
  Tests: crates/psrs-backend/src/mir/tests/exception/dispatch_boundary.rs —
    rejects_an_irreducible_cfg_that_contains_a_try_region (a two-entry cycle
    whose cycle blocks lie inside a protected region; `P10 Wasm structuring`,
    the `Try` terminator's span, the message above),
    structures_try_free_irreducible_input_through_the_unchanged_dispatcher
    (the same two-entry cycle try-free: the dispatcher state local is the one
    local beyond the ten lowered values, and the artifact runs — exit 1 =
    the dispatched result).
  Input boundary: hand-built direct MIR fixture pair (with and without the
    try region), executed under the default profile for the dispatcher side.
  Commands: PSRS_STDLIB_ROOT=… PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
    (subset: cargo test -p psrs-backend --lib
    mir::tests::exception::dispatch_boundary)
  Result: pass, 1/1 runtime fixture executed under wasmtime 48.0.1
    (7bac2c277 2026-08-24), nothing skipped; the rejection test is local.
  Revision: this slice's commit.
  Gaps: none for the row.
```

Validation of the slice: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`PSRS_STDLIB_ROOT=… PSRS_REQUIRE_WASMTIME=1 cargo test --workspace` all pass
(2179 passed, 0 failed, 5 ignored — the ignored ones are the scoreboard
suites, out of scope here; wasmtime 48.0.1, 7bac2c277 2026-08-24). The
500-line limit held without dropping content: `mir/mod.rs` sits at 498
lines; `wasm/lower/structure/cfg/mod.rs` split into a module directory
(`cfg/`, with `try_context.rs` and `build.rs` beside `irreducible.rs`),
`wasm/lower/structure/region.rs` into `region/` (`mod.rs` +
`try_unit.rs`), and `mir/tests/exception/structuring.rs` into
`structuring/` (`mod.rs`, `loops.rs`, `dispatcher_local.rs`).

Notes for the budget slice (EH-12..EH-15):

- The depth global's allocation point is `wasm/lower/mod.rs`
  (`lower_module_with_plan`): the module's globals are assembled there
  today, and the tag section's type indices are already appended in the same
  function — the global (mutable `i32`, init 0, not exported) goes after the
  literal-interning globals, which the same function's `globals` vec
  already orders.
- The watermark save/restore hooks are marked in
  `wasm/lower/structure/region/try_unit.rs` (`emit_try_unit`): the save
  immediately before the synthesized unwind `Op::Block`, the restore as the
  catch path's first action after it. A fresh `i32` local per try region
  needs the structurer to allocate locals beyond the lowered values —
  `local_indices` in `wasm/lower/function/mod.rs` is where the lowered
  locals are enumerated today.
- The prologue/epilogue hook: `emit_function` in `wasm/lower/function/mod.rs`
  drives the structurer per function; the prologue prepends to the entry
  block's emission and the epilogue/`return_call*` decrements hook into
  `emit_terminator` (`wasm/lower/structure/region/mod.rs`) at the
  `Return`/`ReturnCall*` arms. `Function::frame_weight` is recorded at P9
  (`mir/mod.rs`'s function lowering, once `mir/budget.rs` fixes the
  constants).

## Slice 4 record — the budget mechanism (EH-12..EH-15)

The constants stay at the design's provisional values — `FRAME_WEIGHT_FIXED
= 32`, `FRAME_WEIGHT_PER_LOCAL = 16`, `STACK_BUDGET = 262144` — because the
coordination fixture validated them (EH-15 below); no revision was needed.
Two shared CFG analyses had to be made truthful about unwind edges for the
leak fixture to structure at all, both already mandated by the design's
invariant section ("Dominance is rechecked on the CFG that includes unwind
edges"), which slice 3 applied to the partition but not yet to:

- **Value availability** (`mir/verify/function.rs`): the check consumed
  `cfg::dominators` — the normal CFG, on which a handler block is
  unreachable — so a handler could never read any enclosing value. A loop of
  `checkThrows` guards needs exactly that (the handler carries the loop
  counter forward), so the check now consumes `dominators_with_unwind_edges`.
  Truthful in both directions: the handler sees the values its unwind
  predecessors guaranteed, and a join shared by a normal and a handler path
  no longer claims values defined on only one of them. All pre-existing
  fixtures pass unchanged.
- **The structurer's CFG** (`wasm/lower/structure/cfg/mod.rs`,
  `cfg/irreducible.rs`): the predecessor map and the SCC's DFS counted only
  normal edges while the reachable set already included handler blocks. A
  handler jumping back into a loop's body then looked like an external entry
  into its SCC — an irreducibility false positive on a reducible CFG, and
  EH-11's rejection fired on the leak fixture. Predecessors now include the
  unwind edges; the leak fixture structures through the reducible path, and
  the EH-10/EH-11 fixture pair is unchanged.

One lowering-accounting fact the implementation fixes, with the design's own
words as authority ("`locals(f)` is the lowered function's local count"):
P9's lowering adds fresh values beyond the CC source's (WIT call plans,
Boolean/int conversions, control helpers), so `frame_weight` is recorded
from the *lowered* function's parameter and value counts, not the CC
source's. P10's list-copy and dispatcher locals stay unknown at P9 and are
covered by the headroom; recorded in `mir/budget.rs`'s contract comment.

The recorded calibration measurement (see EH-15): on wasmtime 48.0.1 with
`-W max-wasm-stack=524288`, the budget fires at about 1,490 frames of the
coordination fixture's `rec` (weight 176 per frame at P9) while the engine's
trap on the same function sits between 6,000 and 7,000 frames — roughly a
quarter of the engine's limit — and the depth-100 returning recursion
consumes about 7% of the budget. The engine's *default* limit behaved
identically (the same boundaries with and without the flag), so pinning is
inert for the non-budget corpus today and pins the condition explicitly for
EH-16/17. The fixture's failure mode was verified to be the hazard itself:
with `STACK_BUDGET` raised beyond the engine limit, the same fixture fails
with the engine's `call stack exhausted` trap (exit 134) instead of the
caught exception.

```text
EH-12:
  Implementation: crates/psrs-backend/src/mir/budget.rs (FRAME_WEIGHT_FIXED,
    FRAME_WEIGHT_PER_LOCAL, STACK_BUDGET, frame_weight) and the recording at
    P9: crates/psrs-backend/src/mir/mod.rs (`Function::frame_weight`) and
    mir/lower/mod.rs (recorded from the lowered function's parameters +
    values, after the lowerer has finished).
  Tests: crates/psrs-backend/src/mir/budget.rs
    frame_weight_adds_a_fixed_weight_per_local (the formula, the constants,
    and the depth-100 recursion staying inside the budget);
    crates/psrs-backend/src/mir/tests/exception/budget.rs
    records_the_frame_weight_on_every_lowered_function (the checkThrows CC
    module lowered through P9 with the enabled target: every function's
    weight equals frame_weight(parameters + values) and is nonzero).
  Input boundary: CC module through P9; the formula at the unit level.
  Commands: PSRS_STDLIB_ROOT=… PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
    (subset: cargo test -p psrs-backend --lib mir::tests::exception::budget)
  Result: pass, 9/9 executed locally under the workspace run above (plus the
    two coordination runtime fixtures, counted under EH-15).
  Revision: this slice's commit.
  Gaps: none for the row; P10's list-copy/dispatcher locals are outside the
    P9-visible count and inside the headroom (documented in budget.rs).

EH-13:
  Implementation: crates/psrs-backend/src/wasm/lower/budget.rs (Budget:
    increment/decrement/overflow_check), wasm/lower/function/mod.rs (the
    prologue prepended before the first structured instruction; the watermark
    locals appended), wasm/lower/structure/region/mod.rs (the epilogue
    decrement at the `Return` arm and the tail decrement at the
    `ReturnCall`/`ReturnCallRef` arms of emit_terminator; the `Trap` arm
    emits only `unreachable`).
  Tests: budget.rs emits_the_prologue_and_the_return_and_tail_decrements —
    WAT: one `i32.gt_s` before the first `block`, the `if`/`throw` overflow
    path before the first block; thin-IR window counts: main (trap + tail)
    exactly one decrement, rec exactly two (its two return paths), so the
    trapping block carries none; the tail decrement sits between the prologue
    and the `return_call`. budget/budget_execution.rs
    deep_tail_recursion_exits_0_without_a_false_throw — mutual `even`/`odd`
    tail calls at depth 1,000,000 exit 0 with no trap (the decrements keep
    the counter bounded).
  Input boundary: hand-built direct MIR (entry zero-argument Int, per the
    artifact's entry contract), WAT via wasmprinter over the encoded binary.
  Commands: as EH-12.
  Result: pass, 1/1 runtime fixture executed under wasmtime 48.0.1
    (7bac2c277 2026-08-24) with `-W max-wasm-stack=524288`; the WAT/IR
    inspections are local.
  Revision: this slice's commit.
  Gaps: none for the row.

EH-14:
  Implementation: crates/psrs-backend/src/wasm/lower/mod.rs (the depth
    global planned after the literal-interning globals, mutable `i32`, init
    0, not exported, via Budget::for_module) and
    wasm/lower/structure/region/try_unit.rs (the save immediately before the
    synthesized unwind block, the restore as the catch path's first action;
    one fresh `i32` local per try region, numbered in block order and
    appended after the lowered values and list-copy locals — a function with
    try regions never allocates a dispatcher local, since EH-11 rejects that
    combination first).
  Tests: budget/watermark.rs every_try_table_carries_the_watermark_save_and_restore —
    the nested-try fixture: WAT shows three `try_table`s (two regions + the
    entry's); thin-IR walks count one save and one restore per try region in
    the function and in the entry. budget/budget_execution.rs
    a_throw_catch_loop_does_not_leak_the_depth_counter — 21,848 guarded
    thrower iterations (each a throw/catch that leaks the thrower's 48 weight
    per iteration if the restore is missing), then a *shallow* guarded
    rec(100) that must return, then the deep guarded rec(100,000): exit 1 =
    the deep recursion reaches the budget point after the loop; a leak would
    fire the budget at the shallow guard (exit 2).
  Input boundary: hand-built direct MIR under the enabled target.
  Commands: as EH-12.
  Result: pass, 1/1 runtime fixture executed under the pinned engine; the
    watermark walk and WAT inspection are local.
  Revision: this slice's commit.
  Gaps: none for the row.

EH-15:
  Implementation: crates/psrs-backend/src/wasm/lower/entry.rs (the
    synthesized `run` entry: with the budget, the call to `main` sits in a
    `try_table (catch_all)`; the handler's first action restores the depth
    global to the value saved at entry, then `exit-with-code 1` — and the
    entry's own locals gained the watermark save local, carried through
    wasm/mod.rs's `Entry::locals`, wasm/verify/mod.rs's entry local count,
    and wasm/encode.rs's entry code emission);
    crates/psrs-backend/src/mir/tests/exception/execution.rs
    (run_component pins `-W max-wasm-stack=524288` for every target with
    `exceptions` enabled);
    crates/psrs-driver/src/tests/mod.rs (the run helpers pin the same value
    — behavior-neutral for the current corpus, whose default-profile
    programs carry no budget, but pinned for the budget-dependent evidence
    EH-16/17 will run through the driver).
  Tests: budget/coordination.rs — the checkThrows discharge fixture (CC
    module: main guards a callable applying the genuinely non-tail
    self-recursive `rec`; `1 +` after the call keeps every frame real):
    deep_non_tail_recursion_yields_the_compiler_s_exception_never_a_trap
    (depth 100,000: exit 1 = the guard's caught value; stderr contains
    neither `call stack exhausted` nor `unreachable`),
    depth_100_recursion_still_returns_under_the_pinned_budget (depth 100:
    exit 0 — no grossly early firing). Sanity: with `STACK_BUDGET` beyond
    the engine limit the same fixture fails with the engine's trap (exit
    134), confirming the fixture's failure mode is the budget-above-limit
    hazard.
  Input boundary: CC module with the checked `psrs:runtime-exception#checkThrows`
    binding, lowered through P8/P9, executed under the pinned engine.
  Commands: PSRS_STDLIB_ROOT=… PSRS_REQUIRE_WASMTIME=1 cargo test --workspace
    (subset: cargo test -p psrs-backend --lib mir::tests::exception::budget::coordination).
  Result: pass, 2/2 fixtures executed under wasmtime 48.0.1
    (7bac2c277 2026-08-24), `-W max-wasm-stack=524288`; nothing skipped.
    Measured calibration values: budget 262144; weights fixed 32, per-local
    16; pinned limit 524288 bytes (512 KiB, measured equal to the engine
    default); engine wasmtime 48.0.1 (7bac2c277 2026-08-24). Firing depth
    1,490 frames (last non-firing 1,480); engine trap depth (budget off)
    between 6,000 and 7,000 frames; the same fixture disabled traps (exit
    134). Also recorded in the capability-profile implementation notes.
  Revision: this slice's commit.
  Gaps: the disabled-profile rejection and the enabled execution evidence
    are cited here; the profile default flip and the D-04 remeasure belong
    to EH-16/17.

Budget-disabled regression: budget.rs the_disabled_profile_emits_no_budget_code —
  the same trap/tail fixture lowered with the default profile: no depth
  global, no tag, no `throw`, no `i32.gt_s`, no `global.set` in any body;
  the_enabled_profile_adds_the_depth_global_and_tag pins the global's shape
  (index 0 for a fixture without literal globals, mutable `i32`, init 0) and
  the compiler-defined tag's presence.
```

Validation of the slice: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`PSRS_STDLIB_ROOT=… PSRS_REQUIRE_WASMTIME=1 cargo test --workspace` all pass
(the scoreboard suites stay `#[ignore]`d, out of scope here; wasmtime 48.0.1,
7bac2c277 2026-08-24): 2189 passed, 0 failed, 5 ignored. The 500-line limit
held by splitting: `mir/mod.rs`
shed `element_has_bytes`/`referenced_imports` into `mir/imports.rs`,
`wasm/lower/mod.rs` moved the budget planning into `wasm/lower/budget.rs`
(`Budget::for_module`) and the entry synthesis into `wasm/lower/entry.rs`,
and `mir/tests/exception/budget.rs` split into `budget/` (`watermark.rs`,
`budget_execution.rs`, `coordination.rs`).

Notes for the library-surface slice (EH-16):

*(Written by the budget slice; the library-surface record below supersedes
the two points that proved wrong: the guarded-callable shape did map onto
the coordination fixture's `apply`/`rec` split, but the corpus case's own
recursion is converted to constant-stack physical tail calls, which this
note did not know.)*

- The discharge stands: P8 validates the `psrs:runtime-exception#checkThrows`
  source contract and P9 lowers the call into the inline `catch_all` try
  region; `plan_import` plans no host import. EH-16 added the *publishing*
  side (the provider's canonical signature in CC publication, per
  `canonical_signature` in bindings/runtime/exception.rs) and the
  stdlib's `Test/Assert.purs` adaptation (`checkThrows :: forall a. (Unit ->
  a) -> Effect Boolean` over `Target.perform`, with its vendoring record).
- The guarded callable must be a `FunctionRef` over a function whose body
  calls the *deep* function directly: the fabricated Unit operand is the
  callable's only argument, so the depth travels inside the closure's body,
  not through the guard. TCOMutRec's `ntcoN` are plain functions; the
  stdlib-side closure `\_ -> ntco1 100000` maps onto the coordination
  fixture's `apply`/`rec` split exactly.
- The budget executes under the pinned engine (512 KiB) with about 4×
  trap-headroom and depth-100 at ~7% of the budget; the depth-100
  `assertEqual` halves of TCOMutRec run inside the margin the coordination
  fixture measured. EH-16's run must use the driver helpers, which now pin
  `-W max-wasm-stack=524288`.
- Residual gap before TCOMutRec executes: none known in the channel —
  `assertThrows` needs a `Boolean` result and the discharge produces one;
  `assert'`'s failure protocol (storage trap) is untouched in this slice.

## Slice 5 record — the library surface (EH-16, Blocked; closed out by slice 6)

The slice landed the whole library surface and proved the channel end to end
through the real frontend; the corpus execution that closes the row is blocked
by a conflict *inside the corpus case itself*, diagnosed below with the
evidence of every command run.

```text
EH-16:
  Implementation:
    Stdlib (independent psrs-stdlib repository, branch `check-throws`,
      commit 728a82f94af6e7c54ba10124af54644e8c364d19): lib/Test/Assert.purs
      implements `checkThrows :: forall a. (Unit -> a) -> Effect Boolean`
      as `unsafeCoerce (Target.perform \_ -> checkThrowsImpl fn)` over the
      checked `foreign import "psrs:runtime-exception#checkThrows"`
      `checkThrowsImpl :: forall a. (Unit -> a) -> Boolean`; exports,
      `assertThrows`/`assertThrows'` bodies, and `assertImpl`'s storage-trap
      failure protocol unchanged. The adaptation record is
      docs/assert-throws.md in that repository (pinned upstream declaration,
      exact diff, target justification, behavior evidence, remaining
      obligation); `upstream-lock.json`'s Test/Assert.purs vendored_sha256
      refreshed; the stale "unsupported foreigns" notes in that repo's
      native-effect.md/effect-restoration.md updated. Package fingerprint
      fnv1a64-v1:5dfa6f2ce6b3092c; stdlib.lock.json pins the branch commit
      and fingerprint in this slice's compiler commit.
    Compiler publication side: bindings/runtime/exception.rs
      `canonical_signature` publishes the checked CC signature for the
      provider (one non-nullable closure-reference operand whose checked
      signature takes exactly the erased `Unit`, a Boolean result) and adapts
      nothing — the provider has no host ABI, and the dispatch in
      bindings/runtime/call/mod.rs plus the publication path in cc/mod.rs
      (`runtime_cc_signature`) route through it; `mir/runtime/context.rs`
      rechecks the shape at P9 and plans no import.
  Tests:
    crates/psrs-backend/src/bindings/runtime/call/tests.rs
      canonical_exception_call_publishes_the_checked_source_signature_without_a_host_abi —
      publication keeps the callable operand and Boolean result unchanged,
      `checked` accepts the published external, and the three malformed
      shapes (arity, non-callable operand, non-Boolean result) are rejected
      with the binding's module.
    crates/psrs-driver/src/tests/exception_library.rs (all four executed
      under PSRS_REQUIRE_WASMTIME=1, enabled profile, pinned engine):
      check_throws_observes_the_budget_exception_end_to_end — the guard is
      interpreted (runs when the Effect runs), a genuinely non-tail `rec`
      at depth 100000 exhausts the budget inside the guarded call, the guard
      catches, and the program logs `200`, then `true`, exit 0;
      check_throws_reports_false_when_the_argument_returns — a guard around
      the corpus's own `ntco1` logs `1009`, then `false`, exit 0;
      tcomutrec_depth_100_halves_return_inside_the_budget — the corpus
      source cut down to its four depth-100 `assertEqual` halves logs `Done`
      and exits 0 under the enabled profile;
      tcomutrec_is_rejected_when_the_profile_disables_exceptions — the full
      corpus case is rejected with the proposal's source-associated
      diagnostic under the disabled profile.
  Input boundary: the adapted stdlib package (locked consumption), real
    frontend publication (trusted-prelude compile path), executed components.
  Commands: PSRS_STDLIB_ROOT=/home/biu/Projects/psrs-stdlib
    PSRS_REQUIRE_WASMTIME=1 cargo test --workspace; focused:
    cargo test -p psrs-backend --lib bindings::runtime::call and
    cargo test -p psrs-driver --lib tests::exception_library.
  Result: pass for the surface and the channel (wasmtime 48.0.1, 7bac2c277
    2026-08-24, `-W max-wasm-stack=524288`); the row's corpus execution is
    blocked — see the conflict record below.
  Revision: this slice's compiler commit plus the stdlib branch commit above.
  Gaps: the four depth-100000 `assertThrows` halves do not throw on this
    target; the conflict record names the owning decision.
```

### The blocked corpus execution — diagnosis and evidence

`TCOMutRec.purs` compiles under the enabled profile and traps at its first
`assertThrows`. The observed run (the driver's per-program invocation —
`compile_program_sources_with_prelude` over the case's inputs, executed by
`wasmtime run -W max-wasm-stack=524288`, the scoreboard's runner with the
EH-15 pin):

```text
command: PSRS_STDLIB_ROOT=/home/biu/Projects/psrs-stdlib PSRS_REQUIRE_WASMTIME=1 \
  cargo test -p psrs-driver --lib tests::exception_library (the failing case)
stdout:  "" (empty)
stderr:  Assertion failed: An error should have been thrown
         Error: failed to run main module …
         2: wasm trap: wasm `unreachable` instruction executed
exit:    134
wall:    1.25 s (well inside the 10-second guest wall)
```

`assertThrows` failing means the guard returned `false`: the guarded
application `\_ -> ntcoN 100000` **returned normally** at depth 100000. The
diagnosis, with each fact from a command actually run:

- The backend's mandated tail-call treatment reuses frames across
  recursion cycles that are mediated by intermediate closure values.
  `ntco1 y0 = f 0 y0` lowers to a single block ending in `ReturnCallRef`
  (`psrs dump mir` on a probe module); all four `ntcoN 100000` complete and
  return their exact values in constant stack (a probe summing the four
  exits with the expected `1600009 mod 256 = 9`), so the depth global never
  crosses the budget and `checkThrows` observes no throw.
- This constant-stack behavior is not separable from the corpus's own
  *returning* halves: `tco3`'s recursion cycle is also closure-mediated
  (`f x y = g x (h y)` lowers through a partial application to the same
  `ReturnCallRef` shape), and its `assertEqual` at depth 100000 requires the
  reuse to hold. Within one corpus program, the same lowering mechanism must
  both reuse the frame (`tcoN`) and exhaust (`ntcoN`).
- The distinction the corpus encodes is upstream's tail-call pass
  classification (arity-matched named recursion groups loopified; the
  arity-mismatched closure chains left growing), which the corpus's own
  comment anticipates an eta-expanding compiler would change ("In theory,
  these could be made to optimize via eta-expansion in the future, in which
  case the assertions can change"). Reproducing it here needs cross-function
  machinery the MIR does not have (collapsing a mutual recursion group into
  one function's loop), which is a normative decision, not a slice repair:
  the verified tail-call row (CF-07, `indirect_tail_recursion_uses_return_call_ref`)
  pins the current capability deliberately.

What the row waits on: either that decision (a new recursion-classification
topic with its own design) or a revision of the adoption's expected end state
(DEC-23's "TCOMutRec moving … to passing") to record the case as an
upstream-implementation-specific assertion the target does not reproduce.
The channel itself is verified end to end: with a genuinely non-tail
recursion (the coordination fixture's shape, now through the real frontend),
the guard is interpreted, the budget fires inside the guarded call, and the
catch delivers `true`.

### The scalar-propagation repair this slice landed

Compiling the guard through the real frontend exposed a third analysis that
still viewed only normal CFG edges — the same class slice 4 fixed twice:

- **Scalar propagation** (`mir/opt/constants.rs`): `analyze` built its block
  -parameter predecessor lattice from a normal-successor reachability walk, so
  a join shared by a protected path and a catch path saw only the normal
  predecessor's incoming values. For the discharged guard the join's parameter
  merged to the constant `false`, was materialized away, and the removal loop
  stripped the *handler's* `true` argument from its jump — every `checkThrows`
  call observed `false` even when its catch path fired. The predecessor
  collection now uses `unwind_successors`-based reachability
  (`mir/opt/cfg.rs::reachable_blocks_with_unwind`), so catch-path jump
  arguments are real incoming values. All pre-existing fixtures pass
  unchanged (psrs-backend 587 passed).

## Slice 6 record — the profile flip and the ownership decision (EH-16 close-out, EH-17)

The coordinator/user decision this slice lands (Option 2, decided 2026-10-19):
the target does **not** reproduce upstream's TCO-classification
aggressiveness. This backend's mandated closure-tail reuse (`mark_tail` →
`ReturnCallRef`; CF-07's verified row) runs arity-mismatched mutual recursion
in constant stack, while upstream's TCO pass loopifies arity-matched named
mutual-recursion groups and leaves arity-mismatched closure-mediated cycles
growing the stack. `TCOMutRec.purs`'s assertions encode upstream's
classification (its own comment anticipates change), so its four
`assertThrows` cases are upstream-implementation-specific and not reproduced
by this target: the case stays a failure row, re-attributed from the P8
missing-foreign row to a runtime-trap row. The stack budget's purpose is
unchanged — it converts genuine non-tail exhaustion into the compiler's
exception, verified end to end. The decision is recorded as DEC-23's
amendment, in the design's Worked example and Boundaries, and as the
tail-call boundary note in [the tail-calls design](../../design/backend/fp/control-flow-and-tail-calls.md).

```text
EH-16 (close-out):
  Implementation: none beyond the flip and documentation — the channel and
    the library surface were landed by slices 1–5; this slice re-owns the
    corpus acceptance per the decision.
  Tests: slice 5's guard fixtures stand unchanged
    (check_throws_observes_the_budget_exception_end_to_end,
    check_throws_reports_false_when_the_argument_returns,
    tcomutrec_depth_100_halves_return_inside_the_budget);
    tcomutrec_is_rejected_when_the_profile_disables_exceptions now
    constructs its disabled target explicitly and keeps passing.
  Input boundary: the adapted stdlib package through the real frontend,
    executed components under the pinned engine.
  Commands: PSRS_STDLIB_ROOT=/home/biu/Projects/psrs-stdlib
    PSRS_REQUIRE_WASMTIME=1 cargo test --workspace (subset:
    cargo test -p psrs-driver --lib tests::exception_library).
  Result: pass; the channel and library surface are Verified, and the corpus
    acceptance is re-owned and recorded: on the re-measured board the case
    compiles and its row reads "trapped: Assertion failed: An error should
    have been thrown" — the guard observes no throw (closure-tail reuse runs
    the arity-mismatched `ntcoN` recursion in constant stack) and `assert'`
    takes its storage-trap failure. Re-attributed from "P8 library linking
    []: foreign value `Test.Assert.checkThrows` has no target
    implementation".
  Revision: this slice's commit.
  Gaps: none for the revised row; reproducing upstream's classification
    would need a new recursion-classification topic (recorded future work in
    the design and DEC-23).

EH-17:
  Implementation: crates/psrs-backend/src/capability.rs — the stable default
    profile (`wasmtime_wasi_0_2()`, and therefore `Default`) enables
    `exceptions`, with the doc comment citing DEC-23's 2026-10-19 revision;
    the disabled-profile rejection fixtures construct explicit
    `exceptions: false` targets (mir/tests/exception/discharge.rs,
    mir/tests/exception/budget/mod.rs, psrs-driver
    tests/exception_library.rs) and keep passing; the board's own runner
    (tests/suite/runtime.rs::run_component) now pins the engine stack limit
    `-W max-wasm-stack=524288`, because the flip makes every corpus program
    budget-bearing.
  Tests: capability.rs stable_profile_matches_the_documented_wasi_0_2_target
    pins the new documented truth; wasm_feature_flags_follow_independent_profile_fields
    unchanged; the whole workspace run below.
  Input boundary: the flipped stable profile over the full vendored corpus
    (program groups), executed components under the pinned engine.
  Commands: the board re-run per the recorded serial per-root protocol —
    one worker per root (`xargs -P1`), each root filtered by
    `passing/<basename>` with a 110-second outer cap, all 365 roots,
    release suite binary `target/release/deps/suite-461c5a8e1563a05b`
    (SHA-256 5d3bf0585021e936c34f83c229da881a4df64f454b2b5ec9852f659357cbae89),
    `PSRS_STDLIB_ROOT=/home/biu/Projects/psrs-stdlib`,
    `PSRS_REQUIRE_WASMTIME=1`, wasmtime CLI 48.0.1 (7bac2c277 2026-08-24).
  Result: the board is **287/340** (287 completions exit zero, 25 FFI groups
    excluded, 74 support sources remain inputs). The failure-name set is
    identical to the 2026-10-18 baseline; the complete diff is one row's
    re-attribution — `TCOMutRec.purs` from "P8 library linking []: foreign
    value `Test.Assert.checkThrows` has no target implementation" to
    "trapped: Assertion failed: An error should have been thrown" (the
    decision above). Stage split of the 53 non-agreements: 29 P5 typecheck,
    13 P5 kind check, 1 P8 closure conversion (the recursive local binding
    in `Stream.purs`), 5 P3 resolve, 2 P0 lex (DEC-16), 1 runtime trap
    (TCOMutRec), the 10-second runtime timeout in `2288.purs` (unchanged),
    and the resource-bounded `BigFunction.purs` compilation (did not finish
    inside the 90-second cap on this host; killed at the cap, peak RSS
    2,676 KiB — resource-bounded row unchanged in kind). Exit-code audit: a
    second serial per-root pass captured each root's exit-code histogram;
    every one of the 287 agreeing roots carries only exit code 0 — no
    program exits 1 through the entry's unguarded budget exception (the
    agreeing set is identical across both passes). Nothing was recovered
    and nothing regressed; the total is unchanged by measurement.
    The L1–L5 frontend boards were not run: the flip is backend-only, and
    those boards read the vendored corpus with the annotations oracle and do
    not execute (L1 additionally needs `purs`, unavailable on this host) —
    no number of theirs is restated as current.
  Revision: this slice's commit.
  Gaps: none for the row.
```

Validation of the slice: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`PSRS_STDLIB_ROOT=/home/biu/Projects/psrs-stdlib PSRS_REQUIRE_WASMTIME=1
cargo test --workspace` all pass (the five `#[ignore]`d scoreboard suites
are executed separately by the board protocol above). The D-04 gate row, the
BC-08 row, the BE-21/Effects rows, the M7 progress paragraph, and the
README's runtime row carry the numbers of this remeasure.

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

- The adoption's corpus acceptance was revised by the 2026-10-19 ownership
  decision (DEC-23's amendment): the channel's proof is the direct-MIR and
  driver guard fixtures, and `TCOMutRec` stays a recorded failure row,
  re-attributed from its P8 blocker to a runtime trap. The recorded L6/M7
  runtime board (287 of 340 program groups, D-04, remeasured 2026-10-18
  serially with the 10-second guest wall) was re-measured with the flipped
  profile by the scoreboard protocol in EH-17's record below, with the new
  number and the failure-set diff recorded in D-04. Never restate an old
  number as current.
- The 10-second guest wall on the measurement host still applies: the budget's
  per-frame overhead must keep TCOMutRec's deep cases inside the wall, and the
  wall is part of the protocol's conditions, not a number this checklist may
  restate from memory.
- The pinned wasmtime baseline is 48.x (CI installs 48.0.2 and sets
  `PSRS_REQUIRE_WASMTIME=1`); evidence records the exact engine version and,
  for budget-dependent rows, the pinned `max_wasm_stack` value.
