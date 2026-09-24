# Scalars and Numeric Operations

**Feature:** F-02  
**Status:** Stable (design)  
**Prerequisites:** [CC IR](cc-ir.md) and [MIR](mir.md); two's-complement
integer arithmetic, IEEE-754 binary64, floor division, and the Wasm numeric
instruction set. Read [IR boundaries](../00-ir-boundaries.md) first.  
**Summary:** Scalars are the unboxed Wasm value types the backend uses for
`Int`, `Number`, `Boolean`, `Char`, and `Unit`; `String` is an ABI pointer and
not a scalar. This document fixes the complete unary and binary operation
vocabulary, the concrete Wasm lowering of every operation, the module-local
floor division and modulo helpers, and the saturating `Number`-to-`Int`
conversion. It is the reference a frontend uses to lower every built-in scalar
operator without adding representation special cases.

## Scope

This document owns the scalar value model shared by CC and MIR, the unary and
binary operation vocabularies and their lowering, the generated floor
helpers, the saturating float-to-int sequence, and scalar verification. It does
not own the byte-oriented string and ABI boundary (see
[linear memory and the canonical ABI
boundary](../wasm/linear-memory-and-canonical-abi-boundary.md) and
[canonical ABI](../wasm/canonical-abi-and-wit.md)), the erased protocol that
boxes scalars (see [polymorphism and erasure](polymorphism-and-erasure.md)),
concrete GC layouts (see [data representation](data-representation.md)), or the
frontend's intrinsic mapping.

## Background

**Two's-complement integers.** A PureScript `Int` is a signed 32-bit quantity.
Addition, subtraction, and multiplication wrap modulo 2^32, which is exactly
the Wasm `i32.add`/`sub`/`mul` behavior. The official compiler realizes `Int` on
JavaScript with `| 0`, giving the same wrapping semantics; that behavior is the
reference.

**Truncated versus floor division.** Truncated division rounds toward zero
and its remainder takes the sign of the dividend. `Data.Int`'s `div` rounds
toward negative infinity, and `mod` satisfies `a = b * div a b + mod a b`:
for nonzero `b`, the remainder is zero or has the sign of `b` and magnitude
less than `|b|`. Wasm `i32.div_s`/`i32.rem_s` provide the truncated pair, so
the floor pair is computed by a helper.

**IEEE-754 binary64.** `Number` is an IEEE-754 binary64 value. The ordered
comparisons follow the usual rules: a `NaN` is unequal to everything including
itself, and `+0` compares equal to `-0`.

**Unicode scalar values.** A `Char` is a Unicode scalar value in
`0..=0x10FFFF` excluding surrogates, which fits the `i32` representation.
Validity is a type-checking invariant, not a runtime check.

**Saturating conversions.** WebAssembly's trapping `i32.trunc_f64_s` has no
total behavior for out-of-range inputs or `NaN`. The backend defines a
saturating lowering so a conversion of any `Number` produces an `Int`, matching
the language-level expectation that the conversion is total.

## Model

### Value types

| Source type | CC `ValueShape` | MIR `ValueType` | Wasm | Semantics |
| --- | --- | --- | --- | --- |
| `Int` | `Integer` | `I32` | `i32` | Signed 32-bit, wrapping modulo 2^32. |
| `Number` | `Number` | `F64` | `f64` | IEEE-754 binary64. |
| `Boolean` | `Boolean` | `Boolean` | `i32` | `0` is false, `1` is true; no other value is produced. |
| `Char` | `Integer` | `I32` | `i32` | Unicode scalar value. |
| `Unit` | `Integer` | `I32` | `i32` | No payload; the canonical value is `0`. |
| `String` | `String` | `I32` | `i32` | Pointer to a length-prefixed UTF-8 buffer; an ABI pointer, not a scalar. |

