# MIR Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [MIR](../../design/backend/fp/mir.md)

**Progress:** Unverified; audit the current implementation before changing a row.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-03, with BE-13 and BE-15 at the output boundary.

## Scope and dependencies

Complete MIR as the lowest long-lived, typed SSA/CFG representation, including
P9 lowering, representation planning, verification, and its P10 handoff. The
linked design's present-tense requirements remain authoritative beyond this
matrix. CC supplies target-neutral values; [data representation](data-representation.md)
owns physical GC layouts; [control flow and tail calls](control-flow-and-tail-calls.md)
owns general structuring and tail calls; WIT and linear memory own their ABI
rules. MIR must express and verify their required interfaces. Keep spans where
diagnostics or debugging need them and use the design's Code map as the target.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. A
Verified row requires named implementation and reproducible test evidence.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| MIR-01 | P9 consumes verified CC through an explicit conversion and creates one typed MIR module, not CC nodes with target fields. | Trace a CC fixture into MIR and inspect types, spans, imports, and absence of unresolved CC representation handles. | Unverified |
| MIR-02 | Functions contain typed basic blocks, block parameters, SSA instructions, and explicit terminators with unique IDs. | Positive CFG fixtures and negative duplicate/missing ID, missing terminator, and cross-function value tests. | Unverified |
| MIR-03 | Reachable `ReprId`s reserve and resolve to exact defined types in one recursion group without merging distinct nominal identities. | Recursive variant/array/record/closure layout fixtures; inspect one-to-one reachable mapping and valid Wasm type section. | Unverified |
| MIR-04 | P9 lowers scalar, product, variant, array, closure, erased, and canonical ABI shapes to exact MIR value types. | Table-driven positive and mismatch tests across every shape; verify concrete and canonical aggregate layouts separately. | Unverified |
| MIR-05 | MIR carries the required instruction families with exact input/output and effect contracts. | Check numeric, call/ref, GC, array, memory, import and conversion instructions; malformed modules reject invalid operand and result types. | Unverified |
| MIR-06 | CFG verification proves definition dominance, block-argument arity/types, Boolean branch selectors, switch tags/targets, and return types. | Full-module valid joins/loops and malformed non-dominating uses, bad edges, duplicate tags, and wrong return/signature fixtures. | Unverified |
| MIR-07 | Defined-type and reference subtyping obey Wasm mutability, finality, function variance, and nullability rules. | Positive/negative struct, array, function-ref, subtype and cast fixtures; include nullable load into non-null destination rejection. | Unverified |
| MIR-08 | Aggregate reconstruction uses typed helpers and private defaultable array allocation; every slot is initialized before exposure. | Inspect lowered maps, verify malformed early read/escape/incomplete-loop fixtures, and execute empty/nested conversions; coordinate evidence with [generic aggregate erasure](generic-aggregate-erasure.md). | Unverified |
| MIR-09 | External calls use checked import signatures and the selected canonical ABI boundary; unsupported capabilities fail before encoding. | Positive component import call and negative import/type/capability fixtures with source-associated failures where input has a span. | Unverified |
| MIR-10 | P10 consumes and returns verified MIR without changing its representation contract. | Verify before and after each enabled pass; differential execution on traps, calls, mutable arrays, imports, and aggregate reconstruction. | Unverified |
| MIR-11 | Wasm emission mechanically consumes verified MIR and does not make new layout decisions. | Validate emitted core modules/components, inspect representative GC/reference operations, and execute value-sensitive fixtures. | Unverified |
| MIR-12 | MIR diagnostics distinguish invalid compiler IR from unsupported valid source input. | Assert failure stage, diagnostic kind, and source span on representative malformed MIR and unsupported source fixtures. | Unverified |

## Vertical execution order

1. Audit the MIR model, P9 planner, instruction set, verifier, P10 boundary,
   and encoder consumers against every design section; assign evidence gaps.
2. Complete missing types, instructions, lowering, and verifier rules together.
   Add negative fixtures for every new verifier invariant.
3. Exercise each representation family through Wasm validation and mandatory
   component execution, including optimizer paths and malformed inputs.
4. Update evidence and D-04; the broader BE-03, BE-13, and BE-15 gates retain
   their own official-suite and capability scope.

## Evidence record and completion rule

For each ID record owning paths/functions, exact test names/assertions, input
boundary, commands and runtime version, executed/skipped cases, tested revision,
and gaps. A direct MIR fixture proves the MIR contract, not source coverage;
record missing source paths separately. Runtime evidence uses
`PSRS_REQUIRE_WASMTIME=1` and cannot be skipped. After Rust edits run
`cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus focused mandatory
Wasmtime tests. Verify all rows and re-audit the full present-tense design
before closing this topic.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
MIR-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Remaining work and blockers

Initial audit pending. Existing MIR support is a lead, not an acceptance claim.
