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
fn runs_a_capturing_lambda_through_a_closure() {
    let source = r#"module Main where
apply :: (Int -> Int) -> Int -> Int
apply f x = f x
main = let x = 40 in apply (\y -> x + y) 2
"#;
    let artifact = compile_source("Main.purs", source).expect("lowering a capturing lambda");
    assert!(artifact.wat.contains("array.new_fixed"));
    assert!(artifact.wat.contains("struct.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}
