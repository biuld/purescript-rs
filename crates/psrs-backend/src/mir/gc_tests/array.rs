use super::{run_gc, span};
use crate::cc::{
    Assignment, AssignmentKind, BinaryOp, Function, Module, RefShape, Reference, Representation,
    RepresentationTable, ValueDecl, ValueShape,
};
use crate::types::ValueId;
use psrs_hir::{ModuleId, SymbolId};

/// A pure array update clones the source, writes the clone, and leaves the
/// original observable. The expected exit code sums the updated clone element,
/// the preserved source element, and the clone length.
#[test]
fn executes_a_pure_array_clone_and_update() {
    let mut representations = RepresentationTable::default();
    let array = representations.reserve();
    representations.set(
        array,
        Representation::Array {
            element: ValueShape::Integer,
        },
    );
    let array_shape = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(array),
    });
    let symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(1),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(2),
            ty: array_shape,
        },
        ValueDecl {
            id: ValueId(3),
            ty: array_shape,
        },
        ValueDecl {
            id: ValueId(4),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(5),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(6),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(7),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(8),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(9),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(10),
            ty: ValueShape::Integer,
        },
    ];
    let assignments = vec![
        Assignment {
            destination: ValueId(0),
            kind: AssignmentKind::Constant(20),
            span: span(),
        },
        Assignment {
            destination: ValueId(1),
            kind: AssignmentKind::Constant(22),
            span: span(),
        },
        Assignment {
            destination: ValueId(2),
            kind: AssignmentKind::ArrayNew {
                destination: ValueId(2),
                representation: array,
                elements: vec![ValueId(0), ValueId(1)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(3),
            kind: AssignmentKind::ArrayClone {
                destination: ValueId(3),
                representation: array,
                value: ValueId(2),
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(4),
            kind: AssignmentKind::Constant(0),
            span: span(),
        },
        Assignment {
            destination: ValueId(5),
            kind: AssignmentKind::Constant(42),
            span: span(),
        },
        Assignment {
            destination: ValueId(3),
            kind: AssignmentKind::ArraySet {
                destination: ValueId(3),
                representation: array,
                value: ValueId(3),
                index: ValueId(4),
                new_value: ValueId(5),
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(6),
            kind: AssignmentKind::ArrayGet {
                destination: ValueId(6),
                representation: array,
                value: ValueId(3),
                index: ValueId(4),
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(7),
            kind: AssignmentKind::ArrayGet {
                destination: ValueId(7),
                representation: array,
                value: ValueId(2),
                index: ValueId(4),
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(8),
            kind: AssignmentKind::ArrayLen {
                destination: ValueId(8),
                value: ValueId(3),
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(9),
            kind: AssignmentKind::Primitive {
                op: BinaryOp::IntAdd,
                left: ValueId(6),
                right: ValueId(8),
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(10),
            kind: AssignmentKind::Primitive {
                op: BinaryOp::IntAdd,
                left: ValueId(9),
                right: ValueId(7),
            },
            span: span(),
        },
    ];
    let module = Module {
        name: "ArrayClone".into(),
        externals: Vec::new(),
        representations,
        functions: vec![Function {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
            result: ValueId(10),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };
    let (mir, _) =
        crate::mir::lower_module_with_capabilities(module, crate::TargetCapabilities::default())
            .expect("GC array lowering");
    run_gc(&mir, 64);
}
