use super::*;

#[test]
fn runs_an_empty_integer_array_literal() {
    let source = "\
module Main where
values :: Array Int
values = []
main = 1 + arrayLength values
";
    let artifact = compile_source("Main.purs", source).expect("an empty array should compile");
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        let _ = artifact;
        return;
    };
    assert_eq!(output.status.code(), Some(1), "{artifact:?} {output:?}");
}

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
    let source = "module Main where\nvalues = [10, 20, 30]\nmain = arrayLength values\n";
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
    let source = "module Main where\nvalues = [10.0, 20.0, 30.0]\nmain = arrayLength values\n";
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
    let source = "module Main where\nvalues = [10, 20, 30]\nmain = arrayIndex values 1\n";
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
    let source = "module Main where\nvalues = [10.0, 20.0, 30.0]\nuse :: Number -> Int\nuse x = 42\nmain = use (arrayIndex values 1)\n";
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

#[test]
fn preserves_the_original_gc_array_when_updating_a_copy() {
    let source = "module Main where\nmain = let original = [10, 20] in let updated = arrayUpdate original 0 99 in arrayIndex original 0 + arrayIndex updated 0\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a pure array update");
    assert!(artifact.wat.contains("array.copy"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(109));
}

#[test]
fn preserves_aliases_when_updating_a_gc_array() {
    let source = "module Main where\nmain = let original = [10, 20] in let alias = original in let updated = arrayUpdate original 0 99 in arrayIndex alias 0 + arrayIndex updated 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(109));
}

#[test]
fn keeps_independent_gc_array_updates_independent() {
    let source = "module Main where\nmain = let original = [10, 20] in let first = arrayUpdate original 0 99 in let second = arrayUpdate original 1 77 in arrayIndex first 0 + arrayIndex second 1\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(176));
}

#[test]
fn updates_an_array_of_non_null_references() {
    let source = "module Main where\nmain = let original = [{ answer: 10 }] in let updated = arrayUpdate original 0 { answer: 42 } in case arrayIndex updated 0 of\n  { answer: result } -> result\n";
    let artifact = compile_source("Main.purs", source)
        .expect("array cloning must support non-null reference elements");
    assert!(artifact.wat.contains("array.new_default"));
    assert!(artifact.wat.contains("ref.cast"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_an_array_of_records_through_the_gc_array() {
    let source = "module Main where\nvalues = [{ answer: 42 }]\nzero = 0\nmain = case arrayIndex values zero of\n  { answer: result } -> result\n";
    let artifact = compile_source("Main.purs", source).expect("lowering an array of records");
    assert!(artifact.wat.contains("array.new_fixed"));
    assert!(artifact.wat.contains("struct.new"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_an_array_of_data_values_through_the_gc_array() {
    let source = "module Main where\ndata Box = Box Int\nvalues = [Box 42]\nzero = 0\nmain = case arrayIndex values zero of\n  Box result -> result\n";
    let artifact = compile_source("Main.purs", source).expect("lowering an array of data values");
    assert!(artifact.wat.contains("array.new_fixed"));
    assert!(artifact.wat.contains("struct.new"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_record_containing_an_array() {
    let source = "module Main where\ninput = { values: [40, 42] }\nmain = case input of\n  { values: values } -> arrayIndex values 1\n";
    let artifact =
        compile_source("Main.purs", source).expect("lowering a record containing an array");
    assert!(artifact.wat.contains("array.new_fixed"));
    assert!(artifact.wat.contains("struct.new"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_nested_array_through_the_gc_arrays() {
    let source = "module Main where\nmain = arrayIndex (arrayIndex [[40, 42]] 0) 1\n";
    let artifact = compile_source("Main.purs", source).expect("lowering nested arrays");
    assert!(artifact.wat.contains("array.new_fixed"));
    assert!(artifact.wat.contains("array.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}
