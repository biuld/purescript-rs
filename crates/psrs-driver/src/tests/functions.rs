use super::*;

#[test]
fn runs_a_top_level_function_value_through_call_ref() {
    let source = "module Main where\n\
apply :: (Int -> Int) -> Int -> Int\n\
apply f x = f x\n\
inc :: Int -> Int\n\
inc x = x + 1\n\
main = apply inc 41\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a higher-order call");
    assert!(artifact.wat.contains("ref.func"));
    assert!(artifact.wat.contains("call_ref"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}
