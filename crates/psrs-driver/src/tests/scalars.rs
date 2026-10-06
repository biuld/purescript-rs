use super::*;

const SCALAR_SOURCE: &str = r#"
module Main where
import Prelude

equalInt left right = left == right
subtractInt left right = left - right

checkIntArithmetic = booleanAnd (equalInt (40 + 2) 42) (booleanAnd (equalInt (7 - 2) 5) (booleanAnd (equalInt (6 * 7) 42) (booleanAnd (equalInt (10 / 3) 3) (equalInt (10 % 3) 1))))
checkIntWrapping = equalInt (2147483647 + 1) (subtractInt (intNeg 2147483647) 1)
checkIntFloor = booleanAnd (equalInt (intDiv (intNeg 5) 3) (intNeg 2)) (equalInt (intMod (intNeg 5) 3) 1)
checkIntBits = booleanAnd (equalInt (intAnd 6 3) 2) (booleanAnd (equalInt (intOr 4 1) 5) (booleanAnd (equalInt (intXor 6 3) 5) (booleanAnd (equalInt (intShl 3 2) 12) (booleanAnd (equalInt (intShr (intNeg 8) 1) (intNeg 4)) (equalInt (intZshr (intNeg 1) 1) 2147483647)))))
checkIntComparisons = booleanAnd (5 == 5) (booleanAnd (5 /= 6) (booleanAnd (4 < 5) (booleanAnd (5 <= 5) (booleanAnd (6 > 5) (5 >= 5)))))
checkInt = booleanAnd checkIntArithmetic (booleanAnd checkIntWrapping (booleanAnd checkIntFloor (booleanAnd checkIntBits checkIntComparisons)))

checkIntUnary = booleanAnd (equalInt (intNeg 5) (subtractInt 0 5)) (equalInt (intComplement 0) (intNeg 1))
checkNumberUnary = booleanAnd (numberEq (numberNeg 1.5) (numberSub 0.0 1.5)) (numberEq (intToNumber 5) 5.0)
checkBooleanUnary = booleanAnd (booleanEq (booleanNot false) true) (booleanAnd (booleanEq (intToBoolean 0) false) (booleanEq (intToBoolean 5) true))
checkCharConversions = booleanAnd (equalInt (charToInt 'A') 65) (charEq (intToChar 65) 'A')
checkBooleanConversion = booleanAnd (equalInt (booleanToInt true) 1) (equalInt (booleanToInt false) 0)
checkIntConversion = equalInt (numberToInt 3.9) 3
checkSaturatingConversion = booleanAnd (equalInt (numberToInt 2147483648.0) 2147483647) (equalInt (numberToInt (numberNeg 2147483648.0)) (subtractInt (intNeg 2147483647) 1))
checkUnary = booleanAnd checkIntUnary (booleanAnd checkNumberUnary (booleanAnd checkBooleanUnary (booleanAnd checkCharConversions (booleanAnd checkBooleanConversion (booleanAnd checkIntConversion checkSaturatingConversion)))) )

checkNumberArithmetic = booleanAnd (numberEq (numberAdd 1.5 2.5) 4.0) (booleanAnd (numberEq (numberSub 5.0 2.0) 3.0) (booleanAnd (numberEq (numberMul 2.0 3.0) 6.0) (numberEq (numberDiv 6.0 3.0) 2.0)))
checkNumberComparisons = booleanAnd (numberEq 4.0 4.0) (booleanAnd (numberNe 4.0 5.0) (booleanAnd (numberLt 1.0 2.0) (booleanAnd (numberLe 2.0 2.0) (booleanAnd (numberGt 3.0 2.0) (numberGe 2.0 2.0)))))
checkNaN = booleanAnd (numberNe (numberDiv 0.0 0.0) (numberDiv 0.0 0.0)) (equalInt (numberToInt (numberDiv 0.0 0.0)) 0)
checkNumber = booleanAnd checkNumberArithmetic (booleanAnd checkNumberComparisons checkNaN)

checkBoolean = booleanAnd (booleanEq (booleanAnd true false) false) (booleanAnd (booleanEq (booleanOr false true) true) (booleanAnd (booleanEq true true) (booleanNe true false)))

checkChar = booleanAnd (charEq 'A' 'A') (booleanAnd (charNe 'A' 'B') (booleanAnd (charLt 'A' 'B') (booleanAnd (charLe 'A' 'A') (booleanAnd (charGt 'B' 'A') (charGe 'A' 'A')))))

main = if booleanAnd checkInt (booleanAnd checkUnary (booleanAnd checkNumber (booleanAnd checkBoolean checkChar))) then 0 else 1
"#;

