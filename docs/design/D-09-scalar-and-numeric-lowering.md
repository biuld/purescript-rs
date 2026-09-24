# D-09 — Scalar and Numeric Lowering

**Implements:** [F-02 — Build Portable Program Artifacts](../feature/F-02-portable-programs.md)  
**Status:** In progress (backend operations implemented; Core intrinsic integration remains)

## Purpose

Define the complete scalar value and numeric operation slice for the backend so
a frontend can lower every built-in scalar operator without adding
representation special cases. This document complements
[D-06](D-06-low-level-ir-and-wasm-types.md), which defines the IR boundaries,
and [D-10](D-10-linear-memory-representation.md), which defines the canonical
ABI boundary where strings and byte lists are exchanged.

Scalar semantics follow the official PureScript implementation. Where a
JavaScript operator has no direct Wasm instruction, the backend defines an
explicit lowering rather than silently choosing a different behavior.

## Value types

| Source type | CC `ValueShape` | MIR `ValueType` | Wasm | Semantics |
| --- | --- | --- | --- | --- |
| `Int` | `Integer` | `I32` | `i32` | Signed 32-bit, wrapping modulo 2^32 (two's complement). |
| `Number` | `Number` | `F64` | `f64` | IEEE-754 binary64. |
| `Boolean` | `Boolean` | `Boolean` | `i32` | `0` is false, `1` is true; no other value is produced. |
| `Char` | `Integer` | `I32` | `i32` | Unicode scalar value (`0..=0x10FFFF` excluding surrogates). |
| `Unit` | `Integer` | `I32` | `i32` | Always `0`. |
| `String` | `Integer` | `I32` | `i32` | Linear-memory pointer to a length-prefixed UTF-8 buffer (D-10). |

`i64` and `f32` remain MIR value types reserved for the canonical ABI (D-07);
they are not source-level value shapes yet. A future source type maps to them
without changing the operation model.

## Operation model

CC carries target-independent, type-directed scalar operations. CC owns
`BinaryOp`; P8 converts the Core integer subset into it, and P9 converts it to
MIR-owned `NumericOp`. CC also owns `UnaryOp`, which P9 converts to a MIR-owned
unary operation. CC/MIR currently implement the listed unary vocabulary and
binary operations; P9 expands Euclidean `IntDiv`/`IntMod` to ordinary MIR
helper calls.

The planned full vocabulary is:

```text
UnaryOp  = IntNeg | IntComplement | NumberNeg | BooleanNot
         | IntToNumber | NumberToInt | BooleanToInt | IntToBoolean
         | CharToInt | IntToChar

BinaryOp = IntAdd | IntSub | IntMul
         | IntQuot | IntRem | IntDiv | IntMod
         | IntAnd | IntOr | IntXor | IntShl | IntShr | IntZshr
         | IntEq | IntNe | IntLt | IntLe | IntGt | IntGe
         | NumberAdd | NumberSub | NumberMul | NumberDiv
         | NumberEq | NumberNe | NumberLt | NumberLe | NumberGt | NumberGe
         | BooleanAnd | BooleanOr | BooleanEq | BooleanNe
         | CharEq | CharNe | CharLt | CharLe | CharGt | CharGe
```

P9 selects the concrete MIR operation for each CC operation. MIR keeps a
concrete operation vocabulary (or a typed Wasm opcode reference); it never
stores a CC operation enum.

### Integer operations

- `IntAdd`, `IntSub`, `IntMul` wrap modulo 2^32, matching JavaScript `|0`.
- `IntQuot`/`IntRem` are truncated toward zero and map directly to
  `i32.div_s`/`i32.rem_s`. `IntRem` has the sign of the dividend.
- `IntDiv`/`IntMod` are **Euclidean**, matching `Data.Int`: `IntMod` has the
  sign of the divisor and satisfies `a = b * IntDiv a b + IntMod a b` for
  `b /= 0`. They do not map to a single Wasm instruction and lower through a
  runtime helper (below).
- `IntAnd`, `IntOr`, `IntXor`, `IntShl`, `IntShr`, `IntZshr` map to the
  corresponding `i32` bit operations. Shift counts are taken modulo 32, as in
  Wasm and JavaScript.
- Comparisons are signed (`i32.lt_s`, ...). Equality is `i32.eq`/`i32.ne`.
- `IntNeg` is `0 - x`; `IntComplement` is `x ^ -1`.
- Division or remainder by zero traps, because the underlying Wasm instruction
  traps; the language-level behavior is defined by the standard library, not by
  this document.

### Number operations

- Arithmetic maps to `f64.add`/`sub`/`mul`/`div`. `NumberNeg` is `f64.neg`.
- Comparisons map to the ordered `f64` comparisons; `NumberEq`/`NumberNe` use
  `f64.eq`/`f64.ne`, so `NaN` is unequal to itself and `+0 = -0`.
- `Number` has no Euclidean division instruction. If the standard library
  exposes `Number` `div`/`mod`, `mod` lowers through an `f64` helper that
  computes the JavaScript `%` semantics (`x - trunc(x / y) * y`) or the
  Euclidean semantics required by the instance, decided when that instance is
  added.

### Boolean operations

- `BooleanAnd`/`BooleanOr` map to `i32.and`/`i32.or`; `BooleanNot` maps to
  `i32.eqz`. `BooleanEq`/`BooleanNe` are `i32.eq`/`i32.ne`.
- Operations that produce a comparison result produce exactly `0` or `1`.

### Character operations

- `Char` is a Unicode scalar value in an `i32`. Comparisons and equality reuse
  the integer operations. `CharToInt` and `IntToChar` are representation
  no-ops; the source layer is responsible for the validity of the scalar value.

### Conversions

- `IntToNumber` is `f64.convert_i32_s`.
- `NumberToInt` is a saturating conversion so the result is total. P9 lowers
  it to comparisons and a guarded `i32.trunc_f64_s` sequence that uses only core
  WebAssembly; the standard library's `floor`/`ceil`/`trunc` build on it.
- `BooleanToInt` is a no-op on the `0`/`1` representation; `IntToBoolean` is
  `x /= 0` (`i32.ne`).

## Runtime helpers

Operations with no single Wasm instruction lower to ordinary MIR functions:

- Euclidean `IntDiv`/`IntMod`: each operation uses a module-local helper. It
  computes the truncated remainder and adjusts the quotient or remainder when
  their signs differ, so the remainder matches the divisor.
- `Number` `mod`, if enabled: an `f64` helper.

Helpers are module-local MIR functions with ordinary MIR signatures. They are
not runtime imports, add no WIT surface, and are pruned when unused. The
lowering may instead inline the adjustment when the operands are already known
to be non-negative, as an optimization that preserves semantics.

## Verification

- The CC verifier checks each operation's operand and result `ValueShape`
  exactly: integer operations take and produce `Integer`, number operations
  `Number`, boolean operations `Boolean`, comparisons produce `Boolean`, and
  conversions match their declared source and destination shapes.
- The MIR verifier checks the concrete operand and result `ValueType` of every
  lowered operation and the helper signatures.
- Every operation in this slice is core MVP; the target capability gate needs
  no new proposal for it.

## Delivery order

1. Define the CC operation vocabulary and lower the current Core primitives
   through it. **Implemented for Core's current integer subset:** CC `BinaryOp`
   and MIR `NumericOp` are distinct enums with explicit P8/P9 conversions.
2. Add integer bitwise and shift operations, plus Boolean and Char comparison
   lowering, unary operations, and scalar conversions. **Implemented in
   CC/MIR;** Core intrinsic integration remains at step 5.
3. Add the `Number` arithmetic and comparison set. **Implemented in CC/MIR;**
   frontend intrinsic integration remains at step 5.
4. Add the Euclidean `IntDiv`/`IntMod` helpers and their execution tests.
   **Implemented:** helpers are generated only when referenced and execute on
   GC.
5. Extend the Core intrinsic set and the frontend operator mapping in lockstep,
   so each operation has an end-to-end executable test.

Steps 1–4 are backend work; step 5 is the frontend integration boundary and is
tracked by the frontend feature matrix.
