use super::*;
use crate::cc::{Function, Module as CcModule, Reference, Signature, ValueDecl};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

#[test]
fn planner_owns_the_gc_closure_and_capture_layouts() {
    let mut table = RepresentationTable::default();
    table.add_signature(Signature {
        parameters: vec![CcValueShape::Integer],
        result: CcValueShape::Integer,
    });

    let layout = PlannedLayout::plan(&table, TargetCapabilities::default())
        .expect("planning a closure signature");
    let definitions = &layout.types[0].0;

    assert!(matches!(definitions[0].composite, CompositeType::Array(_)));
    assert!(matches!(definitions[1].composite, CompositeType::Struct(_)));
    assert!(matches!(
        definitions[2].composite,
        CompositeType::Func { .. }
    ));
    assert_eq!(
        layout.closure_layout().unwrap(),
        (DefinedTypeId(1), DefinedTypeId(0))
    );
}
#[test]
fn planner_rejects_a_dangling_closure_signature() {
    let table = RepresentationTable {
        representations: vec![Representation::Product {
            fields: vec![CcValueShape::Reference(Reference {
                nullable: false,
                heap: CcRefShape::Closure(SignatureId(0)),
            })],
        }],
        signatures: Vec::new(),
        product_labels: Default::default(),
    };
    assert!(matches!(
        PlannedLayout::plan(&table, TargetCapabilities::default()),
        Err(LayoutError::UnknownSignature)
    ));
}
#[test]
fn gc_planner_rejects_an_mvp_only_target() {
    let table = RepresentationTable {
        representations: vec![Representation::Product { fields: Vec::new() }],
        signatures: Vec::new(),
        product_labels: Default::default(),
    };

    let mvp_only = TargetCapabilities {
        gc: false,
        ..TargetCapabilities::default()
    };
    assert!(matches!(
        PlannedLayout::plan(&table, mvp_only),
        Err(LayoutError::UnsupportedGcTarget)
    ));
}
#[test]
fn module_planner_omits_unreachable_requirements() {
    let mut table = RepresentationTable::default();
    let reachable = table.reserve();
    table.set(reachable, Representation::Product { fields: Vec::new() });
    let unreachable = table.reserve();
    table.set(unreachable, Representation::Product { fields: Vec::new() });
    let value = crate::cc::ValueId(0);
    let value_shape = CcValueShape::Reference(Reference {
        nullable: false,
        heap: CcRefShape::Repr(reachable),
    });
    let module = CcModule {
        name: "reachable-layout".into(),
        externals: Vec::new(),
        representations: table,
        functions: vec![Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: vec![value],
            values: vec![ValueDecl {
                id: value,
                ty: value_shape,
            }],
            assignments: Vec::new(),
            result: value,
            result_type: value_shape,
            span: TextRange::new(0, 1),
        }],
        entry: None,
        span: TextRange::new(0, 1),
    };

    let layout = PlannedLayout::plan_module(&module, TargetCapabilities::default())
        .expect("planning reachable requirements");
    assert_eq!(layout.types[0].0.len(), 1);
    assert_eq!(layout.repr_index(reachable).unwrap(), DefinedTypeId(0));
    assert!(matches!(
        layout.repr_index(unreachable),
        Err(LayoutError::UnknownRepresentation)
    ));
}
