use super::*;
use crate::cc::{Representation, RepresentationTable};
use psrs_span::TextRange;

#[test]
fn state_constructor_protocol_preserves_parameters_and_successor() {
    let mut table = RepresentationTable::default();
    let step = table.reserve();
    table.set(
        step,
        Representation::Product {
            fields: vec![ValueShape::State, ValueShape::Integer],
        },
    );
    table.set_product_labels(step, vec!["state".into(), "value".into()]);
    let parameters = vec![ValueShape::Integer, ValueShape::State];
    let source = table.add_signature(Signature {
        parameters: parameters.clone(),
        result: ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(step),
        }),
    });
    crate::cc::state::register_payload_slots(&mut table);
    let protocols = payload_erased_protocols(
        &mut table,
        &HashMap::from([(TypeId(0), source)]),
        TextRange::default(),
    )
    .unwrap();
    let signature = table.signature(protocols[&source]).unwrap();
    assert_eq!(signature.parameters, parameters);
    let ValueShape::Reference(Reference {
        heap: RefShape::Repr(result),
        ..
    }) = signature.result
    else {
        panic!("Step protocol must retain a product");
    };
    assert_eq!(table.product_labels(result).unwrap(), ["state", "value"]);
    assert_eq!(
        table.representation(result),
        Some(&Representation::Product {
            fields: vec![ValueShape::State, crate::cc::payload::erased_shape()],
        })
    );
}

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

#[test]
fn callable_protocol_rejects_state_without_a_step_result() {
    let mut table = RepresentationTable::default();
    let source = table.add_signature(Signature {
        parameters: vec![ValueShape::State],
        result: ValueShape::Integer,
    });
    let errors = payload_erased_protocols(
        &mut table,
        &HashMap::from([(TypeId(0), source)]),
        TextRange::default(),
    )
    .unwrap_err();
    assert!(errors[0].message.contains("Step product"));
}
