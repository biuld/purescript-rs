use super::*;

#[test]
fn public_number_elementary_functions_match_javascript_special_cases() {
    let source = r#"
module Main where
import Prelude
import Data.Number as Number
foreign import "psrs:intrinsic#numberNeg" negative :: Number -> Number
positiveInfinity = 1.0 / 0.0
negativeInfinity = negative positiveInfinity
zero = Number.sin 0.0 == 0.0
  && 1.0 / Number.sin 0.0 == positiveInfinity
  && 1.0 / Number.sin (negative 0.0) == negativeInfinity
  && Number.cos 0.0 == 1.0
  && Number.tan 0.0 == 0.0
  && 1.0 / Number.tan (negative 0.0) == negativeInfinity
  && Number.exp 0.0 == 1.0
  && Number.log 1.0 == 0.0
  && Number.log positiveInfinity == positiveInfinity
  && Number.log 0.0 == negativeInfinity
  && Number.isNaN (Number.log (negative 1.0))
  && Number.isNaN Number.nan
  && Number.isNaN (Number.cos positiveInfinity)
  && not (Number.isNaN 0.0)
  && not (Number.isNaN Number.infinity)
  && Number.infinity == positiveInfinity
  && negative Number.infinity == negativeInfinity
  && Number.min 1.0 2.0 == 1.0
  && Number.max 1.0 2.0 == 2.0
  && 1.0 / Number.min (negative 0.0) 0.0 == negativeInfinity
  && 1.0 / Number.max (negative 0.0) 0.0 == positiveInfinity
  && 1.0 / Number.max (negative 0.0) (negative 0.0) == negativeInfinity
  && Number.isNaN (Number.min Number.nan 1.0)
  && Number.isNaN (Number.max 1.0 Number.nan)
  && Number.pow 3.0 2.0 == 9.0
  && Number.pow Number.nan 0.0 == 1.0
  && Number.isNaN (Number.pow 1.0 Number.nan)
  && Number.isNaN (Number.pow 1.0 Number.infinity)
  && Number.isNaN (Number.pow (negative 1.0) negativeInfinity)
  && Number.remainder 5.3 2.0 == 1.2999999999999998
  && 1.0 / Number.remainder (negative 1.0) 1.0 == negativeInfinity
  && Number.isNaN (Number.remainder 1.0 0.0)
  && Number.isNaN (Number.remainder positiveInfinity 1.0)
  && Number.sign 4.0 == 1.0
  && Number.sign (negative 4.0) == negative 1.0
  && Number.sign Number.infinity == 1.0
  && Number.sign negativeInfinity == negative 1.0
  && 1.0 / Number.sign (negative 0.0) == negativeInfinity
  && Number.isNaN (Number.sign Number.nan)
main :: Int
main = if zero then 42 else 1
"#;
    let artifact = compile_program_sources_with_prelude(&[("Main.purs", source)])
        .expect("remaining Number foreigns should compile");
    for export in [
        "number_sin",
        "number_cos",
        "number_tan",
        "number_exp",
        "number_log",
        "number_pow",
        "number_min",
        "number_max",
        "number_sign",
        "number_remainder",
        "number_is_nan",
        "number_nan",
        "number_infinity",
    ] {
        assert!(
            artifact
                .wasm
                .windows(export.len())
                .any(|window| window == export.as_bytes()),
            "{export} should be linked"
        );
    }
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn number_constants_and_predicates_require_their_checked_types() {
    for (binding, ty) in [
        ("numberNan", "Int"),
        ("numberInfinity", "Int"),
        ("numberIsNaN", "Number -> Number"),
        ("numberRemainder", "Int -> Number -> Number"),
        ("numberPow", "forall a. a -> a -> a"),
    ] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#{binding}\" value :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("the binding requires its checked Number contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking"),
            "{binding} {ty}: {errors:?}"
        );
    }
}
