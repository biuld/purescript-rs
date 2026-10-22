use super::*;

#[path = "../support/record_kinds.rs"]
mod cases;

#[test]
fn differential_record_shorthand_and_explicit_primitive_kinds() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
    let inputs = cases::sources(cases::MAIN);
    let official = purs_sources_output("imported-record-kinds", &inputs);
    assert!(official.status.success(), "{official:?}");
    psrs_driver::check_program(&inputs).expect("imported aliases have a Type result");
    let official = purs_sources_output("row-symbol", &[("Main.purs", cases::ROW_SYMBOL)]);
    assert!(official.status.success(), "{official:?}");
    psrs_driver::check_source("Main.purs", cases::ROW_SYMBOL).expect("bare Row Symbol stays valid");
    for (name, source) in cases::REJECT {
        let official = purs_sources_output(name, &[("Main.purs", *source)]);
        assert!(
            purs_error_codes(&official).contains(&"KindsDoNotUnify".to_owned()),
            "{name}: {official:?}"
        );
        let errors = psrs_driver::check_source(name, source).expect_err("records require Row Type");
        assert!(
            errors
                .iter()
                .any(|error| error.code == Some("KindsDoNotUnify")),
            "{name}: {errors:?}"
        );
    }
}
