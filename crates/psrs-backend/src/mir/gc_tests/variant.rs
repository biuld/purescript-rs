use super::{run_gc, span};
use crate::cc::{
    Assignment, AssignmentKind, Function, Module, RefShape, Reference, Representation,
    RepresentationTable, ValueDecl, ValueShape, VariantCase,
};
use crate::types::ValueId;
use psrs_hir::{ModuleId, SymbolId};

#[test]
fn lowers_different_variant_cases_through_both_planners() {
    let mut representations = RepresentationTable::default();
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
                    fields: vec![ValueShape::Integer, ValueShape::Integer],
                },
            ],
        },
    );
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
            ty: ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Aggregate,
            }),
        },
        ValueDecl {
            id: ValueId(3),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(4),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(5),
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
            kind: AssignmentKind::VariantNew {
                destination: ValueId(2),
                representation: variant,
                case: 1,
                fields: vec![ValueId(0), ValueId(1)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(3),
            kind: AssignmentKind::VariantTag {
                destination: ValueId(3),
                representation: variant,
                value: ValueId(2),
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(4),
            kind: AssignmentKind::VariantGet {
                destination: ValueId(4),
                representation: variant,
                case: 1,
                field: 1,
                value: ValueId(2),
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(5),
            kind: AssignmentKind::Primitive {
                op: crate::cc::BinaryOp::IntAdd,
                left: ValueId(3),
                right: ValueId(4),
            },
            span: span(),
        },
    ];
    let module = Module {
        name: "Variant".into(),
        externals: Vec::new(),
        representations,
        functions: vec![Function {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
            result: ValueId(5),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };
    let (gc, _) = crate::mir::lower_module_with_capabilities(
        module.clone(),
        crate::TargetCapabilities::default(),
    )
    .expect("GC variant lowering");
    run_gc(&gc, 23);
}