CC has no distinct `Char` or `Unit` shape: the layout classifier maps
`Type::I32`, `Type::Char`, and `Type::Unit` to `ValueShape::Integer`;
`Type::String` maps to the distinct `ValueShape::String`. `Boolean` and `F64`
also have their own shapes. P9 maps `String` to an ABI address while CC can
reject numeric operations on it. `I64` and `F32` remain MIR
value types reserved for the canonical ABI; a future source type can map to
them without changing the operation model.

### Operation vocabularies

CC owns the target-independent vocabulary; MIR owns the concrete one, and P9
converts between them.

```text
CC UnaryOp  = IntNeg | IntComplement | NumberNeg | BooleanNot
            | IntToNumber | NumberToInt | BooleanToInt | IntToBoolean
            | CharToInt | IntToChar

CC BinaryOp = IntAdd | IntSub | IntMul
            | IntQuot | IntRem | IntDiv | IntMod
            | IntAnd | IntOr | IntXor | IntShl | IntShr | IntZshr
            | IntEq | IntNe | IntLt | IntLe | IntGt | IntGe
            | NumberAdd | NumberSub | NumberMul | NumberDiv
            | NumberEq | NumberNe | NumberLt | NumberLe | NumberGt | NumberGe
            | BooleanAnd | BooleanOr | BooleanEq | BooleanNe
            | CharEq | CharNe | CharLt | CharLe | CharGt | CharGe

MIR UnaryOp  = I32Neg | I32Complement | F64Neg | BoolNot
             | I32ToF64 | F64ToF32 | F32ToF64 | F64ToI32Sat
             | BoolToI32 | I32ToBool | I32Identity

MIR NumericOp = I32Add | I32Sub | I32Mul | I32DivS | I32RemS
              | I32And | I32Or | I32Xor | I32Shl | I32ShrS | I32ShrU
              | I32Eq | I32Ne | I32LtS | I32LeS | I32GtS | I32GeS
              | BoolAnd | BoolOr | BoolEq | BoolNe
              | F64Add | F64Sub | F64Mul | F64Div
              | F64Eq | F64Ne | F64Lt | F64Le | F64Gt | F64Ge
```

MIR never stores a CC operation enum. `F64ToF32` and `F32ToF64` are reachable
only from ABI adaptation, not from a CC `UnaryOp`; all other MIR variants are
reachable from the CC vocabulary.

### Invariants

- Operand and result shapes are exact, not merely compatible: integer
  operations take and produce `ValueShape::Integer`, number operations
  `Number`, boolean operations `Boolean`, and comparisons produce `Boolean`.
- `Boolean` values are exactly `0` or `1`.
- Every MIR scalar operation has a defined operand and result `ValueType`; the
  MIR verifier checks them exactly.
- A `String` is never passed to a numeric operation.
- floor helpers are ordinary MIR functions with `i32` parameters and
  result, generated only when the module contains `IntDiv` or `IntMod`.

## Design

### Chosen lowering

P9 selects one concrete MIR operation per CC operation and the Wasm emitter
maps each MIR operation to a leaf opcode or a fixed instruction sequence. The
chosen mapping is:

| CC operation | MIR operation | Wasm |
| --- | --- | --- |
| `IntAdd` / `IntSub` / `IntMul` | `I32Add` / `I32Sub` / `I32Mul` | `i32.add` / `i32.sub` / `i32.mul` |
| `IntQuot` / `IntRem` | `I32DivS` / `I32RemS` | `i32.div_s` / `i32.rem_s` |
| `IntDiv` / `IntMod` | helper `Call` | module-local MIR function |
| `IntAnd` / `IntOr` / `IntXor` | `I32And` / `I32Or` / `I32Xor` | `i32.and` / `i32.or` / `i32.xor` |
| `IntShl` / `IntShr` / `IntZshr` | `I32Shl` / `I32ShrS` / `I32ShrU` | `i32.shl` / `i32.shr_s` / `i32.shr_u` |
| `IntEq`..`IntGe` | `I32Eq`..`I32GeS` | `i32.eq`/`ne`/`lt_s`/`le_s`/`gt_s`/`ge_s` |
| `NumberAdd`..`NumberDiv` | `F64Add`..`F64Div` | `f64.add`/`sub`/`mul`/`div` |
| `NumberEq`..`NumberGe` | `F64Eq`..`F64Ge` | `f64.eq`/`ne`/`lt`/`le`/`gt`/`ge` |
| `BooleanAnd` / `BooleanOr` | `BoolAnd` / `BoolOr` | `i32.and` / `i32.or` |
| `BooleanEq` / `BooleanNe` | `BoolEq` / `BoolNe` | `i32.eq` / `i32.ne` |
| `CharEq`..`CharGe` | `I32Eq`..`I32GeS` | integer comparisons |
| `IntNeg` | `I32Neg` | `0 - x` |
| `IntComplement` | `I32Complement` | `x ^ -1` |
| `NumberNeg` | `F64Neg` | `f64.neg` |
| `BooleanNot` | `BoolNot` | `i32.eqz` |
| `IntToNumber` | `I32ToF64` | `f64.convert_i32_s` |
| `NumberToInt` | `F64ToI32Sat` | saturating sequence (below) |
| `BooleanToInt` | `BoolToI32` | identity |
| `IntToBoolean` | `I32ToBool` | `i32.ne` with `0` |
| `CharToInt` / `IntToChar` | `I32Identity` | identity |

The suffix `S` marks the signed integer operations; equality and bitwise
operations are sign-agnostic. Shifts take their count modulo 32, as Wasm and
JavaScript specify.

### Integer arithmetic

- `IntAdd`, `IntSub`, `IntMul` wrap modulo 2^32.
- `IntQuot`/`IntRem` are truncated toward zero. `IntRem` has the sign of the
  dividend. Division or remainder by zero traps because the Wasm instruction
  traps; `i32.div_s` also traps on `i32.min / -1`, and that is the defined
  two's-complement overflow behavior.
- `IntDiv`/`IntMod` use floor division and a remainder with the divisor's sign,
  matching `Data.Int`; they are computed by
  the helpers below.
- Comparisons are signed; shifts and bitwise operations act on the 32-bit
  pattern.

### Number arithmetic

- Arithmetic maps to the four `f64` operations and negation to `f64.neg`.
- Comparisons use the ordered `f64` operations; `NumberEq`/`NumberNe` are
  `f64.eq`/`f64.ne`, so `NaN` is unequal to itself and `+0 = -0`.
- The current vocabulary has no `Number` remainder. If the standard library
  exposes one, it needs a helper that realizes the required semantics (for
  example the JavaScript `%` semantics `x - trunc(x / y) * y`), decided when
  that operation is added.

### Boolean and character operations

- Boolean `and`/`or` use `i32.and`/`i32.or`; `BooleanNot` is `i32.eqz`.
  Because operands are `0`/`1`, the results are `0`/`1`.
- Every comparison produces exactly `0` or `1`.
- `Char` reuses the signed integer comparisons; `CharToInt` and `IntToChar`
  are representation identities, and the source layer is responsible for the
  validity of the scalar value.

### Conversions

- `IntToNumber` is `f64.convert_i32_s`.
- `NumberToInt` is *saturating*: `NaN` maps to `0`, values at or below
  `-2^31` to `i32.min`, values at or above `2^31` to `i32.max`, and all other
  values truncate toward zero. The sequence (below) uses only core WebAssembly
  instructions, so it does not depend on the `nontrapping-float-to-int`
  proposal even though that capability is enabled in the profile.
- `BooleanToInt` is an identity on the `0`/`1` representation and
  `IntToBoolean` is `x != 0`.

### Rejected alternatives

- **Map `IntDiv`/`IntMod` to `i32.div_s`/`i32.rem_s`.** Rejected: these are
  truncated, not floor, and produce the wrong remainder sign for negative
  operands (`-5` by `3` would give remainder `-2` instead of `1`).
