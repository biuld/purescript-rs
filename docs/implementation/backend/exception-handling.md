# Exception Handling and Stack Exhaustion Implementation Checklist

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Exception handling and stack exhaustion](../../design/backend/fp/exception-handling-and-stack-exhaustion.md)

**Progress:** EH-01 through EH-17 Unverified. This is a design-first slice: no
row is claimed implemented and no evidence is recorded yet.

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
| EH-01 | The thin IR models `Op::Try` with catch clauses, `TagDef`, and the `TagIndex` domain; leaf opcodes stay `wasm_encoder::Instruction`. | Backend unit tests construct/inspect the nodes; the thin-IR verifier accepts them and rejects out-of-range tag indices and unresolved clause label depths. | Unverified |
| EH-02 | The encoder emits the tag section (id 13) between memory and global; tags are declared only when used. | Binary/WAT regression tests: a tag-using module encodes with the tag section in position; a module without tags emits none; wasmparser accepts under the enabled profile. | Unverified |
| EH-03 | The validator is built from the profile so `exceptions` gates the encoding; no other feature is enabled implicitly. | wasmparser rejects a tag-using module built with `exceptions: false` and accepts it enabled; the independent-field test still passes. | Unverified |
| EH-04 | Hand-built direct-MIR/wasm fixtures execute under wasmtime: `throw` + named `catch` with a payload, `throw` + `catch_all` payload-less, unwinding through nested call frames. | `PSRS_REQUIRE_WASMTIME=1` execution asserts caught values/results and exit codes. | Unverified |
| EH-05 | MIR defines `Tag`/`TagId`, `Module::tags`, and the `Throw`/`Try` terminators with the documented fields. | Constructor inspection plus malformed fixtures: missing tag, wrong payload arity/type, unknown handler block. | Unverified |
| EH-06 | The MIR verifier checks unwind edges: `Throw` handlers resolve to the innermost enclosing region's matching handler (or `None` outside every region); handlers have no normal predecessors and belong to exactly one `Try`; regions nest or are disjoint; no back edge targets a try block. | Negative fixtures for each rule, each diagnostic carrying the offending span. | Unverified |
| EH-07 | No protected block ends in a forced `ReturnCall`/`ReturnCallRef`; `mark_tail` skips calls in protected blocks. | A protected-region fixture stays an ordinary call plus return under the enabled profile; a hand-written forced tail terminator in a protected region is rejected. | Unverified |
| EH-08 | The MIR capability verifier requires `exceptions` when a tag exists or `Throw`/`Try` appears, and rejects a module requiring it under a disabled profile with a source-associated diagnostic. | Required-set inference tests; disabled-profile rejection diagnostic; `wasmtime_wasi_0_2()` unchanged in this slice. | Unverified |
| EH-09 | `mir/lower/exception.rs` discharges a call to a `psrs:runtime-exception` binding into the inline try region; when the profile disables `exceptions` the call is rejected at the binding's span. | Direct-MIR discharge fixtures execute under wasmtime (catch returns the handler's value; normal path the protected value). | Unverified |
| EH-10 | The structurer's region plan gains `UnitPlan::Try` with the dominance carve-out; try regions nest or are disjoint; reducible try-free functions are unchanged. | Structuring fixtures: nested tries, try inside loop, loop inside protected region; no dispatcher local on reducible input; WAT inspection shows `try_table` at the expected nesting. | Unverified |
| EH-11 | A function whose CFG is irreducible and contains a try region is rejected with a diagnostic; try-free irreducible input still uses the dispatcher. | Direct-MIR fixture pair: the rejection and the unchanged dispatcher fixture. | Unverified |
| EH-12 | `mir/budget.rs` fixes the weight formula and constants; `Function::frame_weight` is recorded at P9. | Unit test for `frame_weight`; MIR inspection shows the weight on each function when the profile enables `exceptions`. | Unverified |
| EH-13 | Every lowered function gains the prologue check and the epilogue decrement; `return_call*` sites decrement; trapping blocks do not. | WAT inspection: prologue before the first block, decrement on each return/tail path; a trapping block has no decrement; deep tail recursion still exits 0 (no false throw). | Unverified |
| EH-14 | Every `try_table` is preceded by a watermark save and its catch path begins with the restore. | WAT inspection per try region; the leak fixture (a throw/catch loop followed by deep recursion) executes with the deep recursion reaching the budget point, proving no leak. | Unverified |
| EH-15 | Budget/limit coordination: with the engine stack limit pinned via `-W max-wasm-stack` (CLI) or `Config::max_wasm_stack` (Rust API), deep non-tail recursion under `checkThrows` yields the compiler's exception, never `call stack exhausted`. | The coordination fixture executes under the pinned engine and limit; the recorded values (budget, weights, limit, engine version) are written into this record and the capability-profile notes. Failure as a trap means the constants are wrong and the row stays unverified. | Unverified |
| EH-16 | `Test.Assert.checkThrows` is implemented end to end: checked `psrs:runtime-exception#checkThrows` contract, P9 discharge, stdlib adaptation with its recorded target-adaptation entry, and `TCOMutRec.purs` executing (all four `assertThrows` catches, the four depth-100 `assertEqual`s, exit 0). | The corpus case executes under `PSRS_REQUIRE_WASMTIME=1`; the stdlib adaptation record names the pinned upstream declaration and the diff. | Unverified |
| EH-17 | The stable default profile enables `exceptions` (with the budget) as the adoption's final revision; [DEC-05](../../decision/DEC-05-wasmtime-feature-set.md)'s amendment and the capability-profile default table are updated with the evidence. | The full L6/M7 runtime board is re-run by the scoreboard protocol and the new number recorded in D-04; the disabled-profile rejection evidence and the enabled execution evidence are both cited in the profile's implementation notes. | Unverified |

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
