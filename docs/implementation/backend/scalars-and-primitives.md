# Scalars and Primitives Implementation Acceptance

**Feature:** [F-02](../../feature/F-02-portable-programs.md)

**Design:** [Scalars and numeric operations](../../design/backend/fp/scalars-and-primitives.md)

**Progress:** Audited. SP-03..SP-11 are Verified; SP-01, SP-08, and SP-12 are
In progress; SP-02 is Blocked on the CC `ValueShape::String` design gap. See the
evidence records below.

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
| SP-01 | `Int` is wrapping signed 32-bit; `Number` is IEEE binary64; Boolean is canonical 0/1; Char is a Unicode scalar; Unit has its fixed representation. | Boundary-value and malformed-type tests for each shape, including high-bit Int, NaN/infinity, invalid Char, and Boolean normalization. | In progress |
| SP-02 | `String` keeps a separate semantic CC shape despite its `i32` runtime pointer. | Positive string literal/import use and verifier rejection of numeric operations on String pointers. | Blocked |
| SP-03 | Every specified CC unary/binary primitive has an exact MIR instruction or helper lowering. | Exhaustive operation table matching the design vocabulary; reject missing opcode mappings and wrong operand/result types. | Verified |
| SP-04 | Integer add/subtract/multiply and bitwise operations wrap at 32 bits; shift counts follow the specified modulo-32 behavior. | Source or verified Core execution at overflow/underflow and shift counts 0, 31, 32, 33; compare exact bits/results. | Verified |
| SP-05 | Integer quotient/remainder truncate toward zero and trap for divisor zero and signed minimum divided by -1. | Positive signed combinations and expected-trap component cases; distinguish quotient/remainder from floor division/modulo. | Verified |
| SP-06 | Integer division/modulo helpers implement floor quotient and divisor-signed remainder without introducing unrelated traps. | Positive/negative dividend-divisor matrix, zero/overflow trap cases, helper interning, and execution with multiple call sites. | Verified |
| SP-07 | Number arithmetic, comparisons, and unary operations retain Wasm/IEEE semantics including NaN and signed zero. | Value-sensitive f64 execution, bitwise inspection when sign of zero matters, and expected NaN comparison behavior. | Verified |
| SP-08 | Boolean and character operations use their canonical shape and reject invalid values at appropriate boundaries. | True/false logical cases, Unicode scalar boundary cases, and malformed CC/MIR operation fixtures. | In progress |
| SP-09 | `NumberToInt` truncates with saturation for finite overflow, infinities, and NaN; `IntToNumber` follows exact signed conversion. | Execute thresholds around both i32 limits, fractions, infinities, NaN, and full-width integers; assert exact results. | Verified |
| SP-10 | Generated helpers are emitted only when reachable, with collision-free symbols and deterministic scans through nested expressions. | Compare modules with/without div/mod/conversion, nested `If` uses, user-symbol collisions, and repeated calls. | Verified |
| SP-11 | CC and MIR verifiers enforce exact scalar operand/result and helper-call signatures before encoding. | Negative full-module fixtures for mixed Int/Number, Boolean/i32 confusion, String arithmetic, wrong conversion types, and malformed helper calls. | Verified |
| SP-12 | Normal optimization and Wasm encoding preserve values and expected traps under the selected target profile. | Execute optimized/unoptimized representative operations, validate core module/component, and compare against a reference arithmetic oracle. | In progress |

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

## Evidence records

Worktree:
`/private/var/folders/f2/h3z3zsy51xq3kr06707r9ct40000gn/T/opencode/psrs-wt/scalars-primitives`.
Wasmtime `49.0.0 (17830bd3f53 2026-09-21)`. Revision `cc5d0f4` plus the
uncommitted changes described here. Results were reproduced with an isolated
`CARGO_TARGET_DIR` because the shared warm target directory can alias test
binaries between checkouts. Common commands:

```sh
export CARGO_TARGET_DIR=/Users/biu/Projects/purescript-rs/target
cargo test --manifest-path <worktree>/Cargo.toml -p psrs-backend
PSRS_REQUIRE_WASMTIME=1 cargo test --manifest-path <worktree>/Cargo.toml -p psrs-driver --lib
cargo fmt --manifest-path <worktree>/Cargo.toml --all --check
cargo clippy --manifest-path <worktree>/Cargo.toml --workspace --all-targets -- -D warnings
```

