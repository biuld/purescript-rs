use super::*;
use crate::cc::{RefShape, Region, RegionExit, ValueId};

fn protected() -> Function {
    let token = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Exception,
    });
    Function {
        symbol: symbol(0),
        name: "protected".into(),
        parameters: vec![ValueId(0)],
        values: vec![
            ValueDecl {
                id: ValueId(0),
                ty: ValueShape::Integer,
            },
            ValueDecl {
                id: ValueId(1),
                ty: token,
            },
            ValueDecl {
                id: ValueId(2),
                ty: ValueShape::Integer,
            },
        ],
        assignments: vec![Assignment {
            destination: ValueId(2),
            span: TextRange::new(1, 2),
            kind: AssignmentKind::Protect {
                body: Region {
                    assignments: vec![],
                    exit: RegionExit::Yield(ValueId(0)),
                },
                exception: ValueId(1),
                handler: Region {
                    assignments: vec![],
                    exit: RegionExit::Propagate(ValueId(1)),
                },
            },
        }],
        result: ValueId(2),
        result_type: ValueShape::Integer,
        span: TextRange::new(0, 3),
    }
}

#[test]
fn native_protection_permits_a_diverging_handler_without_a_dummy_result() {
    verify_function(&protected(), &HashMap::new(), &table()).unwrap();
}

#[test]
fn exception_binder_cannot_escape_its_handler_or_be_read_in_the_body() {
    let mut f = protected();
    f.result = ValueId(1);
    f.result_type = f.values[1].ty;
    assert!(verify_function(&f, &HashMap::new(), &table()).is_err());
    let mut f = protected();
    let AssignmentKind::Protect { body, .. } = &mut f.assignments[0].kind else {
        unreachable!()
    };
    body.exit = RegionExit::Propagate(ValueId(1));
    assert!(verify_function(&f, &HashMap::new(), &table()).is_err());
}

#[test]
fn handlers_cannot_read_protected_local_definitions_or_yield_a_wrong_shape() {
    let mut f = protected();
    f.values.push(ValueDecl {
        id: ValueId(3),
        ty: ValueShape::Integer,
    });
    let AssignmentKind::Protect { body, handler, .. } = &mut f.assignments[0].kind else {
        unreachable!()
    };
    body.assignments.push(Assignment {
        destination: ValueId(3),
        kind: AssignmentKind::Constant(42),
        span: f.span,
    });
    handler.exit = RegionExit::Yield(ValueId(3));
    assert!(verify_function(&f, &HashMap::new(), &table()).is_err());
    handler_wrong_shape();
}

fn handler_wrong_shape() {
    let mut f = protected();
    let AssignmentKind::Protect { handler, .. } = &mut f.assignments[0].kind else {
        unreachable!()
    };
    handler.exit = RegionExit::Yield(ValueId(1));
    assert!(verify_function(&f, &HashMap::new(), &table()).is_err());
}

#[test]
fn exception_references_cannot_be_cast_into_erased_payloads() {
    let mut f = protected();
    f.parameters.push(ValueId(1));
    f.values[2].ty = crate::cc::payload::erased_shape();
    f.result_type = f.values[2].ty;
    let ValueShape::Reference(reference) = f.values[2].ty else {
        unreachable!()
    };
    f.assignments[0].kind = AssignmentKind::RepresentationCast {
        destination: ValueId(2),
        value: ValueId(1),
        reference,
    };
    assert!(verify_function(&f, &HashMap::new(), &table()).is_err());
}