- **Trapping `Number`-to-`Int`.** Rejected: an out-of-range or `NaN` operand
  would trap a type-correct program. The saturating lowering keeps the
  conversion total.
- **A single "generic integer" operation with a runtime width or signedness
  tag.** Rejected: the Wasm type system already distinguishes `i32` and `i64`,
  so a tag would be dead information and would weaken verification.
- **Lowering every operation only at the Wasm emitter.** Rejected: the MIR
  operation vocabulary is what the MIR verifier checks, and it keeps the
  concrete signedness and operand types explicit before encoding.

## Algorithms

### Saturating `Number` to `Int`

```text
lower_saturating_f64_to_i32(x):
    if x <> x:                 # NaN
        return 0
    if x <= -2147483648.0:
        return i32.min
    if x >=  2147483648.0:
        return i32.max
    return i32.trunc_f64_s(x)
```

The emitter builds this as nested `if` expressions over `f64.le`, `f64.ge`,
and `f64.ne` and calls the trapping `i32.trunc_f64_s` only inside the safe
range, so it never traps.

### Floor division and modulo

Both helpers read their operands as `i32` parameters `a` and `b`. They first
compute the truncated remainder and decide whether an adjustment is needed:

```text
r             = a rem_s b          # truncated remainder
nonzero       = r != 0
remainder_neg = r < 0
divisor_neg   = b < 0
adjust        = nonzero && (remainder_neg != divisor_neg)

# divide:
if adjust: a div_s b - 1 else: a div_s b

# modulo:
if adjust: r + b        else: r
```

The adjustment changes a quotient or remainder only when the truncated remainder
and the divisor have opposite signs. Thus `a = b * div a b + mod a b`, with
`mod a b` taking the sign of `b` (or zero). This is floor division; the
nonnegative-remainder convention is a different rule. Both paths
execute `a rem_s b` (and `a div_s b` for the divide helper), so `b = 0` traps as
the underlying instruction does. This matches the official `Data.Int`
semantics.

### Helper generation and linking

```text
lower_scalar_helpers(module, first_function_id):
    needs_div = any assignment (including nested if branches) is IntDiv
    needs_mod = any assignment is IntMod
    symbol_module = first function's module, else the intrinsics module
    allocate each needed helper symbol downward from u32::MAX, skipping symbols
      used by the module's functions and externals
    emit each helper as an ordinary MIR Function with consecutive FunctionIds
      starting at first_function_id
```

Each helper is a four-block MIR function (entry, then-block, else-block, merge
block) with a `Branch`, two `Jump`s with the computed value, and a `Return`. The
helper symbols are held beside the module in `ScalarHelpers`; when lowering an
`IntDiv`/`IntMod` assignment, P9 emits an `Instruction::Call` to the
corresponding helper instead of a `Primitive`. A module that uses neither
operation generates neither helper.

## Code map

The scalar design is owned by three module groups: the MIR operation
vocabularies, the MIR helper generator, and the Wasm emission modules. The
intended structure is:

```text
mir/numeric.rs                  UnaryOp and NumericOp; CC -> MIR operation selection
mir/scalar_helpers.rs           floor helper detection, generation, and
                                helper symbol allocation
wasm/lower/structure/ops.rs     binary NumericOp -> wasm_encoder::Instruction,
                                plus reference and memory operand helpers
wasm/lower/structure/unary.rs   UnaryOp emission, including the saturating
                                Number -> Int sequence
```

**Operation vocabularies.** `mir/numeric.rs` MUST define the concrete
vocabularies and the total conversion from the CC vocabularies:

