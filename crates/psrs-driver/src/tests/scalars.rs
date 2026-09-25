use super::*;

const SCALAR_SOURCE: &str = r#"
module Main where

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
        "IntAdd",
        "IntSub",
        "IntMul",
        "IntQuot",
        "IntRem",
        "IntAnd",
        "IntOr",
        "IntXor",
        "IntShl",
        "IntShr",
        "IntZshr",
        "IntEq",
        "IntNe",
        "IntLt",
        "IntLe",
        "IntGt",
        "IntGe",
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
fn runs_division_and_modulo_inside_a_case_arm() {
    // Floor division and modulo are lowered through generated helpers. The
    // primitive operations only appear inside the match arms, so helper
    // detection must walk the tag switch.
    let source = r#"module Main where
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