#[test]
fn hexadecimal_integer_literals_use_the_signed_32_bit_range() {
    crate::check_program(&[(
        "Main.purs",
        "module Main where\nvalue :: Int\nvalue = 0xFFFF\n",
    )])
    .expect("0xFFFF fits in a signed 32-bit Int");
    let errors = crate::check_program(&[(
        "Main.purs",
        "module Main where\nvalue :: Int\nvalue = 0x80000000\n",
    )])
    .expect_err("0x80000000 is outside signed 32-bit Int");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("IntOutOfRange"))
    );
}

#[test]
fn scalar_intrinsics_are_reachable_from_source_and_execute_with_documented_semantics() {
    let core = lower_source_to_core("Main.purs", SCALAR_SOURCE)
        .expect("typechecking and lowering source scalar intrinsics into Core");
    let core_dump = format!("{core:#?}");
    for operation in [
        "IntNeg",
        "IntComplement",
        "NumberNeg",
        "BooleanNot",
        "IntToNumber",
        "NumberToInt",
        "BooleanToInt",
        "IntToBoolean",
        "CharToInt",
        "IntToChar",
        "IntDiv",
        "IntMod",
        "I32Add",
        "I32Sub",
        "I32Mul",
        "I32DivS",
        "I32RemS",
        "IntAnd",
        "IntOr",
        "IntXor",
        "IntShl",
        "IntShr",
        "IntZshr",
        "I32Eq",
        "I32Ne",
        "I32LtS",
        "I32LeS",
        "I32GtS",
        "I32GeS",
        "NumberAdd",
        "NumberSub",
        "NumberMul",
        "NumberDiv",
        "NumberEq",
        "NumberNe",
        "NumberLt",
        "NumberLe",
        "NumberGt",
        "NumberGe",
        "BooleanAnd",
        "BooleanOr",
        "BooleanEq",
        "BooleanNe",
        "CharEq",
        "CharNe",
        "CharLt",
        "CharLe",
        "CharGt",
        "CharGe",
    ] {
        assert!(core_dump.contains(operation), "Core is missing {operation}");
    }

    let _artifact = compile_source("Main.purs", SCALAR_SOURCE)
        .expect("lowering all source scalar operations through Wasm emission");

    let Some(output) = super::run_with_wasmtime(SCALAR_SOURCE) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "scalar checks failed: {output:?}"
    );
}

#[test]
fn data_int_bits_uses_the_integer_bitwise_intrinsics() {
    let main = r#"
module Main where
import Data.Int.Bits ((.&.))
main = if intEq (6 .&. 3) 2 then 0 else 1
"#;
    let sources = [
        (
            "Data.Int.Bits.purs",
            include_str!("../../../../stdlib/lib/Data/Int/Bits.purs"),
        ),
        ("Main.purs", main),
    ];
    let Some(output) = super::run_program_with_wasmtime(&sources) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

const CASE_HELPER_SOURCE: &str = "\
module Main where

data Choice = First | Second

pick choice = case choice of
  First -> intDiv 7 2
  Second -> intMod 7 2

main = pick First
";

#[test]
fn generates_floor_helpers_for_division_nested_in_case_branches() {
    let stages = crate::compile_main_stages(CASE_HELPER_SOURCE)
        .expect("case-nested division must generate its helper before MIR lowering");
    assert!(
        stages.cc.functions.iter().any(|function| {
            function.assignments.iter().any(|assignment| {
                matches!(
                    assignment.kind,
                    psrs_backend::cc::AssignmentKind::TagSwitch { .. }
                )
            })
        }),
        "the case must lower to a tag switch"
    );
    for helper in ["__psrs_euclidean_int_div", "__psrs_euclidean_int_mod"] {
        assert_eq!(
            stages
                .mir
                .functions
                .iter()
                .filter(|function| function.name == helper)
                .count(),
            1,
            "expected exactly one {helper}"
        );
    }
    let Some(output) = super::run_with_wasmtime(CASE_HELPER_SOURCE) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(3),
        "floor div 7 2 through a case branch must be 3: {output:?}"
    );
}

const SHIFT_SOURCE: &str = "\
module Main where
import Prelude

checkShift = booleanAnd ((intShl 1 32) == 1) (booleanAnd ((intShl 1 33) == 2) (booleanAnd ((intZshr (intNeg 1) 1) == 2147483647) (booleanAnd ((intShr (intNeg 8) 33) == (intNeg 4)) ((intShl 3 31) == ((intNeg 2147483647) - 1)))))

main = if checkShift then 0 else 1
";

