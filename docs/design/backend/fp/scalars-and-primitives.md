# Scalars and Numeric Operations

**Feature:** F-02  
**Status:** Stable (design)  
**Prerequisites:** [CC IR](cc-ir.md) and [MIR](mir.md); two's-complement
integer arithmetic, IEEE-754 binary64, Euclidean division, and the Wasm numeric
instruction set. Read [IR boundaries](../00-ir-boundaries.md) first.  
**Summary:** Scalars are the unboxed Wasm value types the backend uses for
`Int`, `Number`, `Boolean`, `Char`, and `Unit`; `String` is a GC array of
canonical UTF-8 bytes
([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)),
copied only at the canonical ABI boundary, and not a scalar. This document
fixes the complete unary and binary operation
vocabulary, the concrete Wasm lowering of every operation, the library-owned Euclidean division policy, and the saturating `Number`-to-`Int`
conversion. It is the reference a frontend uses to lower every built-in scalar
operator without adding representation special cases.

## Scope

This document owns the scalar value model shared by CC and MIR, the unary and
binary operation vocabularies and their lowering, the boundary to library arithmetic, the saturating float-to-int sequence, and scalar verification. It does
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

**Truncated versus Euclidean division.** The primitive quotient truncates toward
zero and its remainder takes the dividend's sign. Official `Data.EuclideanRing`
`div/mod` instead satisfy `a = b * div a b + mod a b` with a nonnegative
remainder below `abs b` for nonzero divisors; both return zero for divisor zero.
The ordinary target library implements that policy using the truncating
primitives. For example, `div 3 (-2) = -1` and `mod 3 (-2) = 1`.
The compiler does not provide a second floor-division policy.

**IEEE-754 binary64.** `Number` is an IEEE-754 binary64 value. The ordered
comparisons follow the usual rules: a `NaN` is unequal to everything including
itself, and `+0` compares equal to `-0`.

