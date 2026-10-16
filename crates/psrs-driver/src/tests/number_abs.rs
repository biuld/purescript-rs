use super::*;

#[test]
fn public_number_abs_preserves_magnitude_and_canonicalizes_negative_zero() {
    let source = r#"
module Main where
import Prelude
import Data.Number as Number
apply f value = f value
positiveInfinity = 1.0 / 0.0
negativeInfinity = negate positiveInfinity
nan = 0.0 / 0.0
checks = Number.abs (-42.5) == 42.5
  && Number.abs 42.5 == 42.5
  && Number.abs (-4294967296.5) == 4294967296.5
  && Number.abs (-5.0e-324) == 5.0e-324
  && Number.abs (-1.7976931348623157e308) == 1.7976931348623157e308
  && 1.0 / Number.abs (negate 0.0) == positiveInfinity
  && 1.0 / Number.abs 0.0 == positiveInfinity
  && Number.abs negativeInfinity == positiveInfinity
  && Number.abs positiveInfinity == positiveInfinity
  && Number.abs nan /= Number.abs nan
  && apply Number.abs (-42.0) == 42.0
main :: Int
main = if checks then 42 else 1
"#;
    let mir = lower_source_to_mir(source);
    assert!(format!("{mir:#?}").contains("F64Abs"));
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn number_abs_requires_number_operand_and_result() {
    for ty in ["Int -> Number", "Number -> Int", "forall a. a -> a"] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#numberAbs\" magnitude :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("absolute value requires a checked Number contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking")
        );
    }
}
