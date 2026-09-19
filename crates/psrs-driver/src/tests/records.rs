use super::*;

#[test]
fn runs_a_record_field_access_through_a_gc_struct() {
    let source = "module Main where\nmain = { ignored: 10, answer: 42 }.answer\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a record field access");
    assert!(artifact.wat.contains("struct.new"));
    assert!(artifact.wat.contains("struct.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_record_update_through_a_gc_struct() {
    let source = "module Main where\nmain = { ignored: 10, answer: 1 } { answer = 42 }.answer\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a record update");
    assert!(artifact.wat.contains("struct.new"));
    assert!(artifact.wat.contains("struct.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_nested_record_update_through_a_gc_struct() {
    let source = "module Main where\nmain = { address: { city: 1, zip: 2 }, ignored: 0 } { address { city = 42 } }.address.city\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a nested record update");
    assert!(artifact.wat.contains("struct.new"));
    assert!(artifact.wat.contains("struct.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_record_pattern_in_a_case() {
    let source = "module Main where\nmain = case { ignored: 10, answer: 42 } of\n  { answer: value } -> value\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a record pattern");
    assert!(artifact.wat.contains("struct.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_record_pattern_in_a_function_parameter() {
    let source = "module Main where\nanswer { ignored: ignored, answer: value } = value + ignored\nmain = answer { ignored: 10, answer: 32 }\n";
    let artifact =
        compile_source("Main.purs", source).expect("lowering a record pattern function parameter");
    assert!(artifact.wat.contains("struct.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn rejects_a_record_update_with_an_unknown_field() {
    let source = "module Main where\nmain = { answer: 1 } { missing = 42 }.answer\n";
    let errors = compile_source("Main.purs", source).expect_err("unknown record field");
    assert!(errors.iter().any(|error| {
        error.stage == "P5 typecheck" && error.message.contains("record has no field `missing`")
    }));
}

#[test]
fn rejects_a_record_update_with_duplicate_fields() {
    let source = "module Main where\nmain = { answer: 1 } { answer = 2, answer = 3 }.answer\n";
    let errors = compile_source("Main.purs", source).expect_err("duplicate record field");
    assert!(errors.iter().any(|error| {
        error.stage == "P5 typecheck"
            && error
                .message
                .contains("record label `answer` occurs more than once")
    }));
}

#[test]
fn rejects_a_record_pattern_with_an_unknown_field() {
    let source = "module Main where\nmain = case { answer: 1 } of\n  { missing: value } -> value\n";
    let errors = compile_source("Main.purs", source).expect_err("unknown record pattern field");
    assert!(errors.iter().any(|error| {
        error.stage == "P5 typecheck" && error.message.contains("record has no field `missing`")
    }));
}

#[test]
fn rejects_open_record_patterns() {
    let source =
        "module Main where\nmain = case { answer: 1 } of\n  { answer: value ..rest } -> value\n";
    let errors = compile_source("Main.purs", source).expect_err("open record pattern");
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("open record patterns are not supported yet")
    }));
}
