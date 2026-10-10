use super::*;
use crate::cc::ReprId;

fn boundary(fields: Vec<ValueShape>) -> (Signature, RepresentationTable) {
    let mut table = RepresentationTable::default();
    table
        .representations
        .push(Representation::Product { fields });
    let signature = Signature {
        parameters: vec![ValueShape::Integer, ValueShape::State],
        result: ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(ReprId(0)),
        }),
    };
    (signature, table)
}

#[test]
fn projects_logical_slots_without_assuming_product_field_order() {
    for state_field in 0..2 {
        let mut fields = vec![ValueShape::Number; 2];
        fields[state_field] = ValueShape::State;
        let (signature, table) = boundary(fields);
        let before = table.clone();
        let plan = StateCallProjection::checked(&signature, &table)
            .unwrap()
            .unwrap();
        assert_eq!(plan.state_parameter, 1);
        assert_eq!(plan.state_field, state_field);
        assert_eq!(plan.payload_field, 1 - state_field);
        assert_eq!(
            StateCallProjection::physical_signature(&signature, &table).unwrap(),
            Signature {
                parameters: vec![ValueShape::Integer],
                result: ValueShape::Number,
            }
        );
        assert_eq!(table, before);
    }
}

#[test]
fn nullary_projection_keeps_a_call_boundary() {
    let (mut signature, table) = boundary(vec![ValueShape::State, ValueShape::Integer]);
    signature.parameters.remove(0);
    let projected = StateCallProjection::physical_signature(&signature, &table).unwrap();
    assert!(projected.parameters.is_empty());
    assert_eq!(projected.result, ValueShape::Integer);
    assert!(
        StateCallProjection::checked(&signature, &table)
            .unwrap()
            .is_some()
    );
}

#[test]
fn rejects_malformed_state_boundaries() {
    let (signature, table) = boundary(vec![ValueShape::State, ValueShape::Integer]);
    for parameters in [
        vec![ValueShape::State, ValueShape::Integer],
        vec![ValueShape::State, ValueShape::State],
    ] {
        let mut malformed = signature.clone();
        malformed.parameters = parameters;
        assert!(StateCallProjection::checked(&malformed, &table).is_err());
    }
    for fields in [
        vec![ValueShape::Integer],
        vec![ValueShape::State, ValueShape::State],
        vec![ValueShape::State, ValueShape::Integer, ValueShape::Number],
    ] {
        let (malformed, table) = boundary(fields);
        assert!(StateCallProjection::checked(&malformed, &table).is_err());
    }
    let mut nullable = signature;
    if let ValueShape::Reference(reference) = &mut nullable.result {
        reference.nullable = true;
    }
    assert!(StateCallProjection::checked(&nullable, &table).is_err());
}

#[test]
fn ordinary_signatures_preserve_their_calling_convention() {
    let signature = Signature {
        parameters: vec![ValueShape::Integer],
        result: ValueShape::Number,
    };
    assert_eq!(
        StateCallProjection::physical_signature(&signature, &RepresentationTable::default())
            .unwrap(),
        signature
    );
}

#[test]
fn step_maps_keep_payload_boxing_and_reject_state_conversion() {
    use crate::cc::{AggregateConvert, BoxKind, ValueConversion as C};
    let (signature, mut table) = boundary(vec![ValueShape::State, ValueShape::Integer]);
    let erased = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    });
    table.representations.push(Representation::Product {
        fields: vec![ValueShape::State, erased],
    });
    table.representations.push(Representation::Box {
        value: ValueShape::Integer,
    });
    let payload = C::Sequence(vec![
        C::BoxScalar {
            kind: BoxKind::Integer,
            representation: ReprId(2),
        },
        C::EraseReference,
    ]);
    let mut conversion = AggregateConvert {
        source: signature.result,
        destination: ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(ReprId(1)),
        }),
        plan: C::ProductMap {
            source: ReprId(0),
            target: ReprId(1),
            labels: vec!["state".into(), "value".into()],
            fields: vec![C::Identity, payload.clone()],
        },
    };
    assert_eq!(
        StateCallProjection::payload_conversion(&conversion, &table).unwrap(),
        AggregateConvert {
            source: ValueShape::Integer,
            destination: erased,
            plan: payload
        }
    );
    if let C::ProductMap { fields, .. } = &mut conversion.plan {
        fields[0] = C::EraseReference;
    }
    assert!(StateCallProjection::payload_conversion(&conversion, &table).is_err());
}

#[test]
fn payload_slots_are_registered_once_and_preserve_step_field_order() {
    for state_field in 0..2 {
        let mut fields = vec![ValueShape::Integer; 2];
        fields[state_field] = ValueShape::State;
        let (signature, mut table) = boundary(fields);
        table.add_signature(signature.clone());
        let projection = StateCallProjection::checked(&signature, &table)
            .unwrap()
            .unwrap();
        assert!(
            projection
                .payload_slot_signature(&signature, &table)
                .is_err()
        );
        super::register_payload_slots(&mut table);
        let slot = projection
            .payload_slot_signature(&signature, &table)
            .unwrap();
        let terminal = table.signature(slot).unwrap();
        assert_eq!(terminal.parameters, vec![ValueShape::State]);
        let result = StateCallProjection::checked(terminal, &table)
            .unwrap()
            .unwrap();
        assert_eq!(result.state_field, state_field);
        assert_eq!(result.payload, crate::cc::payload::erased_shape());
        assert_eq!(table.signature(crate::cc::SignatureId(0)), Some(&signature));
        let registered = table.clone();
        super::register_payload_slots(&mut table);
        assert_eq!(table, registered);
    }
}
