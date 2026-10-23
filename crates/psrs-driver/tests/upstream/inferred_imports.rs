use super::*;

#[path = "../support/inferred_imports.rs"]
mod cases;

#[test]
fn inferred_import_schemes_match_official_purs() {
    if !purs_available() {
        return;
    }
    let output = purs_sources_with_foreign_output(
        "checked-foreign-interface",
        cases::FOREIGN,
        &[(
            "Provider.js",
            "export const identityForeign = value => value;\n",
        )],
    );
    assert!(output.status.success(), "{output:?}");
    psrs_driver::check_program(cases::FOREIGN).expect("foreign interface");
    let output = purs_sources_output("checked-interface-cycle", cases::CYCLE);
    assert!(
        purs_error_codes(&output)
            .iter()
            .any(|code| code == "CycleInModules"),
        "{output:?}"
    );
    assert!(psrs_driver::check_program(cases::CYCLE).is_err());
    let mut failures = Vec::new();
    for (name, sources) in cases::ACCEPT {
        let output = purs_sources_output(name, sources);
        assert!(output.status.success(), "{name}: {output:?}");
        if let Err(errors) = psrs_driver::check_program(sources) {
            failures.push(format!("{name}: {errors:?}"));
        }
    }
    for (name, sources) in [
        ("private-inferred", cases::PRIVATE),
        ("bad-inferred-producer", cases::BAD_PRODUCER),
    ] {
        let output = purs_sources_output(name, sources);
        assert!(!output.status.success(), "{name}: {output:?}");
        assert!(
            psrs_driver::check_program(sources).is_err(),
            "{name}: accepted"
        );
    }
    let output = purs_sources_output("missing-inferred-instance", cases::MISSING);
    assert!(
        purs_error_codes(&output)
            .iter()
            .any(|code| code == "NoInstanceFound"),
        "{output:?}"
    );
    let errors = psrs_driver::check_program(cases::MISSING).expect_err("missing instance");
    if !errors
        .iter()
        .any(|error| error.diagnostic.code == Some("NoInstanceFound"))
    {
        failures.push(format!("missing-inferred-instance: {errors:?}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
