use super::*;

#[test]
fn public_number_acos_matches_domain_boundaries_and_rejects_exterior_values() {
    let source = r#"
module Main where
import Prelude
import Data.Number as Number
foreign import "psrs:intrinsic#numberNeg" negative :: Number -> Number
apply f value = f value
positiveInfinity = 1.0 / 0.0
negativeInfinity = negative positiveInfinity
nan = 0.0 / 0.0
checks = Number.acos 1.0 == 0.0
  && 1.0 / Number.acos 1.0 == positiveInfinity
  && Number.acos (-1.0) == 3.141592653589793
  && Number.acos 0.0 == 1.5707963267948966
  && Number.acos (negative 0.0) == 1.5707963267948966
  && Number.acos 0.5 == 1.0471975511965979
  && Number.acos (-0.5) == 2.0943951023931957
  && Number.acos 2.0 /= Number.acos 2.0
  && Number.acos (-2.0) /= Number.acos (-2.0)
  && Number.acos positiveInfinity /= Number.acos positiveInfinity
  && Number.acos negativeInfinity /= Number.acos negativeInfinity
  && Number.acos nan /= Number.acos nan
  && apply Number.acos 1.0 == 0.0
main :: Int
main = if checks then 42 else 1
"#;
    let artifact = compile_program_sources_with_prelude(&[("Main.purs", source)])
        .expect("inverse cosine should compile");
    assert!(
        artifact
            .wasm
            .windows(b"number_acos".len())
            .any(|window| window == b"number_acos"),
        "the numeric runtime export should be linked"
    );
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn number_acos_requires_number_operand_and_result() {
    for ty in ["Int -> Number", "Number -> Int", "forall a. a -> a"] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#numberAcos\" angle :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("inverse cosine requires a checked Number contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking")
        );
    }
}
