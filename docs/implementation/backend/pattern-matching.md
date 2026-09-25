# Pattern Matching Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Pattern matching](../../design/backend/fp/pattern-matching.md)

**Progress:** Unverified; audit existing implementation and tests first.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-05 and BE-06, with BE-08 and BE-09 for generic fields.

## Scope and dependencies

Complete the linked design's pattern matrix, decision DAG, coverage analysis,
and CC realization for patterns expressible by verified Typed Core. The design
is authoritative beyond this matrix. Typed pattern construction and source
syntax belong to the frontend; [data representation](data-representation.md)
owns constructor layouts; [generic aggregate erasure](generic-aggregate-erasure.md)
owns recovery of dependent aggregate fields; [control flow and tail calls](control-flow-and-tail-calls.md)
owns MIR/Wasm switch structuring. Track source and backend fixture evidence
separately. Literal, guard, view, tuple, array, as, and or patterns listed as
future work are not silently promoted to present source support.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. Each
Verified row needs exact implementation, test and execution evidence.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| PM-01 | A case evaluates each scrutinee exactly once and preserves left-to-right, top-to-bottom first-match order. | Source and verified Core cases with side effects or traps, duplicate constructor rows, and an irrefutable first row; inspect CC evaluation order and execute outcomes. | Unverified |
| PM-02 | Matrix rows keep source indices, typed columns, residual slots, and constructor/record specialization rules. | Unit cases for wildcard, variable, constructor, record and nested combinations; assert specialized/default matrices and bindings. | Unverified |
| PM-03 | Decision compilation chooses a useful column, specializes irrefutable rows correctly, and hashes equivalent normalized residual states. | Inspect DAG size/sharing for repeated subproblems and assert first-row irrefutable base case; compare against a simple first-match oracle. | Unverified |
| PM-04 | Coverage and usefulness detect exhaustive, missing, and redundant rows with a valid nested witness and source span. | Exhaustive/non-exhaustive recursive ADTs, nested records, duplicate rows, and impossible branches; validate warning/error location and witness usefulness. | Unverified |
| PM-05 | Decision DAG realizes constructor tests, tags, projections, variable binds, records, and newtype erasure through ordinary CC operations. | Inspect generated CC for each pattern family; execute field-bearing and nested cases, including failure edges and no redundant projection. | Unverified |
| PM-06 | Nullary sums dispatch through CC TagSwitch, MIR Switch, and Wasm `br_table` when tags are unique; duplicates preserve source order. | Inspect all three stages, validate bytes, and execute dense/default/duplicate-tag cases with value-sensitive results. | Unverified |
| PM-07 | Field-bearing sums and records perform tag/shape tests before projections and use typed branch results. | Malformed CC/MIR fixtures reject wrong tag, field, branch shape, or nondominating projection; valid nested cases execute. | Unverified |
| PM-08 | Dependent erased scalar and aggregate fields recover according to their declared templates, never by unsafe nominal cast. | Generic ADT/record pattern cases over at least two instantiations, inspect conversion plans and execute recovered payload values; coordinate accepted aggregate cases. | Unverified |
| PM-09 | An impossible missing edge becomes typed CC/MIR Unreachable and Wasm unreachable; real missing cases yield diagnostics. | Exhaustive and non-exhaustive fixtures inspect reachability, diagnostic spans, and expected runtime behavior. | Unverified |
| PM-10 | Pattern compilation terminates on recursive types and preserves useful sharing without changing first-match semantics. | Recursive ADT and nested pattern stress fixtures with bounded compile time/DAG size and oracle comparison. | Unverified |
| PM-11 | Optimization and Wasm lowering preserve warnings, source order, and selected-branch values. | Compare pre/post optimization execution on duplicate, nested, and generic cases; assert diagnostics remain source-associated. | Unverified |

## Vertical execution order

1. Audit typed pattern inputs, matrix/coverage/DAG code, CC realizer, MIR
   switch verifier, and Wasm encoder against every present-tense design rule.
2. Finish analysis and lowering as one vertical slice per pattern family;
   add negative verifier fixtures and source-order oracle cases.
3. Run optimized component execution for representative constructor, record,
   newtype, nested, recursive, and generic cases with mandatory Wasmtime.
4. Record evidence and update D-04. Frontend pattern syntax and official-suite
   landing remain independently tracked.

## Evidence record and completion rule

For every ID record code paths/functions, named tests/assertions, input
boundary, commands, Wasmtime version, executed/skipped cases, revision, and
gaps. Typed Core fixtures prove backend pattern handling only. Runtime cases
must execute with `PSRS_REQUIRE_WASMTIME=1`; skips leave rows unverified.
After Rust changes run `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus focused runtime
tests. Close only after all rows and the complete present-tense design pass.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
PM-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Remaining work and blockers

Initial audit pending. Prior generic aggregate acceptance is relevant to PM-08
but does not verify the whole pattern decision contract.