```text
SP-01:
  Implementation: cc/layout/scalar.rs (Type -> ValueShape), mir/layout/mod.rs
    (Integer -> I32, Number -> F64, Boolean -> Boolean)
  Tests: mir/gc_tests/binary_matrix (IntAdd/Sub/Mul wrap), driver
    tests/scalars.rs::scalar_intrinsics... (checkIntWrapping), ...::
    number_to_int_saturates... (NaN/+-infinity), ...::char_operations_preserve_bmp_scalar_values,
    mir/verify/tests/scalar.rs::rejects_a_boolean_constant_that_is_not_canonical
  Input boundary: source, CC fixtures, malformed MIR
  Commands: common commands above
  Result: pass; executed under Wasmtime
  Revision: cc5d0f4 + uncommitted
  Gaps: Unit-shaped values are not constructible from the current source
    bootstrap, so the canonical `0` representation has no executed fixture;
    astral Unicode code points (`U+10000..=U+10FFFF`) are rejected by the
    frontend lexer (`astral code point in character literal`), which is outside
    this topic's ownership.
```

```text
SP-02:
  Implementation: cc/representation.rs has no `ValueShape::String`;
    cc/layout/scalar.rs and cc/mod.rs map `Type::String`/`SourceType::String`
    to `ValueShape::Integer`, and cc/verify/ops/mod.rs requires
    `StringConstant` to produce `ValueShape::Integer`
  Tests: none; the design-mandated distinction does not exist in code
  Input boundary: n/a
  Commands: n/a
  Result: blocked, not executed
  Revision: cc5d0f4 + uncommitted
  Gaps: the design ([CC IR](../../design/backend/fp/cc-ir.md),
    [scalars](../../design/backend/fp/scalars-and-primitives.md)) requires a
    distinct `ValueShape::String` so the CC verifier rejects arithmetic on
    string pointers. Adding the variant changes the shared representation model
    and the erased-protocol/string-ABI lowering, so it is left to the owning
    topics rather than patched here.
```

```text
SP-03:
  Implementation: cc/scalar.rs (full CC vocabularies), mir/numeric.rs
    (`From<cc::UnaryOp>`, `TryFrom<cc::BinaryOp>`), mir/scalar_helpers.rs,
    wasm/lower/structure/ops.rs::primitive, wasm/lower/structure/unary.rs
  Tests: mir/gc_tests/binary_matrix::verifies_and_executes_every_cc_binary_scalar_variant_on_both_targets
    (all CC binary variants), mir/gc_tests/unary::lowers_unary_and_conversion_operations_on_both_targets
    (all 10 CC unary variants), driver tests/scalars.rs::scalar_intrinsics_are_reachable_from_source_and_execute_with_documented_semantics
    (asserts every operation name appears in lowered Core)
  Input boundary: source and CC fixtures
  Commands: common commands above
  Result: pass; executed
  Revision: cc5d0f4 + uncommitted
  Gaps: none
```

```text
SP-04:
  Implementation: wasm/lower/structure/ops.rs (`i32.add/sub/mul/and/or/xor/shl/shr_s/shr_u`)
  Tests: driver tests/scalars.rs::shift_counts_are_taken_modulo_32 asserts
    `1 shl 32 = 1`, `1 shl 33 = 2`, `3 shl 31 = i32.min`,
    `(-8) shr 33 = -4`, `(-1) zshr 1 = 2147483647`; ...::scalar_intrinsics... 
    asserts `2147483647 + 1 = intNeg 2147483647 - 1`; binary_matrix checks
    IntAdd/Sub/Mul/And/Or/Xor/Shl/Shr/Zshr exact values
  Input boundary: source and executed Wasm
  Commands: common commands above
  Result: pass; executed under Wasmtime
  Revision: cc5d0f4 + uncommitted
  Gaps: none
```

