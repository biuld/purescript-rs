# Data Representation Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Data representation](../../design/backend/fp/data-representation.md)

**Progress:** Unverified; audit existing implementation and tests first.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-05 through BE-10 and BE-15.

## Scope and dependencies

Complete the physical Wasm GC layouts and P9 operation lowering in the linked
design. Its present-tense Model, Algorithms, Code map, Invariants, and
Boundaries are authoritative beyond this matrix. CC provides abstract shapes;
[MIR](mir.md) owns the typed IR and verifier; [generic aggregate erasure](generic-aggregate-erasure.md)
owns conversions between nominal aggregate layouts; [polymorphism and erasure](polymorphism-and-erasure.md)
owns erased values. This topic must implement the concrete interfaces they
require. Linear memory holds ABI bytes, not language heap values. Keep distinct
nominal identities and use the design's Code map as the organization target.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. A
Verified row needs named implementation, negative verification, and execution
evidence where behavior is observable.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| DR-01 | P9 maps every CC scalar, string, reference, erased, and closure shape to the specified MIR value type. | Table-driven layout tests for `Int`, `Number`, `Boolean`, `Char`, `Unit`, `String`, products, variants, arrays, closures, and erased values; reject mismatches. | Unverified |
| DR-02 | Planner reserves all reachable nominal types in one recursion group and maps each `ReprId` to one `DefinedTypeId`. | Recursive and mutually referring record/array/variant/closure fixtures inspect type identities, forward references, and valid Wasm type section. | Unverified |
| DR-03 | Products and closed records are structs with canonical field order and correct mutability; pure update creates a new value. | Construct, project, pattern match, and update mixed fields; retain the old value and execute assertions on both. | Unverified |
| DR-04 | Field-bearing sums use an abstract tag-carrying supertype and final case subtypes; all-nullary sums use immediate `i32` tags. | Inspect type hierarchy and tag positions; execute nullary, single/multiple-field and nested ADT cases; reject wrong tag/field selection. | Unverified |
| DR-05 | Newtype representation erases the wrapper exactly where the design specifies. | Inspect CC/MIR for absence of wrapper allocation and execute construction/matching through nested uses. | Unverified |
| DR-06 | Arrays use mutable GC element storage but source `ArraySet` is a pure clone/update; canonical and concrete layouts remain distinct. | Execute empty, singleton, nested, and aliased update/read cases; inspect `array.new`, `get`, `set`, clone and type indices. | Unverified |
| DR-07 | Closures use a code reference and uniform nullable `eqref` capture array with exact capture boxing. | Inspect layout, capture ordering and code signature; execute escaping closures capturing each scalar class and GC references. | Unverified |
| DR-08 | Scalar boxes and erased/reference recovery obey exact nullability and nominal provenance rules. | Positive and negative `ref.test`/`ref.cast`, box/unbox, i31 Boolean capture, full-width integer, and Number paths; no nominal cast substitutes for aggregate reconstruction. | Unverified |
| DR-09 | Product, variant, array, closure, and conversion operations lower to exact typed MIR instructions. | Full-module verifier rejects wrong operand, field/index, mutability, arity, nullability, and layout; valid cases validate as Wasm. | Unverified |
| DR-10 | Private defaultable aggregate allocation cannot escape before full initialization. | Malformed MIR early-read/return/branch fixtures and nested conversion execution; coordinate proof with [generic aggregate erasure](generic-aggregate-erasure.md). | Unverified |
| DR-11 | Capability flags reject operations requiring disabled GC, references, or typed function references before emission. | Compile representative operations with each feature disabled; assert named failure and no invalid artifact. | Unverified |
| DR-12 | Emitted core module and component execute representative reachable layouts without relying on WAT text alone. | Mandatory Wasmtime cases for products, sums, arrays, closures, erased fields and conversion paths, with value-sensitive assertions. | Unverified |

## Vertical execution order

1. Audit CC shapes, P9 planner, MIR operations/verifier, target profile and
   executable fixtures against every design section.
2. Complete each layout and corresponding operation/verifier rules together;
   add malformed fixtures before accepting a representation family.
3. Validate and execute artifacts for each reachable family, including
   aliases, nested values, capability failures, and generic/concrete paths.
4. Record evidence and update D-04. Broader source syntax, open rows, WIT
   aggregates, and official-suite gates retain their own owners.

## Evidence record and completion rule

For every ID record code paths/functions, exact tests/assertions, input
boundary, commands, Wasmtime version, executed/skipped cases, tested revision,
and gaps. Typed Core or direct MIR fixtures establish only their entry boundary.
Runtime cases require `PSRS_REQUIRE_WASMTIME=1`; skipped cases are unverified.
After Rust edits run `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus focused mandatory
runtime cases. Close only when every row and every present-tense design rule
has evidence.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
DR-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Remaining work and blockers

Initial audit pending. The accepted generic aggregate topic is a dependency,
not proof that all physical representation families meet this checklist.
