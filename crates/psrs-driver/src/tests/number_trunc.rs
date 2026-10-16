use super::*;

#[test]
fn number_trunc_preserves_number_range_nonfinite_values_and_signed_zero() {
    let source = r#"
module Main where
foreign import "psrs:intrinsic#numberTrunc" truncate :: Number -> Number
foreign import "psrs:intrinsic#numberDiv" divide :: Number -> Number -> Number
foreign import "psrs:intrinsic#numberNeg" negative :: Number -> Number
foreign import "psrs:intrinsic#numberEq" equal :: Number -> Number -> Boolean
foreign import "psrs:intrinsic#numberNe" unequal :: Number -> Number -> Boolean
foreign import "psrs:intrinsic#booleanAnd" and :: Boolean -> Boolean -> Boolean
fraction = and (equal (truncate 42.9) 42.0) (equal (truncate (negative 42.9)) (negative 42.0))
large = and (equal (truncate 4294967296.5) 4294967296.0) (equal (truncate (negative 4294967296.5)) (negative 4294967296.0))
zeros = and (equal (divide 1.0 (truncate 0.5)) (divide 1.0 0.0)) (equal (divide 1.0 (truncate (negative 0.5))) (divide (negative 1.0) 0.0))
nonfinite = and (equal (truncate (divide 1.0 0.0)) (divide 1.0 0.0)) (and (equal (truncate (divide (negative 1.0) 0.0)) (divide (negative 1.0) 0.0)) (unequal (truncate (divide 0.0 0.0)) (truncate (divide 0.0 0.0))))
main = if and fraction (and large (and zeros nonfinite)) then 42 else 1
"#;
    let mir = lower_source_to_mir(source);
    assert!(format!("{mir:#?}").contains("F64Trunc"));
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn number_trunc_checks_foreign_operand_and_result_contracts() {
    for ty in ["Int -> Number", "Number -> Int", "forall a. a -> a"] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#numberTrunc\" truncate :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("truncation requires Number operand and result");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking")
        );
    }
}