#[test]
fn shift_counts_are_taken_modulo_32() {
    let Some(output) = super::run_with_wasmtime(SHIFT_SOURCE) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "shift counts 32 and 33 must be taken modulo 32: {output:?}"
    );
}

const SATURATING_SOURCE: &str = "\
module Main where
import Prelude

i32min = (intNeg 2147483647) - 1

checkSat = booleanAnd ((numberToInt 2147483647.9) == 2147483647) (booleanAnd ((numberToInt (numberNeg 2147483648.0)) == i32min) (booleanAnd ((numberToInt (numberNeg 2147483648.9)) == i32min) (booleanAnd ((numberToInt (numberDiv 1.0 0.0)) == 2147483647) (booleanAnd ((numberToInt (numberDiv (numberNeg 1.0) 0.0)) == i32min) (booleanAnd ((numberToInt (numberDiv 0.0 0.0)) == 0) ((numberToInt (numberNeg 3.9)) == (intNeg 3)))))))

main = if checkSat then 0 else 1
";

#[test]
fn number_to_int_saturates_at_infinities_nan_and_the_i32_bounds() {
    let Some(output) = super::run_with_wasmtime(SATURATING_SOURCE) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "number-to-int thresholds must saturate and truncate as designed: {output:?}"
    );
}

const SIGNED_ZERO_SOURCE: &str = "\
module Main where

signedZero = booleanAnd (numberEq (numberNeg 0.0) 0.0) (numberGt (numberDiv 1.0 0.0) (numberDiv 1.0 (numberNeg 0.0)))

main = if signedZero then 0 else 1
";

const CHAR_BOUNDARY_SOURCE: &str = "\
module Main where
import Prelude

checkCharBound = booleanAnd ((charToInt 'A') == 65) (booleanAnd ((charToInt 'é') == 233) ((charToInt '�') == 65533))

main = if checkCharBound then 0 else 1
";

#[test]
fn accepts_a_supplementary_scalar_as_one_character() {
    for literal in [r"'\x10000'", r"'\x1F600'", "'😀'"] {
        let source = format!("module Main where\nmain = charToInt {literal}\n");
        check_source("Main.purs", &source).unwrap_or_else(|errors| {
            panic!("a supplementary scalar is one Char, per DEC-16: {errors:?}")
        });
    }
}

#[test]
fn rejects_an_unpaired_surrogate_in_a_character_literal() {
    for literal in [r"'\xD834'", r"'\xDF06'"] {
        let source = format!("module Main where\nmain = charToInt {literal}\n");
        let errors = check_source("Main.purs", &source).expect_err("lone surrogate Char literal");
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("unpaired surrogate")),
            "{errors:?}"
        );
    }
}

#[test]
fn char_operations_preserve_bmp_scalar_values() {
    let Some(output) = super::run_with_wasmtime(CHAR_BOUNDARY_SOURCE) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "numeric char conversions must preserve BMP scalar values: {output:?}"
    );
}

#[test]
fn number_operations_preserve_signed_zero() {
    let Some(output) = super::run_with_wasmtime(SIGNED_ZERO_SOURCE) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "negative zero must compare equal to zero but divide to negative infinity: {output:?}"
    );
}

fn assert_runtime_trap(source: &str, needle: &str) {
    let Some(output) = super::run_with_wasmtime(source) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert!(
        !output.status.success(),
        "expected a runtime trap for {source:?}: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(needle),
        "expected {needle:?} for {source:?}, got: {stderr}"
    );
}

#[test]
fn truncated_and_floor_division_trap_on_zero_divisor_and_signed_overflow() {
    assert_runtime_trap(
        "module Main where\nmain = 1 / 0\n",
        "integer divide by zero",
    );
    assert_runtime_trap(
        "module Main where\nmain = 1 % 0\n",
        "integer divide by zero",
    );
    assert_runtime_trap(
        "module Main where\nmain = intDiv 1 0\n",
        "integer divide by zero",
    );
    assert_runtime_trap(
        "module Main where\nmain = intMod 1 0\n",
        "integer divide by zero",
    );
    assert_runtime_trap(
        "module Main where\nmain = ((intNeg 2147483647) - 1) / (intNeg 1)\n",
        "integer overflow",
    );
    assert_runtime_trap(
        "module Main where\nmain = intDiv ((intNeg 2147483647) - 1) (intNeg 1)\n",
        "integer overflow",
    );
}

