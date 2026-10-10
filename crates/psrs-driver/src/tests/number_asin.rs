use super::*;

#[test]
fn public_number_asin_matches_domain_boundaries_zero_signs_and_exterior_values() {
    let source = r#"
module Main where
import Prelude
import Data.Number as Number
foreign import "psrs:intrinsic#numberNeg" negative :: Number -> Number
apply f value = f value
positiveInfinity = 1.0 / 0.0
negativeInfinity = negative positiveInfinity
nan = 0.0 / 0.0
checks = Number.asin 0.0 == 0.0
  && 1.0 / Number.asin 0.0 == positiveInfinity
  && 1.0 / Number.asin (negative 0.0) == negativeInfinity
  && Number.asin 1.0 == 1.5707963267948966
  && Number.asin (-1.0) == negative 1.5707963267948966
  && Number.asin 0.5 == 0.5235987755982989
  && Number.asin (-0.5) == negative 0.5235987755982989
  && Number.asin 2.0 /= Number.asin 2.0
  && Number.asin (-2.0) /= Number.asin (-2.0)
  && Number.asin positiveInfinity /= Number.asin positiveInfinity
  && Number.asin negativeInfinity /= Number.asin negativeInfinity
  && Number.asin nan /= Number.asin nan
  && apply Number.asin 0.0 == 0.0
main :: Int
main = if checks then 42 else 1
"#;
    let artifact = compile_program_sources_with_prelude(&[("Main.purs", source)])
        .expect("inverse sine should compile");
    assert!(
        artifact
            .wasm
            .windows(b"number_asin".len())
            .any(|window| window == b"number_asin"),
        "the numeric runtime export should be linked"
    );
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn number_asin_requires_number_operand_and_result() {
    for ty in ["Int -> Number", "Number -> Int", "forall a. a -> a"] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#numberAsin\" angle :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("inverse sine requires a checked Number contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking")
        );
    }
}
