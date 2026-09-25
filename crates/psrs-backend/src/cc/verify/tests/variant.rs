//! Negative variant/tag-switch fixtures for the pattern-matching realizer
//! (PM-07): a malformed projection must name an existing case and field and
//! produce the field's stored shape, and a tag switch must agree with its
//! destination on the selected branch shape.

use super::super::verify_function;
use super::{symbol, table};
use crate::cc::{
    Assignment, AssignmentKind, Function, RefShape, Reference, Representation, TagCase, ValueDecl,
    ValueId, ValueShape, VariantCase,
};
use psrs_span::TextRange;
use std::collections::HashMap;

fn range() -> TextRange {
    TextRange::new(0, 1)
}

fn variant_table() -> (crate::cc::RepresentationTable, crate::cc::ReprId) {
    let mut representations = table();
    let variant = representations.reserve();
    representations.set(
        variant,
        Representation::Variant {
            cases: vec![
                VariantCase {
                    tag: 0,
                    fields: vec![ValueShape::Integer],
                },
                VariantCase {
                    tag: 1,
                    fields: Vec::new(),
                },
            ],
        },
    );
    (representations, variant)
}

fn variant_value(id: ValueId, variant: crate::cc::ReprId) -> ValueDecl {
    ValueDecl {
        id,
        ty: ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(variant),
        }),
    }
}

fn projection_function(
    variant: crate::cc::ReprId,
    case: u32,
    field: u32,
    destination: ValueShape,
) -> Function {
    let value = ValueId(0);
    let result = ValueId(1);
    Function {
        symbol: symbol(0),
        name: "bad_projection".into(),
        parameters: vec![value],
        values: vec![
            variant_value(value, variant),
            ValueDecl {
                id: result,
                ty: destination,
            },
        ],
        assignments: vec![Assignment {
            destination: result,
            kind: AssignmentKind::VariantGet {
                destination: result,
                representation: variant,
                case,
                field,
                value,
            },
            span: range(),
        }],
        result,
        result_type: destination,
        span: range(),
    }
}

#[test]
fn rejects_a_variant_get_with_an_unknown_case() {
    let (representations, variant) = variant_table();
    let function = projection_function(variant, 5, 0, ValueShape::Integer);
    let errors = verify_function(&function, &HashMap::new(), &representations).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("out of range")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_variant_get_with_an_unknown_field() {
    let (representations, variant) = variant_table();
    let function = projection_function(variant, 0, 3, ValueShape::Integer);
    let errors = verify_function(&function, &HashMap::new(), &representations).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("out of range")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_variant_get_with_the_wrong_result_shape() {
    let (representations, variant) = variant_table();
    let function = projection_function(variant, 0, 0, ValueShape::Number);
    let errors = verify_function(&function, &HashMap::new(), &representations).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("incompatible result shape")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_tag_switch_case_with_the_wrong_branch_shape() {
    let selector = ValueId(0);
    let default_value = ValueId(1);
    let result = ValueId(2);
    let case_value = ValueId(3);
    let function = Function {
        symbol: symbol(0),
        name: "bad_tag_switch".into(),
        parameters: vec![selector, case_value],
        values: vec![
            ValueDecl {
                id: selector,
                ty: ValueShape::Integer,
            },
            ValueDecl {
                id: case_value,
                ty: ValueShape::Number,
            },
            ValueDecl {
                id: default_value,
                ty: ValueShape::Integer,
            },
            ValueDecl {
                id: result,
                ty: ValueShape::Integer,
            },
        ],
        assignments: vec![
            Assignment {
                destination: default_value,
                kind: AssignmentKind::Constant(0),
                span: range(),
            },
            Assignment {
                destination: result,
                kind: AssignmentKind::TagSwitch {
                    value: selector,
                    cases: vec![TagCase {
                        tag: 0,
                        assignments: Vec::new(),
                        value: case_value,
                    }],
                    default_assignments: Vec::new(),
                    default_value,
                },
                span: range(),
            },
        ],
        result,
        result_type: ValueShape::Integer,
        span: range(),
    };
    let errors = verify_function(&function, &HashMap::new(), &table()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("incompatible result shapes")),
        "{errors:?}"
    );
}