```text
SP-05:
  Implementation: mir/numeric.rs IntQuot/IntRem -> I32DivS/I32RemS;
    wasm/lower/structure/ops.rs
  Tests: mir/gc_tests/binary_matrix (IntQuot 7 2 = 3, IntRem 7 2 = 1);
    driver tests/scalars.rs::truncated_and_floor_division_trap_on_zero_divisor_and_signed_overflow
    asserts runtime traps `integer divide by zero` for `1 / 0`, `1 % 0`,
    `intDiv 1 0`, `intMod 1 0` and `integer overflow` for
    `i32.min / -1` through both `intQuot` and `intDiv`
  Input boundary: source and executed Wasm
  Commands: common commands above
  Result: pass; executed under Wasmtime
  Revision: cc5d0f4 + uncommitted
  Gaps: none
```

```text
SP-06:
  Implementation: mir/scalar_helpers.rs (`lower_scalar_helpers`,
    `contains_operation`, `euclidean_helper`, `ScalarHelpers::binary_instruction`)
  Tests: mir/gc_tests/div_mod::lowers_euclidean_integer_division_and_modulo_on_both_targets
    (all four dividend/divisor sign combinations, expected 3),
    ...::detects_division_and_modulo_nested_in_tag_switch_cases (case-nested
    `intDiv`/`intMod` lower and execute to 4),
    driver tests/scalars.rs::generates_floor_helpers_for_division_nested_in_case_branches
    (asserts both helpers exist once and `7 div 2 = 3` at runtime)
  Input boundary: CC fixtures and source
  Commands: common commands above
  Result: pass; executed under Wasmtime; before the `contains_operation`
    fix the nested case failed with `missing MIR helper for scalar operation
    IntDiv`
  Revision: cc5d0f4 + uncommitted (fix: recurse into `TagSwitch` cases and the
    default arm, not only `If`/`Primitive`)
  Gaps: none
```

```text
SP-07:
  Implementation: mir/numeric.rs F64 mapping, wasm/lower/structure/ops.rs
    (`f64.add/sub/mul/div/eq/ne/lt/le/gt/ge`), wasm/lower/structure/unary.rs F64Neg
  Tests: mir/gc_tests/binary_matrix (exact Number results/comparisons);
    driver tests/scalars.rs::scalar_intrinsics... `checkNaN` (`0/0 /= 0/0`);
    ...::number_operations_preserve_signed_zero (`-0.0 == 0.0` true and
    `1/0 > 1/-0` true)
  Input boundary: source and executed Wasm
  Commands: common commands above
  Result: pass; executed under Wasmtime
  Revision: cc5d0f4 + uncommitted
  Gaps: none
```

```text
SP-08:
  Implementation: cc/verify/scalar.rs, mir/verify/instruction/primitive.rs,
    mir/verify/instruction/unary.rs, mir/verify/instruction/mod.rs
    (Boolean constant canonical check)
  Tests: mir/gc_tests/binary_matrix (BooleanAnd/Or/Eq/Ne, CharEq..CharGe);
    driver tests/scalars.rs::char_operations_preserve_bmp_scalar_values
    (`'A'`, `'é'`, `U+FFFD`); cc/verify/tests.rs::rejects_boolean_logic_on_integer_operands
    and ::rejects_a_comparison_with_an_integer_result;
    mir/verify/tests/scalar.rs::rejects_boolean_logic_on_i32_operands and
    ::rejects_a_boolean_constant_that_is_not_canonical
  Input boundary: source, malformed CC, malformed MIR
  Commands: common commands above
  Result: pass; executed under Wasmtime
  Revision: cc5d0f4 + uncommitted
  Gaps: astral Unicode scalar literals are rejected by the frontend lexer
    before reaching the backend; invalid Char remains a type-checking
    invariant.
```

```text
SP-09:
  Implementation: mir/numeric.rs I32ToF64/F64ToI32Sat;
    wasm/lower/structure/unary.rs `emit_saturating_f64_to_i32`
  Tests: mir/gc_tests/unary::lowers_unary_and_conversion_operations_on_both_targets
    (NaN -> 0, `2^31` -> i32.max, `-2^31-0.5` -> i32.min);
    driver tests/scalars.rs::number_to_int_saturates_at_infinities_nan_and_the_i32_bounds
    asserts 2147483647.9 -> 2147483647, -2147483648.0/-2147483648.9 -> i32.min,
    +inf -> i32.max, -inf -> i32.min, NaN -> 0, -3.9 -> -3
  Input boundary: CC fixture and source
  Commands: common commands above
  Result: pass; executed under Wasmtime
  Revision: cc5d0f4 + uncommitted
  Gaps: none
```

