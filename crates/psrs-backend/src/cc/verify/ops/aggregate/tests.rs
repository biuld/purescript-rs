use super::*;
use crate::cc::{Assignment, AssignmentKind, ReprId, ValueId, VariantCase};
use psrs_span::TextRange;

fn reference(id: ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(id),
    })
}

fn table() -> RepresentationTable {
    let mut table = RepresentationTable::default();
    let integer_box = table.reserve();
    table.set(
        integer_box,
        Representation::Box {
            value: ValueShape::Integer,
        },
    );
    let array_erased = table.reserve();
    table.set(
        array_erased,
        Representation::Array {
            element: erased_shape(),
        },
    );
    let array_integer = table.reserve();
    table.set(
        array_integer,
        Representation::Array {
            element: ValueShape::Integer,
        },
    );
    let product = table.reserve();
    table.set(
        product,
        Representation::Product {
            fields: vec![ValueShape::Integer, erased_shape()],
        },
    );
    table.set_product_labels(product, vec!["a".into(), "b".into()]);
    let variant = table.reserve();
    table.set(
        variant,
        Representation::Variant {
            cases: vec![VariantCase {
                tag: 0,
                fields: vec![erased_shape()],
            }],
        },
    );
    let not_array = table.reserve();
    table.set(not_array, Representation::Product { fields: Vec::new() });
    table
}

fn assignment() -> Assignment {
    Assignment {
        destination: ValueId(9),
        kind: AssignmentKind::Constant(0),
        span: TextRange::new(0, 1),
    }
}

fn verify(
    plan: ValueConversion,
    source: ValueShape,
    destination: ValueShape,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    verify_conversion(
        &assignment(),
        &AggregateConvert {
            source,
            destination,
            plan,
        },
        source,
        destination,
        table,
    )
}

fn boxed_integer_to_erased() -> ValueConversion {
    ValueConversion::Sequence(vec![
        ValueConversion::BoxScalar {
            kind: BoxKind::Integer,
            representation: ReprId(0),
        },
        ValueConversion::EraseReference,
    ])
}

#[test]
fn accepts_a_valid_array_map() {
    let table = table();
    let plan = ValueConversion::ArrayMap {
        source: ReprId(2),
        target: ReprId(1),
        element: Box::new(boxed_integer_to_erased()),
    };
    assert!(verify(plan, reference(ReprId(2)), reference(ReprId(1)), &table).is_ok());
}

#[test]
fn rejects_endpoints_that_disagree_with_the_typed_values() {
    let table = table();
    let conversion = AggregateConvert {
        source: reference(ReprId(2)),
        destination: reference(ReprId(1)),
        plan: ValueConversion::Identity,
    };
    let result = verify_conversion(
        &assignment(),
        &conversion,
        reference(ReprId(1)),
        reference(ReprId(1)),
        &table,
    );
    assert!(result.is_err());
}

#[test]
fn rejects_an_array_map_whose_source_is_not_an_array() {
    let table = table();
    let plan = ValueConversion::ArrayMap {
        source: ReprId(5),
        target: ReprId(1),
        element: Box::new(ValueConversion::Identity),
    };
    assert!(verify(plan, reference(ReprId(5)), reference(ReprId(1)), &table).is_err());
}

#[test]
fn rejects_an_array_map_whose_target_is_not_an_array() {
    let table = table();
    let plan = ValueConversion::ArrayMap {
        source: ReprId(2),
        target: ReprId(5),
        element: Box::new(ValueConversion::Identity),
    };
    assert!(verify(plan, reference(ReprId(2)), reference(ReprId(5)), &table).is_err());
}

#[test]
fn rejects_an_array_map_with_an_incompatible_element_plan() {
    let table = table();
    let plan = ValueConversion::ArrayMap {
        source: ReprId(2),
        target: ReprId(1),
        element: Box::new(ValueConversion::Identity),
    };
    assert!(verify(plan, reference(ReprId(2)), reference(ReprId(1)), &table).is_err());
}

#[test]
fn rejects_a_product_map_with_the_wrong_labels() {
    let table = table();
    let plan = ValueConversion::ProductMap {
        source: ReprId(3),
        target: ReprId(3),
        labels: vec!["a".into(), "z".into()],
        fields: vec![ValueConversion::Identity, ValueConversion::Identity],
    };
    assert!(verify(plan, reference(ReprId(3)), reference(ReprId(3)), &table).is_err());
}

#[test]
fn rejects_a_product_map_with_the_wrong_arity() {
    let table = table();
    let plan = ValueConversion::ProductMap {
        source: ReprId(3),
        target: ReprId(3),
        labels: vec!["a".into(), "b".into()],
        fields: vec![ValueConversion::Identity],
    };
    assert!(verify(plan, reference(ReprId(3)), reference(ReprId(3)), &table).is_err());
}

#[test]
fn accepts_a_type_instantiation_recovery() {
    let table = table();
    let plan = ValueConversion::RecoverReference {
        destination: reference(ReprId(2)),
        evidence: RecoveryEvidence::TypeInstantiation,
    };
    assert!(verify(plan, erased_shape(), reference(ReprId(2)), &table).is_ok());
}

#[test]
fn rejects_a_variant_field_recovery_with_the_wrong_template() {
    let table = table();
    let plan = ValueConversion::RecoverReference {
        destination: reference(ReprId(2)),
        evidence: RecoveryEvidence::ErasedVariantField {
            variant: ReprId(4),
            tag: 0,
            field: 0,
            template: reference(ReprId(1)),
        },
    };
    assert!(verify(plan, erased_shape(), reference(ReprId(2)), &table).is_err());
}

#[test]
fn rejects_a_variant_field_recovery_with_the_wrong_tag_or_field() {
    let table = table();
    for (tag, field) in [(9u32, 0u32), (0, 7)] {
        let plan = ValueConversion::RecoverReference {
            destination: reference(ReprId(2)),
            evidence: RecoveryEvidence::ErasedVariantField {
                variant: ReprId(4),
                tag,
                field,
                template: reference(ReprId(2)),
            },
        };
        assert!(
            verify(plan, erased_shape(), reference(ReprId(2)), &table).is_err(),
            "tag {tag} field {field} should be rejected"
        );
    }
}

#[test]
fn rejects_a_variant_field_recovery_with_a_non_variant_handle() {
    let table = table();
    let plan = ValueConversion::RecoverReference {
        destination: reference(ReprId(2)),
        evidence: RecoveryEvidence::ErasedVariantField {
            variant: ReprId(5),
            tag: 0,
            field: 0,
            template: reference(ReprId(2)),
        },
    };
    assert!(verify(plan, erased_shape(), reference(ReprId(2)), &table).is_err());
}

#[test]
fn rejects_recovery_from_a_non_erased_source() {
    let table = table();
    let plan = ValueConversion::RecoverReference {
        destination: reference(ReprId(2)),
        evidence: RecoveryEvidence::TypeInstantiation,
    };
    assert!(verify(plan, reference(ReprId(2)), reference(ReprId(2)), &table).is_err());
}

#[test]
fn rejects_a_sequence_with_an_incompatible_step() {
    let table = table();
    let plan = ValueConversion::Sequence(vec![
        ValueConversion::EraseReference,
        ValueConversion::UnboxScalar {
            kind: BoxKind::Number,
            representation: ReprId(0),
            destination: ValueShape::Number,
        },
    ]);
    assert!(verify(plan, reference(ReprId(2)), ValueShape::Number, &table).is_err());
}
