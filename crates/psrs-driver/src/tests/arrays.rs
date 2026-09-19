use super::*;

#[test]
fn runs_a_concrete_integer_array_literal() {
    let source = "module Main where\nmake = [1, 2, 3]\nmain = let ignored = make in 42\n";
    let artifact = compile_source("Main.purs", source).expect("lowering an array literal");
    assert!(artifact.wat.contains("array.new_fixed"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}
