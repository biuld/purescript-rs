use super::*;

#[test]
fn public_number_atan_matches_zero_signs_unit_slopes_and_infinities() {
    let source = r#"
module Main where
import Prelude
import Data.Number as Number
foreign import "psrs:intrinsic#numberNeg" negative :: Number -> Number
apply f value = f value
positiveInfinity = 1.0 / 0.0
negativeInfinity = negative positiveInfinity
nan = 0.0 / 0.0
tiny = 5.0e-324
checks = tiny > 0.0
  && Number.atan tiny == tiny
  && Number.atan (negative tiny) == negative tiny
  && Number.atan 0.0 == 0.0
  && 1.0 / Number.atan 0.0 == positiveInfinity
  && 1.0 / Number.atan (negative 0.0) == negativeInfinity
  && Number.atan 1.0 == 0.7853981633974483
  && Number.atan (-1.0) == negative 0.7853981633974483
  && Number.atan positiveInfinity == 1.5707963267948966
  && Number.atan negativeInfinity == negative 1.5707963267948966
  && Number.atan nan /= Number.atan nan
  && apply Number.atan 0.0 == 0.0
main :: Int
main = if checks then 42 else 1
"#;
    let artifact = compile_program_sources_with_prelude(&[("Main.purs", source)])
        .expect("inverse tangent should compile");
    assert!(
        artifact
            .wasm
            .windows(b"number_atan".len())
            .any(|window| window == b"number_atan"),
        "the numeric runtime export should be linked"
    );
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn number_atan_requires_number_operand_and_result() {
    for ty in ["Int -> Number", "Number -> Int", "forall a. a -> a"] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#numberAtan\" angle :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("inverse tangent requires a checked Number contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking")
        );
    }
}
