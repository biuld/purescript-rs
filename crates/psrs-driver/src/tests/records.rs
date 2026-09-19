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
