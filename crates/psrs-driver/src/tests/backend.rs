use super::*;

#[test]
fn structures_wasm_ir_with_a_structured_region() {
    let source =
        "module Main where\nchoose condition = if condition then 9 else 2\nmain = choose true\n";
    let core = lower_source_to_core("Main.purs", source).unwrap();
    let stages = psrs_backend::compile_with_stages(core).unwrap();
    assert!(
        stages.wasm.functions.iter().any(|function| function
            .body
            .iter()
            .any(|op| matches!(op, psrs_backend::wasm::Op::Block { .. }))),
        "expected a structured block region in the Wasm IR"
    );
}

#[test]
fn reports_type_errors_with_source_ranges() {
    let source = "module Main where\nmain = if 1 then 2 else 3\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(errors.iter().any(|error| error.stage == "P5 typecheck"));
    let integer_offset = source.find("1 then").unwrap() as u32;
    assert!(
        errors
            .iter()
            .any(|error| error.span == TextRange::new(integer_offset, integer_offset + 1))
    );
}
