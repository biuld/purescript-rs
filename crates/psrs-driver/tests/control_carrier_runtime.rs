//! Nominal carriers compose with ordinary polymorphic and aggregate storage.
mod control_support;

#[test]
fn control_and_owner_cross_generic_storage_without_changing_identity() {
    control_support::execute(
        "nominal-carriers",
        include_str!("control_support/nominal_carriers.purs"),
        42,
    );
}
