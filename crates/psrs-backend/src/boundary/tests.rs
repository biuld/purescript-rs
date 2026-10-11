use super::*;
use crate::cc::RepresentationTable;
use psrs_span::TextRange;

#[test]
fn callable_protocol_rejects_missing_owner_signature() {
    let errors = payload_erased_protocols(
        &mut RepresentationTable::default(),
        &HashMap::from([(TypeId(0), SignatureId(99))]),
        TextRange::default(),
    )
    .unwrap_err();
    assert!(errors[0].message.contains("no registered signature"));
}
