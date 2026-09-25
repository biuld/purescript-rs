//! Full-function negative fixtures for dictionary products at the CC boundary
//! (DICT-02, DICT-09). A dictionary is an ordinary product, so a projection or
//! construction whose field index or field shape disagrees with the planned
//! representation must be rejected as invalid compiler IR.

use super::super::verify_function;
use super::{symbol, table};
use crate::cc::{
    Assignment, AssignmentKind, Function, RefShape, Reference, ReprId, Representation,
    RepresentationTable, ValueDecl, ValueId, ValueShape,
};
use psrs_span::TextRange;
use std::collections::HashMap;

fn dictionary_product() -> (RepresentationTable, ReprId) {
    let mut representations = table();
    let dictionary = representations.reserve();
    representations.set(
        dictionary,
        Representation::Product {
            fields: vec![
                ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Erased,
                }),
                ValueShape::Boolean,
            ],
        },
    );
    (representations, dictionary)
}

fn erased_reference() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

fn dictionary_reference(dictionary: ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(dictionary),
    })
}

#[test]
fn rejects_a_dictionary_projection_with_an_out_of_range_field() {
    let (representations, dictionary) = dictionary_product();
    let value = ValueId(0);
    let destination = ValueId(1);
    let function = Function {
        symbol: symbol(0),
        name: "out_of_range_dictionary_field".into(),
        parameters: vec![value],
        values: vec![
            ValueDecl {
                id: value,
                ty: dictionary_reference(dictionary),
            },
            ValueDecl {
                id: destination,
                ty: ValueShape::Boolean,
            },
        ],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::ProductGet {
                destination,
                representation: dictionary,
                field: 2,
                value,
            },
            span: TextRange::new(0, 1),
        }],
        result: destination,
        result_type: ValueShape::Boolean,
        span: TextRange::new(0, 1),
    };
    assert!(verify_function(&function, &HashMap::new(), &representations).is_err());
}

#[test]
fn rejects_a_dictionary_construction_with_a_wrong_field_shape() {
    let (representations, dictionary) = dictionary_product();
    let method = ValueId(0);
    let destination = ValueId(1);
    let function = Function {
        symbol: symbol(0),
        name: "wrong_dictionary_field_shape".into(),
        parameters: vec![method],
        values: vec![
            ValueDecl {
                id: method,
                ty: ValueShape::Integer,
            },
            ValueDecl {
                id: destination,
                ty: dictionary_reference(dictionary),
            },
        ],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::ProductNew {
                destination,
                representation: dictionary,
                // The second field stores Boolean, not Integer.
                arguments: vec![method, method],
            },
            span: TextRange::new(0, 1),
        }],
        result: destination,
        result_type: dictionary_reference(dictionary),
        span: TextRange::new(0, 1),
    };
    assert!(verify_function(&function, &HashMap::new(), &representations).is_err());
}

#[test]
fn accepts_a_well_formed_dictionary_product() {
    let (representations, dictionary) = dictionary_product();
    let method = ValueId(0);
    let flag = ValueId(1);
    let constructed = ValueId(2);
    let projected = ValueId(3);
    let function = Function {
        symbol: symbol(0),
        name: "well_formed_dictionary".into(),
        parameters: vec![method, flag],
        values: vec![
            ValueDecl {
                id: method,
                ty: erased_reference(),
            },
            ValueDecl {
                id: flag,
                ty: ValueShape::Boolean,
            },
            ValueDecl {
                id: constructed,
                ty: dictionary_reference(dictionary),
            },
            ValueDecl {
                id: projected,
                ty: erased_reference(),
            },
        ],
        assignments: vec![
            Assignment {
                destination: constructed,
                kind: AssignmentKind::ProductNew {
                    destination: constructed,
                    representation: dictionary,
                    arguments: vec![method, flag],
                },
                span: TextRange::new(0, 1),
            },
            Assignment {
                destination: projected,
                kind: AssignmentKind::ProductGet {
                    destination: projected,
                    representation: dictionary,
                    field: 0,
                    value: constructed,
                },
                span: TextRange::new(0, 1),
            },
        ],
        result: projected,
        result_type: erased_reference(),
        span: TextRange::new(0, 1),
    };
    verify_function(&function, &HashMap::new(), &representations)
        .expect("a well-formed dictionary product verifies");
}
