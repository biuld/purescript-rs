use super::*;

#[test]
fn number_floor_and_ceil_preserve_range_nonfinite_values_and_signed_zero() {
    for (binding, instruction, positive, negative, large, zero_input) in [
        (
            "numberFloor",
            "F64Floor",
            "42.0",
            "43.0",
            "4294967296.0",
            "0.5",
        ),
        (
            "numberCeil",
            "F64Ceil",
            "43.0",
            "42.0",
            "4294967297.0",
            "(negative 0.5)",
        ),
    ] {
        let zero_sign = if binding == "numberFloor" {
            "1.0"
        } else {
            "(negative 1.0)"
        };
        let source = format!(
            r#"
module Main where
foreign import "psrs:intrinsic#{binding}" rounding :: Number -> Number
foreign import "psrs:intrinsic#numberDiv" divide :: Number -> Number -> Number
foreign import "psrs:intrinsic#numberNeg" negative :: Number -> Number
foreign import "psrs:intrinsic#numberEq" equal :: Number -> Number -> Boolean
foreign import "psrs:intrinsic#numberNe" unequal :: Number -> Number -> Boolean
foreign import "psrs:intrinsic#booleanAnd" and :: Boolean -> Boolean -> Boolean
fraction = and (equal (rounding 42.9) {positive}) (equal (rounding (negative 42.9)) (negative {negative}))
large = equal (rounding 4294967296.5) {large}
zeros = and (equal (divide 1.0 (rounding {zero_input})) (divide {zero_sign} 0.0)) (equal (divide 1.0 (rounding (negative 0.0))) (divide (negative 1.0) 0.0))
nonfinite = and (equal (rounding (divide 1.0 0.0)) (divide 1.0 0.0)) (and (equal (rounding (divide (negative 1.0) 0.0)) (divide (negative 1.0) 0.0)) (unequal (rounding (divide 0.0 0.0)) (rounding (divide 0.0 0.0))))
main = if and fraction (and large (and zeros nonfinite)) then 42 else 1
"#
        );
        let mir = lower_source_to_mir(&source);
        assert!(format!("{mir:#?}").contains(instruction));
        let Some(output) = run_with_wasmtime(&source) else {
            return;
        };
        assert_eq!(output.status.code(), Some(42), "{binding}: {output:?}");
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
    }
}

#[test]
fn number_floor_and_ceil_reject_invalid_foreign_contracts() {
    for binding in ["numberFloor", "numberCeil"] {
        for ty in ["Int -> Number", "Number -> Int", "forall a. a -> a"] {
            let source = format!(
                "module Main where\nforeign import \"psrs:intrinsic#{binding}\" rounding :: {ty}\nmain = 0\n"
            );
            let errors = compile_program_sources(&[("Main.purs", &source)])
                .expect_err("rounding requires Number operand and result");
            assert!(
                errors
                    .iter()
                    .any(|error| error.diagnostic.stage == "P8 primitive linking")
            );
        }
    }
}