```rust
pub enum UnaryOp { I32Neg, I32Complement, F64Neg, BoolNot, I32ToF64, F64ToF32, F32ToF64, F64ToI32Sat, BoolToI32, I32ToBool, I32Identity }
pub enum NumericOp { I32Add, I32Sub, I32Mul, I32DivS, I32RemS, I32And, I32Or, I32Xor, I32Shl, I32ShrS, I32ShrU, I32Eq, I32Ne, I32LtS, I32LeS, I32GtS, I32GeS, BoolAnd, BoolOr, BoolEq, BoolNe, F64Add, F64Sub, F64Mul, F64Div, F64Eq, F64Ne, F64Lt, F64Le, F64Gt, F64Ge }
impl From<cc::UnaryOp> for UnaryOp;   // total
impl TryFrom<cc::BinaryOp> for NumericOp;   // IntDiv/IntMod have no instruction
```

Every CC unary operation MUST map to exactly one `UnaryOp`, and every CC binary
operation except `IntDiv` and `IntMod` MUST map to exactly one `NumericOp`.

**Helper generation.** `mir/scalar_helpers.rs` MUST detect the
helper-requiring operations, allocate their symbols, and emit the helpers:

```rust
pub fn lower_scalar_helpers(module: &cc::Module, first_function_id: u32) -> (ScalarHelpers, Vec<Function>);
impl ScalarHelpers {
    pub fn binary_instruction(&self, op: cc::BinaryOp, destination: ValueId, left: ValueId, right: ValueId, span: TextRange) -> Result<Instruction, Vec<BackendError>>;
}
```

`lower_scalar_helpers` MUST emit each needed helper as an ordinary `Function`
with consecutive `FunctionId`s starting at `first_function_id`, MUST allocate
each helper symbol downward from `u32::MAX` while skipping symbols already used
by the module's functions and externals, and MUST return the symbol table. A
module that uses neither `IntDiv` nor `IntMod` MUST generate no helper.
`ScalarHelpers::binary_instruction` MUST select a `Call` to the matching
generated helper for `IntDiv`/`IntMod` and MUST otherwise select the
corresponding `Primitive` from `mir/numeric.rs`.

**Wasm emission.** `wasm/lower/structure/ops.rs` MUST expose:

```rust
pub fn primitive(op: NumericOp) -> wasm_encoder::Instruction<'static>;
pub fn ref_test(reference: RefType) -> wasm_encoder::Instruction<'static>;
pub fn ref_cast(reference: RefType) -> wasm_encoder::Instruction<'static>;
pub fn memory(offset: u32) -> wasm_encoder::MemArg;
```

`primitive` MUST map every `NumericOp` to its leaf Wasm opcode, and the module
MUST NOT re-declare the Wasm instruction set. `wasm/lower/structure/unary.rs`
MUST expose:

```rust
impl Structurer<'_> {
    pub fn emit_unary_primitive(&self, destination: ValueId, op: UnaryOp, value: ValueId, span: TextRange, body: &mut Body) -> Result<(), Vec<BackendError>>;
}
fn emit_saturating_f64_to_i32(value_local: u32, span: TextRange, body: &mut Body);
```

