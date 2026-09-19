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

#[test]
fn runs_a_concrete_number_array_literal() {
    let source = "module Main where\nmake = [1.5, 2.5, 3.5]\nmain = let ignored = make in 42\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a Number array literal");
    assert!(artifact.wat.contains("array.new_fixed"));
    assert!(artifact.wat.contains("f64.const"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_array_length_through_the_gc_array() {
    let source = "module Main where\nmain = arrayLength [10, 20, 30]\n";
    let artifact = compile_source("Main.purs", source).expect("lowering array length");
    assert!(artifact.wat.contains("array.len"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn runs_number_array_length_through_the_gc_array() {
    let source = "module Main where\nmain = arrayLength [10.0, 20.0, 30.0]\n";
    let artifact = compile_source("Main.purs", source).expect("lowering Number array length");
    assert!(artifact.wat.contains("array.len"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn runs_array_index_through_the_gc_array() {
    let source = "module Main where\nmain = arrayIndex [10, 20, 30] 1\n";
    let artifact = compile_source("Main.purs", source).expect("lowering array index");
    assert!(artifact.wat.contains("array.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(20));
}

#[test]
fn runs_number_array_index_through_the_gc_array() {
    let source = "module Main where\nuse :: Number -> Int\nuse x = 42\nmain = use (arrayIndex [10.0, 20.0, 30.0] 1)\n";
    let artifact = compile_source("Main.purs", source).expect("lowering Number array index");
    assert!(artifact.wat.contains("array.get"));
    assert!(artifact.wat.contains("f64.const"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_array_update_through_the_gc_array() {
    let source = "module Main where\nmain = arrayIndex (arrayUpdate [10, 20, 30] 1 42) 1\n";
    let artifact = compile_source("Main.purs", source).expect("lowering array update");
    assert!(artifact.wat.contains("array.set"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_number_array_update_through_the_gc_array() {
    let source = "module Main where\nuse :: Number -> Int\nuse x = 42\nmain = use (arrayIndex (arrayUpdate [10.0, 20.0, 30.0] 1 42.0) 1)\n";
    let artifact = compile_source("Main.purs", source).expect("lowering Number array update");
    assert!(artifact.wat.contains("array.set"));
    assert!(artifact.wat.contains("f64.const"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}
