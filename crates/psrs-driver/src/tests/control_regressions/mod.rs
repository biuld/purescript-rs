//! Retired recognizer regressions re-expressed at their P8 construction owner.
//! Original cases: 01f7e66c, control_lifecycle_bodies/control_observable_paths.
mod checkpoints;
mod foreign;
mod lifecycle;
mod observables;
mod root;
mod steps;
mod transforms;

const SOURCE: &str = include_str!("../../../tests/control_support/nominal_carriers.purs");

fn binding(
    table: &psrs_backend::cc::RepresentationTable,
    role: psrs_backend::cc::ControlHelperRole,
) -> (psrs_hir::SymbolId, psrs_backend::cc::ControlHelperId) {
    let index = table
        .control_helpers()
        .iter()
        .position(|helper| helper.role() == role)
        .unwrap();
    (
        table.control_helpers()[index].function(),
        psrs_backend::cc::ControlHelperId(index as u32),
    )
}