`emit_unary_primitive` MUST emit the specified sequence for every `UnaryOp`,
MUST route `F64ToI32Sat` to the saturating lowering, and MUST bind the result to
`destination`. `emit_saturating_f64_to_i32` MUST implement the
[saturating algorithm](#saturating-number-to-int) using only core Wasm
instructions.

## Invariants and verification

The CC verifier checks each operation's operand and result `ValueShape`
exactly; the MIR verifier checks each lowered operation's operand and result
`ValueType` exactly, including:

- integer operations take two `I32` and produce `I32`;
- integer and character comparisons take two `I32` and produce `Boolean`;
- boolean operations take two `Boolean` and produce `Boolean`;
- `f64` arithmetic takes two `F64` and produces `F64`, and `f64` comparisons
  produce `Boolean`;
- unary negation/complement match their operand and result types;
- `I32ToF64` is `I32 -> F64`, `F64ToI32Sat` is `F64 -> I32`, `BoolToI32` is
  `Boolean -> I32`, and `I32ToBool` is `I32 -> Boolean`; and
- the floor helper signatures are `(I32, I32) -> I32`.

A mismatch is reported with the operation's source span. Every operation in
this vocabulary is part of the core WebAssembly baseline; the target capability
gate needs no new proposal for it.

## Worked example

floor division of `-5` by `3`. The CC fragment

```text
v0 = -5
v1 = 3
v2 = IntDiv(v0, v1)
v3 = IntMod(v0, v1)
result = v2 + v3*...          // in source, `div (-5) 3` and `mod (-5) 3`
```

has both operations replaced by helper calls, and the module gains
`__psrs_floor_int_div` and `__psrs_floor_int_mod`. Tracing the divide
helper:

```text
a = -5, b = 3
r             = -5 rem_s 3   = -2
nonzero       = true
remainder_neg = true
divisor_neg   = false
adjust        = true
result        = (-5 div_s 3) - 1 = -1 - 1 = -2
```

The modulo helper computes `r + b = -2 + 3 = 1`. Substituting into
`a = b * div a b + mod a b` gives `-5 = 3 * (-2) + 1`, the floor identity.
The `div_mod` and `binary_matrix` fixtures execute this and the other sign
combinations through Wasm GC and check the combined boolean result.

## Boundaries and interfaces

- **Input:** CC `Unary`/`Primitive` assignments with `ValueShape` declarations.
- **Output:** MIR `UnaryPrimitive`/`Primitive` instructions, or `Call`s to
  generated helpers, with concrete `ValueType`s.
- **To [polymorphism and erasure](polymorphism-and-erasure.md):** these are the
  unboxed shapes that the erased protocol boxes at a polymorphic boundary.
- **To [data representation](data-representation.md):** these are the scalar
  field and element types used in GC structs and arrays.
- **To the ABI boundary:** `String` and the reserved `I64`/`F32` types belong to
  [linear memory](../wasm/linear-memory-and-canonical-abi-boundary.md) and
  [canonical ABI](../wasm/canonical-abi-and-wit.md), not to the scalar
  vocabulary.

## Open questions and future work

- **`Number` remainder.** If a source `mod` for `Number` is added, its exact
  semantics (JavaScript `%` versus floor) and helper must be fixed here.
- **Saturating-capability switch.** The profile enables the saturating
  float-to-int proposal; if the sequence were replaced by `i32.trunc_sat_f64_s`
  the capability gate would have to require it.
- **`i64`/`f32` source types.** Adding them is a vocabulary extension with no
  change to the operand/result model.
- **Fast paths for known-sign constants.** The helper could inline the
  adjustment when both operands are statically non-negative; this is an
  optimization that must preserve the floor result.

## Implementation notes

The CC and MIR vocabularies, lowerings, and verifiers implement the full unary
and binary set above. The source bootstrap exposes the operations that do not
already have symbolic integer syntax as specialized functions: `intNeg`,
`intComplement`, `numberNeg`, `booleanNot`, the six conversion names from the
table (`intToNumber`, `numberToInt`, `booleanToInt`, `intToBoolean`,
`charToInt`, and `intToChar`), `intDiv`, `intMod`, the six integer bitwise
and shift names, all `number*`, `boolean*`, and `char*` binary names in the
table.
The existing symbols `+`, `-`, `*`, `/`, `%`, `==`, `/=`, `<`, `<=`, `>`, and
`>=` continue to expose the integer arithmetic and comparison operations.
Fully saturated intrinsic applications lower to typed Core unary or binary
primitives; Core verification checks their exact scalar operand and result
types before P8 maps them into CC. A source-level driver fixture compiles every
operation and executes the documented floor, conversion, comparison, and
wrapping behaviors through Wasmtime when it is available.

## References

- IEEE 754-2019, binary64 arithmetic.
- WebAssembly 3.0: numeric instructions, `i32`/`f64` semantics, and traps.
- [DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md),
  [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md).
