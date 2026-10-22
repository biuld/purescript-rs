use super::*;

#[path = "../support/record_instantiation.rs"]
mod cases;

#[test]
fn differential_record_field_instantiation_against_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
    for (name, source) in cases::cases() {
        let official = purs_sources_output(name, &[("Main.purs", &source)]);
        assert!(official.status.success(), "{name}: {official:?}");
        psrs_driver::check_source(name, &source)
            .unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
    }
}
