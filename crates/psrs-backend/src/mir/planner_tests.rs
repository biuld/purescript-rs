use super::planner::{GcPlanner, LinearMemoryPlanner, LinearRepresentation, RepresentationPlanner};
use crate::TargetCapabilities;
use crate::cc::{
    self, Function, Module as CcModule, Reference, ReprId, Representation, Signature, SignatureId,
    ValueDecl, ValueShape,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn module() -> CcModule {
    let mut representations = cc::RepresentationTable::default();
    let product = representations.reserve();
    representations.set(
        product,
        Representation::Product {
            fields: vec![ValueShape::Integer, ValueShape::Number],
        },
    );
    let array = representations.reserve();
    representations.set(
        array,
        Representation::Array {
            element: ValueShape::Number,
        },
    );
    let signature = representations.add_signature(Signature {
        parameters: vec![ValueShape::Reference(Reference {
            nullable: false,
            heap: cc::RefShape::Repr(product),
        })],
        result: ValueShape::Integer,
    });
    let closure_value = cc::ValueId(0);
    let array_value = cc::ValueId(1);
    CcModule {
        name: "planner".into(),
        externals: Vec::new(),
        representations,
        functions: vec![Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "entry".into(),
            parameters: vec![closure_value, array_value],
            values: vec![
                ValueDecl {
                    id: closure_value,
                    ty: ValueShape::Reference(Reference {
                        nullable: false,
                        heap: cc::RefShape::Closure(signature),
                    }),
                },
                ValueDecl {
                    id: array_value,
                    ty: ValueShape::Reference(Reference {
                        nullable: false,
                        heap: cc::RefShape::Repr(array),
                    }),
                },
            ],
            assignments: Vec::new(),
            result: closure_value,
            result_type: ValueShape::Reference(Reference {
                nullable: false,
                heap: cc::RefShape::Closure(signature),
            }),
            span: TextRange::new(0, 1),
        }],
        entry: None,
        span: TextRange::new(0, 1),
    }
}

#[test]
fn gc_and_linear_planners_consume_the_same_cc_requirements() {
    let module = module();
    let gc = GcPlanner {
        target: TargetCapabilities::default(),
    }
    .plan_module(&module)
    .expect("GC planner should accept the fixture");
    let linear = LinearMemoryPlanner
        .plan_module(&module)
        .expect("linear planner should accept the same fixture");
    assert_eq!(
        gc.repr_index(ReprId(0)).unwrap(),
        crate::types::DefinedTypeId(0)
    );
    assert!(linear.representations.contains_key(&ReprId(0)));
    assert!(linear.representations.contains_key(&ReprId(1)));
    assert_eq!(linear.signatures[&SignatureId(0)].table_slot, 0);
    assert!(linear.next_offset >= 12);
    assert_eq!(linear.array(ReprId(1)).unwrap().2, 8);
    assert_eq!(linear.array(ReprId(1)).unwrap().3, 8);
}

#[test]
fn linear_planner_assigns_aligned_product_fields() {
    let layout = LinearMemoryPlanner
        .plan_module(&module())
        .expect("linear planner should plan fields");
    let LinearRepresentation::Product { fields, size, .. } = &layout.representations[&ReprId(0)]
    else {
        panic!("expected product layout");
    };
    assert_eq!(fields[0].offset, 0);
    assert_eq!(fields[1].offset, 8);
    assert_eq!(*size, 16);
}
