use super::*;

#[test]
fn public_number_atan2_matches_quadrants_zero_signs_and_infinities() {
    let source = r#"
module Main where
import Prelude
import Data.Number as Number
foreign import "psrs:intrinsic#numberNeg" negative :: Number -> Number
apply f y x = f y x
positiveInfinity = 1.0 / 0.0
negativeInfinity = negative positiveInfinity
nan = 0.0 / 0.0
checks = Number.atan2 0.0 1.0 == 0.0
  && 1.0 / Number.atan2 0.0 1.0 == positiveInfinity
  && 1.0 / Number.atan2 (negative 0.0) 1.0 == negativeInfinity
  && Number.atan2 0.0 (negative 1.0) == 3.141592653589793
  && Number.atan2 (negative 0.0) (negative 1.0) == negative 3.141592653589793
  && Number.atan2 1.0 0.0 == 1.5707963267948966
  && Number.atan2 (negative 1.0) 0.0 == negative 1.5707963267948966
  && Number.atan2 1.0 (negative 0.0) == 1.5707963267948966
  && Number.atan2 1.0 positiveInfinity == 0.0
  && 1.0 / Number.atan2 (negative 1.0) positiveInfinity == negativeInfinity
  && Number.atan2 1.0 negativeInfinity == 3.141592653589793
  && Number.atan2 positiveInfinity positiveInfinity == 0.7853981633974483
  && Number.atan2 positiveInfinity negativeInfinity == 2.356194490192345
  && Number.atan2 negativeInfinity negativeInfinity == negative 2.356194490192345
  && Number.atan2 0.1 (negative 1.0e-20) == 1.5707963267948966
  && Number.atan2 nan 1.0 /= Number.atan2 nan 1.0
  && Number.atan2 1.0 nan /= Number.atan2 1.0 nan
  && apply Number.atan2 0.0 1.0 == 0.0
main :: Int
main = if checks then 42 else 1
"#;
    let artifact = compile_program_sources_with_prelude(&[("Main.purs", source)])
        .expect("four-quadrant inverse tangent should compile");
    assert!(
        artifact
            .wasm
            .windows(b"number_atan2".len())
            .any(|window| window == b"number_atan2"),
        "the numeric runtime export should be linked"
    );
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn number_atan2_requires_number_operands_and_result() {
    for ty in [
        "Int -> Number -> Number",
        "Number -> Number -> Int",
        "forall a. a -> a -> a",
    ] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#numberAtan2\" angle :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("four-quadrant inverse tangent requires a checked Number contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking")
        );
    }
}