```text
SP-10:
  Implementation: mir/scalar_helpers.rs `lower_scalar_helpers`
  Tests: mir/gc_tests/div_mod::does_not_emit_helpers_without_division_or_modulo
    (a module using only IntAdd/IntQuot keeps one function);
    ...::skips_helper_symbols_used_by_module_functions (a module symbol at
    `(ModuleId(0), u32::MAX)` forces the allocator to skip it; helper symbols
    stay distinct); ...::detects_division_and_modulo_nested_in_tag_switch_cases
    (nested case); ...::detects_modulo_nested_in_a_tag_switch_default_arm
    (nested default arm); ...::lowers_euclidean_integer_division_and_modulo_on_both_targets
    (repeated call sites)
  Input boundary: CC fixtures
  Commands: common commands above
  Result: pass; executed under Wasmtime
  Revision: cc5d0f4 + uncommitted
  Gaps: none. Conversion helpers named in the matrix belong to
    generic-aggregate-erasure and are unchanged.
```

```text
SP-11:
  Implementation: cc/verify/scalar.rs, cc/verify/ops/mod.rs,
    mir/verify/instruction/primitive.rs, mir/verify/instruction/unary.rs,
    mir/verify/instruction/mod.rs, mir/verify/mod.rs (call signatures)
  Tests: cc/verify/tests.rs::rejects_integer_arithmetic_on_number_operands,
    ::rejects_number_comparisons_on_integer_operands,
    ::rejects_boolean_logic_on_integer_operands,
    ::rejects_a_comparison_with_an_integer_result,
    ::rejects_int_to_number_on_a_number_operand;
    mir/verify/tests/scalar.rs::rejects_i32_arithmetic_on_f64_operands,
    ::rejects_f64_arithmetic_on_i32_operands,
    ::rejects_boolean_logic_on_i32_operands,
    ::rejects_an_integer_comparison_that_produces_i32,
    ::rejects_a_boolean_constant_that_is_not_canonical,
    ::rejects_a_call_with_the_wrong_argument_type
  Input boundary: malformed CC and MIR
  Commands: common commands above
  Result: pass
  Revision: cc5d0f4 + uncommitted
  Gaps: String arithmetic rejection (SP-02) cannot be verified until
    `ValueShape::String` exists.
```

```text
SP-12:
  Implementation: psrs-backend compile pipeline (Core optimization, P9 MIR
    optimization, Wasm structure/encode, wasmparser validation)
  Tests: every driver tests/scalars.rs execution test runs the optimized
    artifact through `compile_source`/Wasmtime; the negative trap test asserts
    traps survive optimization; mir/gc_tests run the validator
  Input boundary: source and executed Wasm
  Commands: common commands above
  Result: pass; executed under Wasmtime
  Revision: cc5d0f4 + uncommitted
  Gaps: no explicit unoptimized-vs-optimized comparison or independent
    arithmetic oracle in this suite; represented by optimized execution and
    exact-value assertions only.
```

## Remaining work and blockers

- SP-02: add `ValueShape::String` (or an equivalent semantic string shape) in
  the owning CC-representation and string/ABI topics, then reject scalar
  primitives on it in `cc/verify/scalar.rs` and update
  `cc/verify/ops/mod.rs`.
- SP-01: provide a reachable Unit value (or a typed Core/CC fixture) that
  executes the canonical `Int`/`I32` `0` representation.
- SP-12: add an explicit unoptimized/optimized comparison and a reference
  arithmetic oracle for representative operations.
- Astral `Char` literals are rejected by the frontend lexer; that limitation
  belongs to the frontend, not this topic.

Implementation deviation: the worked example names the helpers
`__psrs_floor_int_div`/`__psrs_floor_int_mod`, while the code emits
`__psrs_euclidean_int_div`/`__psrs_euclidean_int_mod`. The names are cosmetic
and the tests assert the emitted names; the design's symbol-allocation and
operand contract are satisfied.
