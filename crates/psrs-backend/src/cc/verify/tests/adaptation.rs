use super::super::verify_function;
use crate::cc::{
    Assignment, AssignmentKind, Function, RefShape, Reference, ReprId, Representation,
    RepresentationTable, ValueDecl, ValueId, ValueShape,
};
use psrs_span::TextRange;
use std::collections::HashMap;

fn boxed_integer_table() -> (RepresentationTable, ReprId) {
    let mut representations = RepresentationTable::default();
    let boxed = representations.reserve();
    representations.set(
        boxed,
        Representation::Box {
            value: ValueShape::Integer,
        },
    );
    (representations, boxed)
}

fn adaptation_function(source: ValueShape, target: Reference) -> Function {
    let value = ValueId(0);
    let destination = ValueId(1);
    Function {
        symbol: super::symbol(0),
        name: "adapt".into(),
        parameters: vec![value],
        values: vec![
            ValueDecl {
                id: value,
                ty: source,
            },
            ValueDecl {
                id: destination,
                ty: ValueShape::Reference(target),
            },
        ],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::RepresentationCast {
                destination,
                value,
                reference: target,
            },
            span: TextRange::new(0, 1),
        }],
        result: destination,
        result_type: ValueShape::Reference(target),
        span: TextRange::new(0, 1),
    }
}

#[test]
fn rejects_a_nullable_erased_source_crossing_to_a_concrete_reference() {
    let (representations, boxed) = boxed_integer_table();
    let source = ValueShape::Reference(Reference {
        nullable: true,
        heap: RefShape::Erased,
    });
    let target = Reference {
        nullable: false,
        heap: RefShape::Repr(boxed),
    };
    let function = adaptation_function(source, target);

    let errors = verify_function(&function, &HashMap::new(), &representations)
        .expect_err("a nullable erased source is not the exact erased requirement");
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P8 CC verification")
    );
}

#[test]
fn rejects_a_nullable_erased_target_crossing_from_a_concrete_reference() {
    let (representations, boxed) = boxed_integer_table();
    let source = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(boxed),
    });
    let target = Reference {
        nullable: true,
        heap: RefShape::Erased,
    };
    let function = adaptation_function(source, target);

    let errors = verify_function(&function, &HashMap::new(), &representations)
        .expect_err("a nullable erased target is not the exact erased requirement");
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P8 CC verification")
    );
}

#[test]
fn accepts_exact_non_null_erased_boundary_casts() {
    let (representations, boxed) = boxed_integer_table();
    for (source, target) in [
        (
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            }),
            Reference {
                nullable: false,
                heap: RefShape::Repr(boxed),
            },
        ),
        (
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(boxed),
            }),
            Reference {
                nullable: false,
                heap: RefShape::Erased,
            },
        ),
    ] {
        let function = adaptation_function(source, target);
        assert!(
            verify_function(&function, &HashMap::new(), &representations).is_ok(),
            "exact non-null erased adaptation must remain valid"
        );
    }
}

#[test]
fn accepts_a_same_heap_nullability_change_for_loop_carried_references() {
    let (representations, _) = boxed_integer_table();
    let function = adaptation_function(
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        }),
        Reference {
            nullable: true,
            heap: RefShape::Erased,
        },
    );

    assert!(verify_function(&function, &HashMap::new(), &representations).is_ok());
}