**Unicode scalar values.** A `Char` is a Unicode scalar value in
`0..=0x10FFFF` excluding surrogates, which fits the `i32` representation.
Validity is a type-checking invariant, not a runtime check. A supplementary
scalar is one valid `Char`
([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).

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
| `String` | `String` | `(ref $string)` | `(ref $string)` | GC array of canonical UTF-8; not a scalar. Copied into a transient buffer only at the canonical ABI boundary ([DEC-10](../../../decision/DEC-10-canonical-abi-buffer-lifetime.md), [DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)). |

CC has no distinct `Char` or `Unit` shape: the layout classifier maps
`Type::I32`, `Type::Char`, and `Type::Unit` to `ValueShape::Integer`;
`Type::String` maps to the distinct `ValueShape::String`. `Boolean` and `F64`
also have their own shapes. P9 maps `String` to the target's GC string
representation while CC can reject numeric operations on it. `I64` and `F32`
remain MIR
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
            | IntQuot | IntRem
            | IntAnd | IntOr | IntXor | IntShl | IntShr | IntZshr
            | IntEq | IntNe | IntLt | IntLe | IntGt | IntGe
            | NumberAdd | NumberSub | NumberMul | NumberDiv
            | NumberEq | NumberNe | NumberLt | NumberLe | NumberGt | NumberGe
            | BooleanAnd | BooleanOr | BooleanEq | BooleanNe
            | StringEq
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
- `StringEq` takes two `ValueShape::String` values and produces `Boolean`.
  Canonical UTF-8 byte length and contents define equality; object identity is
  not observable.
- Euclidean division and modulo remain ordinary library functions, composed
  from checked primitive arithmetic and conditionals.

## Design

### Chosen lowering

P9 selects one concrete MIR operation per CC operation and the Wasm emitter
maps each MIR operation to a leaf opcode or a fixed instruction sequence. The
chosen mapping is:

| CC operation | MIR operation | Wasm |
| --- | --- | --- |
| `IntAdd` / `IntSub` / `IntMul` | `I32Add` / `I32Sub` / `I32Mul` | `i32.add` / `i32.sub` / `i32.mul` |
| `IntQuot` / `IntRem` | `I32DivS` / `I32RemS` | `i32.div_s` / `i32.rem_s` |
| `IntAnd` / `IntOr` / `IntXor` | `I32And` / `I32Or` / `I32Xor` | `i32.and` / `i32.or` / `i32.xor` |
| `IntShl` / `IntShr` / `IntZshr` | `I32Shl` / `I32ShrS` / `I32ShrU` | `i32.shl` / `i32.shr_s` / `i32.shr_u` |
| `IntEq`..`IntGe` | `I32Eq`..`I32GeS` | `i32.eq`/`ne`/`lt_s`/`le_s`/`gt_s`/`ge_s` |
| `NumberAdd`..`NumberDiv` | `F64Add`..`F64Div` | `f64.add`/`sub`/`mul`/`div` |
| `NumberEq`..`NumberGe` | `F64Eq`..`F64Ge` | `f64.eq`/`ne`/`lt`/`le`/`gt`/`ge` |
| `BooleanAnd` / `BooleanOr` | `BoolAnd` / `BoolOr` | `i32.and` / `i32.or` |
| `BooleanEq` / `BooleanNe` | `BoolEq` / `BoolNe` | `i32.eq` / `i32.ne` |
| `StringEq` | MIR length check and UTF-8 byte loop | compare canonical byte lengths and contents |
| `CharEq`..`CharGe` | `I32Eq`..`I32GeS` | integer comparisons |
| `IntNeg` | `I32Neg` | `0 - x` |
| `IntComplement` | `I32Complement` | `x ^ -1` |
| `NumberNeg` | `F64Neg` | `f64.neg` |
| `NumberAbs` | `F64Abs` | `f64.abs` |
| `NumberSqrt` | `F64Sqrt` | `f64.sqrt` |
| `NumberTrunc` | `F64Trunc` | `f64.trunc` |
| `NumberFloor` / `NumberCeil` | `F64Floor` / `F64Ceil` | `f64.floor` / `f64.ceil` |
| `BooleanNot` | `BoolNot` | `i32.eqz` |
| `IntToNumber` | `I32ToF64` | `f64.convert_i32_s` |
| `NumberToInt` | `F64ToI32Sat` | saturating sequence (below) |
| `BooleanToInt` | `BoolToI32` | identity |
| `IntToBoolean` | `I32ToBool` | `i32.ne` with `0` |
| `CharToInt` / `IntToChar` | `I32Identity` | identity |

The suffix `S` marks the signed integer operations; equality and bitwise
operations are sign-agnostic. Shifts take their count modulo 32, as Wasm and
JavaScript specify.

`StringEq` is the only String operation in this vocabulary. Its MIR lowering
checks byte lengths first, then compares unsigned bytes from index zero until a
mismatch or the shared length is reached. Canonical UTF-8 gives each Unicode
scalar sequence one byte sequence, so byte equality is scalar String equality.

### Integer arithmetic

- `IntAdd`, `IntSub`, `IntMul` wrap modulo 2^32.
- `IntQuot`/`IntRem` are truncated toward zero. `IntRem` has the sign of the
  dividend. Division or remainder by zero traps because the Wasm instruction
  traps; `i32.div_s` also traps on `i32.min / -1`, and that is the defined
  two's-complement overflow behavior.
- Public `div/mod` belong to the library and use nonnegative Euclidean
  remainders and explicit zero handling; see the background above.
- Comparisons are signed; shifts and bitwise operations act on the 32-bit
  pattern.

### Number arithmetic

- Arithmetic maps to the four `f64` operations and negation to `f64.neg`.
- `NumberAbs` (`numberAbs :: Number -> Number`) lowers to `f64.abs`, clearing
  the sign bit without changing the magnitude or NaN payload. Negative zero
  becomes positive zero, either infinity becomes positive infinity, and NaN
  remains NaN. Core, CC and MIR require Number/F64 operands and results. This
  implements the official Data.Number.abs foreign slot; no integer conversion
  or library-name recognition is involved. The primitive remains explicit for
  constant operands; no new constant folding is claimed. See the
  [WebAssembly absolute-value semantics](https://webassembly.github.io/spec/core/exec/numerics.html#op-fabs).
- `NumberSqrt` (`numberSqrt :: Number -> Number`) lowers to `f64.sqrt`. It
  follows IEEE-754 square root: exact squares stay exact, positive infinity
  stays positive infinity, a negative finite value or negative infinity becomes
  NaN, NaN stays NaN, and negative zero stays negative zero. The operation does
  not trap. It implements the official Data.Number.sqrt foreign slot and is not
  folded when its operand is constant. See the
  [WebAssembly square-root semantics](https://webassembly.github.io/spec/core/exec/numerics.html#op-fsqrt).
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

### Number integral rounding

`NumberTrunc` (`numberTrunc :: Number -> Number`) lowers to `f64.trunc`:
finite values round toward zero without converting to i32. It preserves zero
signs and infinities; NaN produces NaN without a payload-bit guarantee. Core,
CC and MIR verify Number/F64 operand and result types. The primitive remains
available for constant operands even when the MIR optimizer does not fold it.
The library uses it for the official Data.Number.trunc foreign slot; public
Int.trunc retains its official finite check and range clamping wrapper.

`NumberFloor` and `NumberCeil` have the same Number/F64 type contract and
nonfinite/zero-sign guarantees, with rounding toward negative and positive
infinity, respectively. They lower directly to `f64.floor` and `f64.ceil`,
without an integer representation boundary. The library owns ECMAScript
round-to-nearest with ties toward positive infinity: this differs from Wasm
`f64.nearest` and cannot be replaced by adding 0.5 before flooring. Public
Int.floor/ceil/round retain their unchanged finite checks and clamping wrappers.

### Complete decimal conversion

`NumberFromDecimal` (`numberFromDecimal :: String -> Number`) converts a complete
ASCII signed decimal token, with an optional decimal exponent, to binary64.
It uses the pinned Rust core parser's nearest-representable rounding, including
ties to even, signed underflow zero, and overflow to signed infinity. Empty,
partial, nondecimal, non-ASCII, and whitespace-containing tokens produce NaN.
`Infinity` and `NaN` spellings are outside this primitive's grammar.

Core and CC validate String input and Number output before ABI erasure. MIR
copies canonical UTF-8 to a transient linear-memory buffer using the shared
string boundary protocol, calls the checked numeric-runtime export, and frees
the buffer. The runtime allocates nothing and retains no pointer. Its artifact
contract declares both numeric exports and private table initialization;
static stack analysis covers every path reachable from its entry points.
The compiler preserves potentially trapping canonical-buffer allocation for
both numeric formatting and conversion, even when the result is unused.

The library owns ECMAScript whitespace, longest-prefix recognition, rollback of
an incomplete exponent, `Infinity` recognition, and ordinary predicate/builder
calls. It preserves the official public `Data.Number.fromString` wrapper and
the foreign slot's rank-N `Fn4` signature. Whole parsing functions are not
compiler intrinsics. The contracts are
[ECMAScript parseFloat](https://tc39.es/ecma262/multipage/global-object.html#sec-parsefloat-string)
and [Rust f64::from_str](https://doc.rust-lang.org/std/primitive.f64.html#impl-FromStr-for-f64).

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

- **Compiler-owned Euclidean or floor helpers.** Rejected: public arithmetic
  policy belongs to ordinary library definitions. The old `intDiv/intMod`
  bindings implemented a different signed-divisor policy and are retired.
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

### Library division and modulo

The official pure wrappers retain their source signatures and branches. The
Wasm-specific foreign delegates expose truncating quotient/remainder and wrapping
arithmetic. Their callers implement Euclidean adjustment and zero handling.
There is no `IntDiv` or `IntMod` CC operation or generated MIR helper. Retired
intrinsic IDs 26 and 27 remain reserved; the names are rejected rather than
being silently rebound to a different policy.

## Code map

The language vocabulary, exhaustive target selection, concrete MIR operations,
and Wasm emission have separate owners. See
[intrinsic implementations](intrinsic-implementations.md). The structure is:

```text
mir/numeric.rs                  UnaryOp and NumericOp; CC -> MIR operation selection
target_intrinsics/             exhaustive language-to-target selection
mir/lower/runtime_call.rs      artifact protocol adaptation
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
impl TryFrom<cc::BinaryOp> for NumericOp;   // StringEq is lowered as a byte comparison
```

Every CC unary operation MUST map to exactly one `UnaryOp`, and every CC binary
operation except `StringEq` MUST map to exactly one `NumericOp`. String equality
uses its checked GC byte-array lowering.

**Implementation selection.** `target_intrinsics::implementation` MUST classify
all active HIR identities exhaustively as direct operations, generated operations,
artifact exports, elaborated values, or explicit unsupported values. This table
selects the implementation; the consuming pass performs its own typed conversion.
Artifact adaptation validates the language scheme before erasure, and linking
checks the actual MIR consumer signature against the provider.

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
  `Boolean -> I32`, and `I32ToBool` is `I32 -> Boolean`.

A mismatch is reported with the operation's source span. Every operation in
this vocabulary is part of the core WebAssembly baseline; the target capability
gate needs no new proposal for it.

## Worked example

For `a = -5` and `b = 3`, raw quotient/remainder produce `-1` and `-2`.
The library adjusts them to `div a b = -2` and `mod a b = 1`, satisfying
`-5 = 3 * (-2) + 1`. For `a = 3` and `b = -2`, the raw remainder is already
nonnegative: the library returns quotient `-1` and remainder `1`.
For `b = 0`, the wrapper returns zero before executing either trapping primitive.
Source tests exercise these wrappers through case branches and all operand signs.

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
  semantics (JavaScript `%` versus Euclidean policy) must be owned by the
  library wrapper rather than a new compiler arithmetic operation.
- **Saturating-capability switch.** The profile enables the saturating
  float-to-int proposal; if the sequence were replaced by `i32.trunc_sat_f64_s`
  the capability gate would have to require it.
- **`i64`/`f32` source types.** Adding them is a vocabulary extension with no
  change to the operand/result model.
- **Fast paths for known-sign constants.** Ordinary library code may inline its
  adjustment when operands are known, preserving the Euclidean result.

## Implementation notes

The CC and MIR vocabularies, lowerings, and verifiers implement the full unary
and binary set above. The source bootstrap exposes the operations that do not
already have symbolic integer syntax as specialized functions: `intNeg`,
`intComplement`, `numberNeg`, `numberTrunc`, `numberFloor`, `numberCeil`, `booleanNot`, the six conversion names from the
table (`intToNumber`, `numberToInt`, `booleanToInt`, `intToBoolean`,
`charToInt`, and `intToChar`), the six integer bitwise
and shift names, all `number*`, `boolean*`, and `char*` binary names in the
table.
The existing symbols `+`, `-`, `*`, `/`, `%`, `==`, `/=`, `<`, `<=`, `>`, and
`>=` continue to expose the integer arithmetic and comparison operations.
Fully saturated intrinsic applications lower to typed Core unary or binary
primitives; Core verification checks their exact scalar operand and result
types before P8 maps them into CC. A source-level driver fixture compiles every
operation and executes the documented Euclidean, conversion, comparison, and
wrapping behaviors through Wasmtime when it is available.

## References

- IEEE 754-2019, binary64 arithmetic.
- WebAssembly 3.0: numeric instructions, `i32`/`f64` semantics, and traps.
- [DEC-05](../../../decision/DEC-05-wasmtime-feature-set.md),
  [DEC-09](../../../decision/DEC-09-gc-only-language-heap.md).
