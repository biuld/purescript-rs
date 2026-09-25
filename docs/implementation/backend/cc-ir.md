# CC IR Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [CC IR](../../design/backend/fp/cc-ir.md)

**Progress:** Unverified; audit the current implementation before changing a row.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-01 and BE-02.

## Scope and dependencies

Complete the linked design's present-tense CC contract from verified Typed Core
through P8 and CC verification. The design's Model, Algorithms, Code map,
Invariants, and Boundaries remain authoritative if a row below misses an
obligation. This topic owns target-neutral ANF, closure conversion, abstract
representation requirements, external bindings, and their verifier. Concrete
GC layouts, MIR CFGs, WIT adaptation, and the pattern decision algorithm have
their own topics; exercise their interfaces here where CC must supply input.
Keep source spans and Core's left-to-right evaluation order. Use the design's
Code map as the organization target, recording justified equivalent ownership.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**. Every
Verified row needs the evidence record below; existing code alone is not
acceptance. A Blocked row names the dependency and resumption condition.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| CC-01 | P8 consumes verified Typed Core and emits a separate CC module with stable function symbols, parameters, ordered assignments, and source spans. | Trace a Core fixture through P8; reject missing symbols, duplicate definitions, or misplaced parameters in CC verification. | Unverified |
| CC-02 | ANF names intermediate computations in Core evaluation order, including callee before arguments, strict `let`, and branch-local work. | Side-effecting or trapping operands expose order in source-to-component execution; inspect CC assignment order and branch placement. | Unverified |
| CC-03 | CC values use target-neutral `ValueShape`, `ReprId`, and `SignatureId`; strings retain their semantic shape and nominal identities stay distinct. | Inspect interning and representation tables for equal/different keys, recursive reservations, and absence of Wasm heap indices or ABI pointer details in CC. | Unverified |
| CC-04 | Function signatures and representation requirements are complete before P9 and stable across linked source modules. | Compile mutually referring modules and recursive declarations; check one canonical handle per equivalent requirement and no unresolved handle. | Unverified |
| CC-05 | The full CC operation vocabulary has exact operand/result shapes and preserved spans. | Exercise constants, primitives, calls, products, variants, arrays, reference tests/casts, conversions, `If`, and unreachable/switch forms that the design permits; malformed fixtures reject wrong shapes. | Unverified |
| CC-06 | Lambda lifting computes ordered, deduplicated free captures and binds lifted parameters before body assignments. | Nested and escaping closures capture scalars, references, and functions; inspect capture order and execute later reads. | Unverified |
| CC-07 | Global function values, partial applications, and recursive function groups have callable wrappers and valid capture environments. | Execute direct and indirect calls, underapplication and overapplication, mutually recursive closures, and an escaping recursive function; check one evaluation of supplied operands. | Unverified |
| CC-08 | Erased/concrete function adapters use exact signatures and capture the adapted value once. | Inspect CC adapters in both directions and execute value-sensitive calls through the same P8-to-Wasm path; coordinate aggregate cases with [generic aggregate erasure](generic-aggregate-erasure.md). | Unverified |
| CC-09 | External bindings are validated against Core and CC, then projected without dropping valid unused declarations too early. | Positive imported-call and unused-binding fixtures; malformed names, signatures, duplicate bindings, and missing references fail at the correct boundary. | Unverified |
| CC-10 | CC verification checks every definition/use, assignment order, captures, calls, branches, representation operations, and aggregate plans. | Full-module negative fixtures for undefined/non-dominating values, wrong capture index/type, call arity/signature, branch shape, and invalid representation evidence. | Unverified |
| CC-11 | CC-to-P9 is explicit and preserves source ranges and one evaluation of each computation. | Inspect P9's input contract; run an executable component whose observable outputs distinguish repeated or reordered evaluation. | Unverified |
| CC-12 | In-scope failures produce source-associated diagnostics; malformed internal CC is rejected before MIR emission. | Assert diagnostic kind and span for unsupported input and verify internal-error paths on intentionally malformed CC. | Unverified |

## Vertical execution order

1. Audit Core-to-P8 contracts, existing CC operations, verifier rules, and
   tests; record each row's real gap. Implement the model and interning first.
2. Finish ANF, closure and adapter lowering, external bindings, and all CC
   operations needed by the design. Extend verifier checks with negative tests.
3. Compile through P9, Wasm validation, and component execution. Include
   value-sensitive order, capture, recursion, and cross-module cases. Run the
   normal optimizer; inspect pre-optimization CC when it can erase the evidence.
4. Re-audit every present-tense design rule and update the evidence record and
   D-04 status. Do not treat completion of this topic as completion of BE-01/02.

## Evidence record and completion rule

For each ID record implementation paths and entry points; exact test paths,
names, and assertions; input boundary (source, verified Typed Core, or malformed
CC); exact command, runtime version and executed/skipped cases; tested revision;
and remaining gaps. A Typed Core fixture proves backend behavior only: keep any
missing source path explicit. Runtime rows must execute with
`PSRS_REQUIRE_WASMTIME=1`; skipped tests leave those rows unverified. After Rust
changes run `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus the focused
mandatory Wasmtime cases. Finish only when every row is Verified and the design
audit finds no untracked present-tense requirement.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
CC-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Remaining work and blockers

Initial audit pending. No row is presumed complete from the current code map.
