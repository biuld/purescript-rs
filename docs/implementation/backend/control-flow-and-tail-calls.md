# Control Flow and Tail Calls Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Control flow and tail calls](../../design/backend/fp/control-flow-and-tail-calls.md)

**Progress:** Unverified; audit the current implementation before changing a row.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-03, BE-05, BE-13, and BE-16.

## Scope and dependencies

Implement the linked design's general MIR CFG structuring and tail-call
contract through validated Wasm execution. This includes removing the temporary
`Branch { merge_block }` hint, deriving joins from CFG edges, reducible and
irreducible graph handling, switch lowering, self-tail loopification, and
feature-gated direct/indirect tail calls. MIR owns typed value semantics;
[pattern matching](pattern-matching.md) selects case edges. Multi-value,
exceptions, stack switching, branch hints, and unrelated optimization ideas
listed as future work are outside this topic. Use the design's Code map and
retain source spans on transformed calls and branches.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**;
record exact reproducible evidence for every Verified row.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| CF-01 | MIR terminators model Return, Jump, Branch, Switch, ReturnCall, and ReturnCallRef without a merge hint. | Inspect constructors and all producers/consumers; malformed old-style or missing-edge fixtures fail, and ordinary joins derive from CFG. | Unverified |
| CF-02 | CFG analysis computes reachability, dominators, backedges, natural loops, nesting, and joins from edges. | Diamond, nested-loop, multiple-exit, dead-block, and invalid graph fixtures assert computed regions and diagnostic locations. | Unverified |
| CF-03 | The structurer emits `block`/`loop`/`if` with valid depth-relative branches for every reducible CFG. | Validate and execute diamonds, loop-carried parameters, nested loops, early exits, and shared continuation cases; assert no dispatcher fallback on reducible input. | Unverified |
| CF-04 | Irreducible CFGs use the specified dispatcher fallback with preserved block parameters and edge semantics. | Direct MIR irreducible fixtures validate and execute through Wasm; inspect selector dispatch and compare results with a CFG interpreter or equivalent oracle. | Unverified |
| CF-05 | `Switch` preserves unique case tags, signed selector normalization, sparse/default behavior, and target block arguments. | Source and direct MIR fixtures cover dense nullary tags (`br_table`), sparse/negative tags, duplicate tags, defaults, and malformed target signatures. | Unverified |
| CF-06 | Self-tail recursion evaluates new arguments first and becomes a loop with correctly carried parameters and constant stack use. | Source or verified Core recursion with permutation/side effects; inspect MIR/Wasm loop and execute deep recursion beyond ordinary call-stack depth. | Unverified |
| CF-07 | Other direct and reference tail calls become ReturnCall/ReturnCallRef when the target enables tail calls. | Inspect exact call signatures and emitted Wasm opcodes; execute mutual and indirect tail recursion with value-sensitive results. | Unverified |
| CF-08 | A target without tail-call support lowers the same semantics to ordinary call plus return and emits no tail-call opcode. | Compile equivalent fixtures under both profiles, validate bytes and execute shallow cases; assert disabled capability rejection only for unsupported forced operations. | Unverified |
| CF-09 | MIR verification checks tail position, callee signature, return type, branch target arguments, dominance, and switch uniqueness. | Full-module negative fixtures for bad tail signatures, non-dominating call operands, wrong edge values, duplicate cases, and invalid selector types. | Unverified |
| CF-10 | P10 and Wasm lowering preserve CFG and tail-call semantics, traps, spans, and enabled feature profile. | Compare optimized/unoptimized results on loops, switches, recursive calls, and traps; validate emitted module/component with the selected profile. | Unverified |
| CF-11 | In-scope source case/recursion paths reach structured Wasm; direct MIR fixtures remain identified as such. | Track each executable input boundary and test the normal component pipeline with mandatory Wasmtime, including a deep recursion case. | Unverified |

## Vertical execution order

1. Inventory terminators and all CFG producers/consumers; remove the merge
   hint across MIR, verifier, optimizer, structurer, fixtures, and docs.
2. Complete graph analysis and structuring, then switch and tail-call lowering
   with verifier rules at each boundary.
3. Run source and direct MIR fixtures through validation and mandatory Wasmtime
   execution under enabled/disabled tail-call profiles. Re-audit the design.
4. Record evidence and update D-04 without promoting broader BE rows solely
   from this topic's focused tests.

## Evidence record and completion rule

For each ID record implementation entry points, named tests/assertions, input
boundary, exact commands and feature flags, runtime version, executed/skipped
cases, revision, and gaps. Direct MIR fixtures establish backend behavior only.
Run runtime cases with `PSRS_REQUIRE_WASMTIME=1`; skipped cases cannot verify a
row. After Rust changes run `cargo fmt --all --check`,
`cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`. Completion requires
every row Verified and a full present-tense design audit.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
CF-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Remaining work and blockers

Initial audit pending. The current structurer and merge hint require explicit
comparison with this design before assigning any Verified state.
