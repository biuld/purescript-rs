use super::*;

#[test]
fn runs_a_top_level_function_value_through_call_ref() {
    let source = r#"module Main where
apply :: (Int -> Int) -> Int -> Int
apply f x = f x
inc :: Int -> Int
inc x = x + 1
main = apply inc 41
"#;
    let artifact = compile_source("Main.purs", source).expect("lowering a higher-order call");
    assert!(artifact.wat.contains("ref.func"));
    assert!(artifact.wat.contains("call_ref"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_non_capturing_local_lambda_through_call_ref() {
    let source = r#"module Main where
main = let f = \x -> x + 1 in f 41
"#;
    let artifact = compile_source("Main.purs", source).expect("lowering a local lambda");
    assert!(artifact.wat.contains("ref.func"));
    assert!(artifact.wat.contains("call_ref"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn rejects_a_capturing_lambda_until_closure_conversion() {
    let source = r#"module Main where
main = let x = 40 in let f = \y -> x + y in f 2
"#;
    let errors =
        compile_source("Main.purs", source).expect_err("capturing lambda should be explicit");
    assert!(errors.iter().any(|error| {
        error.stage == "P8 closure conversion"
            && error
                .message
                .contains("capturing lambdas require closure conversion")
    }));
}
