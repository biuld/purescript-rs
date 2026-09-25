# Polymorphism and Erasure Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Polymorphism and erasure](../../design/backend/fp/polymorphism-and-erasure.md)

**Progress:** Unverified; audit existing implementation and tests first.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-02 and BE-08; FE-09 supplies typed polymorphic input.

## Scope and dependencies

Complete rank-1 erased-value representation, signature interning, scalar
boxing, reference recovery, closure capture, and higher-order function
adapters in the linked design. The design's full present-tense contract applies
even when a row below is missing. Typed Core owns type checking and dictionary
evidence; [generic aggregate erasure](generic-aggregate-erasure.md) owns
recursive array/closed-record conversion; [data representation](data-representation.md)
owns physical GC layouts. Erased values must not cross the WIT canonical ABI.
Keep the erased fallback even if optimization specializes some calls.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**;
existing code or a fixture that only inspects WAT does not verify execution.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| PE-01 | A bare type variable has non-null erased `eqref` shape, while generic aggregates normalize recursively to canonical layouts. | Inspect CC shapes for `a`, `Array a`, nested arrays, dependent records and ADT fields; reject conflating bare erasure with aggregate identity. | Unverified |
| PE-02 | Equal normalized function signatures share `SignatureId` and MIR function type; closure receiver and capture conventions are exact. | Equivalent/different signature interning cases, indirect call verification, and emitted type-index inspection. | Unverified |
| PE-03 | Integer, Boolean, and String use full-width `i32` boxes at erased boundaries; Number uses `f64` box. | Value-sensitive round trips including `Int` extremes, both Booleans, signed zero/NaN policy where relevant, and nonempty String contents. | Unverified |
| PE-04 | GC references erase by upcast and recover by checked shape/provenance without avoidable allocation. | Inspect CC/MIR for reference-only conversions; execute valid ADT/array/record/closure identity and reject wrong nominal recovery. | Unverified |
| PE-05 | `i31` is reserved for Boolean capture encoding and never substitutes for a general boxed full-width `Int`. | Capture and erased-call fixtures with high-bit integer values; inspect emitted boxing operations. | Unverified |
| PE-06 | Direct generic calls adapt argument and result representations at the declaration signature and caller instantiation. | One generic body called at multiple scalar and reference instantiations; execute payload-sensitive results and inspect boundary plans. | Unverified |
| PE-07 | Concrete-to-generic and generic-to-concrete function adapters convert each argument and result at invocation with exact arity/signatures. | Execute both adapter directions, mixed scalar/reference arguments, returned function values, and repeated calls; prove original function value evaluated once. | Unverified |
| PE-08 | Closure captures use the uniform nullable `eqref` array and exact per-shape encoding/decoding. | Escaping closures capture Int, Boolean, Number, String, reference, and already-erased values; execute later reads and inspect no double boxing. | Unverified |
| PE-09 | RepresentationTest/Cast is restricted to valid erased boundaries and cannot replace nominal aggregate reconstruction. | CC/MIR verifier negative fixtures for unrelated nominal layouts, wrong box kind, nullability, and signature; coordinate positive aggregate cases with [generic aggregate erasure](generic-aggregate-erasure.md). | Unverified |
| PE-10 | CC and MIR verifiers reject malformed adapters, captures, calls, and unresolved representation requirements. | Full-module negative fixtures for wrong signature, capture index/type, arity, cast provenance, and result shape before Wasm emission. | Unverified |
| PE-11 | Erased values are recovered before canonical WIT calls; optimized and unspecialized execution agree. | Source or verified Core fixture crossing a concrete ABI call, plus execution retaining an erased generic path and normal optimized execution. | Unverified |

## Vertical execution order

1. Audit typed Core inputs, CC normalization/signatures, P9 boxes/captures,
   verifiers, and current runtime fixtures against the design.
2. Finish exact scalar/reference conversions and adapters with malformed
   verifier tests; preserve the existing generic aggregate acceptance path.
3. Execute unspecialized and optimized component cases with payload-sensitive
   assertions, then check WIT boundaries and source-vs-Core coverage.
4. Record all evidence and update D-04 without treating this topic as the
   entire FE-09 or official backend suite gate.

## Evidence record and completion rule

For each ID record implementation paths/entry points; exact tests and
assertions; input boundary; commands, runtime version and actual executions;
revision; and gaps. Verified Core fixtures do not establish source support.
Runtime cases use `PSRS_REQUIRE_WASMTIME=1`; skips leave rows unverified. After
Rust edits run `cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus the mandatory
focused runtime cases. Complete only after every row and every present-tense
design obligation is verified.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
PE-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Remaining work and blockers

Initial audit pending. Existing generic aggregate acceptance is related
evidence but does not automatically close these erasure and adapter rows.