#[test]
fn runs_division_and_modulo_inside_a_case_arm() {
    // Floor division and modulo are lowered through generated helpers. The
    // primitive operations only appear inside the match arms, so helper
    // detection must walk the tag switch.
    let source = r#"module Main where
import Prelude
data Tag = A | B
compute t = case t of
  A -> intDiv 7 3
  B -> intMod 7 3
main = compute A + compute B + 39
"#;
    let compilation = compile_source_with_dumps("Main.purs", source)
        .expect("lowering division and modulo inside a case arm");
    assert!(compilation.dumps.mir.contains("__psrs_euclidean_int_div"));
    assert!(compilation.dumps.mir.contains("__psrs_euclidean_int_mod"));
    let Some(output) = super::run_with_wasmtime(source) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

const STRING_BYTES_SOURCE: &str = r#"module Main where
import Prelude

-- `stringToBytes` is lossless: a source string is a sequence of Unicode scalar
-- values, so every byte of its canonical UTF-8 encoding is one `Int` in
-- `0..255`. The array length is the encoded byte count, not the scalar-value
-- count.
asciiLength = arrayLength (stringToBytes "A") == 1
asciiByte = arrayIndex (stringToBytes "A") 0 == 65
-- "e-acute" U+00E9 is two UTF-8 bytes
accentLength = arrayLength (stringToBytes "é") == 2
accentFirst = arrayIndex (stringToBytes "é") 0 == 195
accentSecond = arrayIndex (stringToBytes "é") 1 == 169
-- U+1F600 is one four-byte sequence, not a surrogate pair
astralLength = arrayLength (stringToBytes "😀") == 4
astralLast = arrayIndex (stringToBytes "😀") 3 == 128
emptyLength = arrayLength (stringToBytes "") == 0

main =
  if booleanAnd asciiLength (booleanAnd asciiByte (booleanAnd accentLength
    (booleanAnd accentFirst (booleanAnd accentSecond (booleanAnd astralLength
      (booleanAnd astralLast emptyLength))))))
  then 0
  else 1
"#;

#[test]
fn string_to_bytes_yields_the_canonical_utf8_encoding() {
    let Some(output) = super::run_with_wasmtime(STRING_BYTES_SOURCE) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "each UTF-8 byte must become one Int in 0..255: {output:?}"
    );
}

const STRING_BYTES_ROUND_TRIP_SOURCE: &str = r#"module Main where
import Prelude

-- `bytesToString` is the checked direction: it reads the canonical UTF-8 of a
-- scalar sequence back as a `String`, and that string re-encodes to exactly the
-- same bytes.
asciiLength = arrayLength (stringToBytes (bytesToString [104, 105])) == 2
asciiFirst = arrayIndex (stringToBytes (bytesToString [104, 105])) 0 == 104
asciiSecond = arrayIndex (stringToBytes (bytesToString [104, 105])) 1 == 105
accentLength = arrayLength (stringToBytes (bytesToString [195, 169])) == 2
astralLength = arrayLength (stringToBytes (bytesToString [240, 159, 152, 128])) == 4
emptyLength = arrayLength (stringToBytes (bytesToString [])) == 0

main =
  if booleanAnd asciiLength (booleanAnd asciiFirst (booleanAnd asciiSecond
    (booleanAnd accentLength (booleanAnd astralLength emptyLength))))
  then 0
  else 1
"#;

#[test]
fn bytes_to_string_round_trips_canonical_utf8() {
    let Some(output) = super::run_with_wasmtime(STRING_BYTES_ROUND_TRIP_SOURCE) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "valid UTF-8 bytes must round-trip through a source String: {output:?}"
    );
}

const STRING_BYTES_REJECTED_SOURCE: &str = r#"module Main where
import Prelude

-- A truncated two-byte sequence is malformed, so `bytesToString` rejects it
-- instead of producing replacement text.
main = arrayLength (stringToBytes (bytesToString [195])) * 0
"#;

/// An element outside `0..255` is not a byte, so it must trap before the
/// `i32.store8` that would otherwise truncate it.
const STRING_BYTES_OUT_OF_RANGE_SOURCE: &str = r#"module Main where
import Prelude

main = arrayLength (stringToBytes (bytesToString [256])) * 0
"#;

#[test]
fn bytes_to_string_traps_on_out_of_range_elements() {
    let Some(output) = super::run_with_wasmtime(STRING_BYTES_OUT_OF_RANGE_SOURCE) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert!(
        !output.status.success(),
        "an element above 255 must trap instead of being narrowed to a byte: {output:?}"
    );
}

#[test]
fn bytes_to_string_traps_on_malformed_utf8() {
    let Some(output) = super::run_with_wasmtime(STRING_BYTES_REJECTED_SOURCE) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert!(
        !output.status.success(),
        "a malformed UTF-8 byte sequence must trap, not decode to U+FFFD: {output:?}"
    );
}
