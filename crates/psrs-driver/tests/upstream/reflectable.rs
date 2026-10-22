use super::*;

#[path = "../support/reflectable.rs"]
mod cases;

#[test]
fn differential_reflectable_literals_and_rejected_result_type() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
    for (name, main) in cases::cases() {
        let inputs = cases::inputs(&main);
        let official = purs_sources_output(&format!("reflectable-{name}"), &inputs);
        assert!(official.status.success(), "{name}: {official:?}");
        psrs_driver::check_program(&inputs).unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
    }
    let main = "module Main where\nimport Data.Reflectable\nimport Type.Proxy (Proxy(..))\nvalue :: Int\nvalue = reflectType (Proxy :: Proxy \"hi\")\n";
    let inputs = cases::inputs(main);
    let official = purs_sources_output("reflectable-wrong-result", &inputs);
    assert!(
        purs_error_codes(&official).contains(&"TypesDoNotUnify".to_owned()),
        "{official:?}"
    );
    let errors = psrs_driver::check_program(&inputs).expect_err("String cannot reflect to Int");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TypesDoNotUnify")),
        "{errors:?}"
    );
}
