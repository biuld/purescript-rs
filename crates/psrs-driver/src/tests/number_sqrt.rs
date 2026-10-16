use super::*;

#[test]
fn public_number_sqrt_matches_ieee_squares_zero_signs_and_nonfinite_values() {
    let source = r#"
module Main where
import Prelude
import Data.Number as Number
foreign import "psrs:intrinsic#numberNeg" negative :: Number -> Number
apply f value = f value
positiveInfinity = 1.0 / 0.0
negativeInfinity = negative positiveInfinity
nan = 0.0 / 0.0
checks = Number.sqrt 4.0 == 2.0
  && Number.sqrt 9.0 == 3.0
  && Number.sqrt 4294967296.0 == 65536.0
  && Number.sqrt 0.0 == 0.0
  && 1.0 / Number.sqrt 0.0 == positiveInfinity
  && 1.0 / Number.sqrt (negative 0.0) == negativeInfinity
  && Number.sqrt positiveInfinity == positiveInfinity
  && Number.sqrt (-1.0) /= Number.sqrt (-1.0)
  && Number.sqrt negativeInfinity /= Number.sqrt negativeInfinity
  && Number.sqrt nan /= Number.sqrt nan
  && apply Number.sqrt 4.0 == 2.0
main :: Int
main = if checks then 42 else 1
"#;
    let mir = lower_source_to_mir(source);
    assert!(format!("{mir:#?}").contains("F64Sqrt"));
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn number_sqrt_requires_number_operand_and_result() {
    for ty in ["Int -> Number", "Number -> Int", "forall a. a -> a"] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#numberSqrt\" root :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("square root requires a checked Number contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking")
        );
    }
}
