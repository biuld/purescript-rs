//! Record subsumption labels.
//!
//! `purs` reports `PropertyIsMissing` when a closed record lacks a label the
//! expected row requires, and `AdditionalProperty` when a record has a label
//! a closed expected row does not allow. These tests drive the lenient type
//! check, with the standard library, on the corpus files.

use super::*;

fn codes(source: &str) -> Vec<&'static str> {
    check_program_types_lenient_with_prelude(&[("Main.purs", source)])
        .err()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|error| error.diagnostic.code)
        .collect()
}

#[test]
fn an_empty_record_lacks_a_required_label() {
    let source = include_str!("../../../../../tests/upstream/failing/Object.purs");
    let codes = codes(source);
    assert!(codes.contains(&"PropertyIsMissing"), "{codes:?}");
    assert!(!codes.contains(&"TypesDoNotUnify"), "{codes:?}");
}

#[test]
fn a_closed_record_lacks_a_required_label() {
    let source = include_str!("../../../../../tests/upstream/failing/MissingRecordField.purs");
    let codes = codes(source);
    assert!(codes.contains(&"PropertyIsMissing"), "{codes:?}");
    assert!(!codes.contains(&"TypesDoNotUnify"), "{codes:?}");
}

#[test]
fn a_closed_record_rejects_an_additional_label() {
    let source = include_str!("../../../../../tests/upstream/failing/ExtraRecordField.purs");
    let codes = codes(source);
    assert!(codes.contains(&"AdditionalProperty"), "{codes:?}");
    assert!(!codes.contains(&"TypesDoNotUnify"), "{codes:?}");
}

#[test]
fn a_duplicate_row_label_is_not_an_additional_property() {
    let source = include_str!("../../../../../tests/upstream/failing/3701.purs");
    let codes = codes(source);
    assert!(codes.contains(&"TypesDoNotUnify"), "{codes:?}");
    assert!(!codes.contains(&"AdditionalProperty"), "{codes:?}");
}
