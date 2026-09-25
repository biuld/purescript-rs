# Scalars and Primitives Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Scalars and numeric operations](../../design/backend/fp/scalars-and-primitives.md)

**Progress:** Unverified; audit existing implementation and tests first.

**Roadmap:** [D-04 backend matrix](../../design/D-04-suite-roadmap.md#backend-feature-matrix), primarily BE-04; FE-08 supplies source typing.

## Scope and dependencies

Implement the linked design's complete scalar value model, CC/MIR operation
vocabulary, helper generation, verifier rules, and Wasm numeric semantics.
`String` is an `i32` ABI pointer with a distinct CC semantic shape; byte
encoding and memory ownership belong to the linear-memory design. The design's
present-tense rules remain authoritative beyond the matrix. Source operator
syntax/type checking belongs to the frontend, but this topic must accept all
valid typed primitive inputs and report unsupported ones accurately.

## Acceptance matrix

States are **Unverified**, **In progress**, **Blocked**, and **Verified**.
Verified requires exact test and executed result evidence.

| ID | Design obligation | Required acceptance evidence | State |
| --- | --- | --- | --- |
| SP-01 | `Int` is wrapping signed 32-bit; `Number` is IEEE binary64; Boolean is canonical 0/1; Char is a Unicode scalar; Unit has its fixed representation. | Boundary-value and malformed-type tests for each shape, including high-bit Int, NaN/infinity, invalid Char, and Boolean normalization. | Unverified |
| SP-02 | `String` keeps a separate semantic CC shape despite its `i32` runtime pointer. | Positive string literal/import use and verifier rejection of numeric operations on String pointers. | Unverified |
| SP-03 | Every specified CC unary/binary primitive has an exact MIR instruction or helper lowering. | Exhaustive operation table matching the design vocabulary; reject missing opcode mappings and wrong operand/result types. | Unverified |
| SP-04 | Integer add/subtract/multiply and bitwise operations wrap at 32 bits; shift counts follow the specified modulo-32 behavior. | Source or verified Core execution at overflow/underflow and shift counts 0, 31, 32, 33; compare exact bits/results. | Unverified |
| SP-05 | Integer quotient/remainder truncate toward zero and trap for divisor zero and signed minimum divided by -1. | Positive signed combinations and expected-trap component cases; distinguish quotient/remainder from floor division/modulo. | Unverified |
| SP-06 | Integer division/modulo helpers implement floor quotient and divisor-signed remainder without introducing unrelated traps. | Positive/negative dividend-divisor matrix, zero/overflow trap cases, helper interning, and execution with multiple call sites. | Unverified |
| SP-07 | Number arithmetic, comparisons, and unary operations retain Wasm/IEEE semantics including NaN and signed zero. | Value-sensitive f64 execution, bitwise inspection when sign of zero matters, and expected NaN comparison behavior. | Unverified |
| SP-08 | Boolean and character operations use their canonical shape and reject invalid values at appropriate boundaries. | True/false logical cases, Unicode scalar boundary cases, and malformed CC/MIR operation fixtures. | Unverified |
| SP-09 | `NumberToInt` truncates with saturation for finite overflow, infinities, and NaN; `IntToNumber` follows exact signed conversion. | Execute thresholds around both i32 limits, fractions, infinities, NaN, and full-width integers; assert exact results. | Unverified |
| SP-10 | Generated helpers are emitted only when reachable, with collision-free symbols and deterministic scans through nested expressions. | Compare modules with/without div/mod/conversion, nested `If` uses, user-symbol collisions, and repeated calls. | Unverified |
| SP-11 | CC and MIR verifiers enforce exact scalar operand/result and helper-call signatures before encoding. | Negative full-module fixtures for mixed Int/Number, Boolean/i32 confusion, String arithmetic, wrong conversion types, and malformed helper calls. | Unverified |
| SP-12 | Normal optimization and Wasm encoding preserve values and expected traps under the selected target profile. | Execute optimized/unoptimized representative operations, validate core module/component, and compare against a reference arithmetic oracle. | Unverified |

## Vertical execution order

1. Audit the complete design operation tables against CC, MIR, helpers,
   verifiers, source inputs, and existing tests; record each uncovered opcode.
2. Implement missing operations and exact checks together. Add boundary,
   negative, and trap tests before considering a family complete.
3. Execute value-sensitive component cases for every semantic family and
   compare normal optimization with a path retaining the operation.
4. Update evidence and D-04; broader official-suite and frontend operator
   gates remain separate.

## Evidence record and completion rule

For each ID record code paths/functions, exact test names/assertions, input
boundary, commands, Wasmtime version, executed/skipped cases, revision, and
gaps. Distinguish expected traps from compiler errors and runtime skips. Use
`PSRS_REQUIRE_WASMTIME=1` for required execution. After Rust edits run
`cargo fmt --all --check`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`, plus focused mandatory
runtime tests. Close only after every row and full present-tense design audit
is Verified.

Use this fillable record for each acceptance ID (one test may support several IDs):

```text
SP-xx:
  Implementation: repository paths and owning entry points
  Tests: repository paths, exact names, and assertions exercised
  Input boundary: source / verified Typed Core / malformed CC or MIR
  Commands: exact reproducible commands and required environment
  Result: pass/fail, executed runtime cases, skipped cases and reasons
  Revision: tested commit plus any uncommitted changes
  Gaps: remaining requirements, or none
```

## Remaining work and blockers

Initial audit pending. Current primitive subset does not establish complete
design coverage without the operation-table audit.
