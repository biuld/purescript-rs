//! Rank-N cell transport, in-place polymorphic writes, and ignored-write order.
//! The programs are the stdlib oracle cases. CI has no sibling checkout, so the
//! bytes live in this crate and are compared with that checkout when it exists.

const IDENTITY: &str = include_str!("fixtures/Identity.purs");
const MALFORMED: &str = include_str!("fixtures/Malformed.purs");

#[test]
fn polymorphic_refs_keep_cell_identity_and_write_order() {
    super::assert_matches_stdlib_oracle(IDENTITY, "conformance/native/Identity.purs");
    let Some(output) = super::super::run_program_with_prelude_wasmtime(&[("Main.purs", IDENTITY)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert_eq!(output.stdout, b"before\nafter\n", "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn a_coerced_non_cell_ref_is_rejected_before_codegen() {
    super::assert_matches_stdlib_oracle(MALFORMED, "conformance/native/Malformed.purs");
    let errors = crate::compile_program_sources_with_prelude(&[("Main.purs", MALFORMED)])
        .expect_err("Int and Ref have different runtime shapes");
    assert!(
        errors.iter().any(|error| {
            error.diagnostic.stage == "P8 closure conversion"
                && error.diagnostic.message
                    == "unsupported aggregate conversion between normalized runtime shapes"
        }),
        "{errors:?}"
    );
}
