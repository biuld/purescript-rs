use crate::cc::lower::{FunctionLowerer, GeneratedSymbolAllocator};
use crate::cc::{
    Function, RefShape, Reference, Representation, RepresentationTable, ValueDecl, ValueShape,
    VariantCase,
};
use psrs_core::{
    CaseBranch, ConstructorInfo, Expr, ExprKind, Module, Pattern, PatternKind, Type,
    TypeConstructor,
};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[test]
fn single_constructor_product_dispatch_projects_and_binds_first_row_once() {
    let module_id = ModuleId(0);
    let type_id = HirTypeId::new(module_id, 0);
    let constructor = SymbolId::new(module_id, 0);
    let module = Module {
        id: module_id,
        name: "ProductDecisionTest".into(),
        externals: Vec::new(),
        types: vec![Type::Constructor(TypeConstructor::User(type_id)), Type::I32],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: vec![ConstructorInfo {
            symbol: constructor,
            name: "Product".into(),
            type_id,
            tag: 0,
            field_count: 1,
            field_types: vec![psrs_core::TypeId(1)],
        }],
        declarations: Vec::new(),
        entry: None,
        span: TextRange::new(0, 40),
    };
    let pattern = |local, start| Pattern {
        kind: PatternKind::Constructor {
            symbol: constructor,
            arguments: vec![Pattern {
                kind: PatternKind::Var {
                    id: local,
                    ty: psrs_core::TypeId(1),
                },
                ty: psrs_core::TypeId(1),
                span: TextRange::new(start + 2, start + 3),
            }],
        },
        ty: psrs_core::TypeId(0),
        span: TextRange::new(start, start + 5),
    };
    let first_local = LocalId(42);
    let redundant_local = LocalId(43);
    let branches = vec![
        CaseBranch {
            pattern: pattern(first_local, 1),
            value: Expr {
                kind: ExprKind::Local(first_local),
                ty: psrs_core::TypeId(1),
                span: TextRange::new(6, 7),
            },
            span: TextRange::new(1, 8),
        },
        CaseBranch {
            pattern: pattern(redundant_local, 10),
            value: Expr {
                kind: ExprKind::Local(redundant_local),
                ty: psrs_core::TypeId(1),
                span: TextRange::new(15, 16),
            },
            span: TextRange::new(10, 17),
        },
    ];

    let mut representations = RepresentationTable::default();
    let representation = representations.reserve();
    representations.set(
        representation,
        Representation::Variant {
            cases: vec![VariantCase {
                tag: 0,
                fields: vec![ValueShape::Integer],
            }],
        },
    );
    let signatures = HashMap::new();
    let enum_types = HashSet::new();
    let aggregate_types = HashSet::from([type_id]);
    let newtype_ids = HashSet::new();
    let array_types = HashMap::new();
    let record_types = HashMap::new();
    let constructor_tags = HashMap::from([(constructor, 0)]);
    let constructors_by_type = HashMap::from([(type_id, vec![(constructor, 0)])]);
    let constructor_types = HashMap::from([(constructor, representation)]);
    let function_types = HashMap::new();
    let function_wrappers = HashMap::new();
    let generated_symbols = Rc::new(RefCell::new(GeneratedSymbolAllocator::new(&module)));
    let root_shape = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(representation),
    });
    let mut lowerer = FunctionLowerer {
        next_value: 1,
        values: vec![ValueDecl {
            id: crate::types::ValueId(0),
            ty: root_shape,
        }],
        locals: HashMap::new(),
        signatures: &signatures,
        representations: &representations,
        module: &module,
        enum_types: &enum_types,
        aggregate_types: &aggregate_types,
        newtype_ids: &newtype_ids,
        boxed_integer_type: None,
        boxed_number_type: None,
        array_types: &array_types,
        record_types: &record_types,
        constructor_tags: &constructor_tags,
        constructors_by_type: &constructors_by_type,
        constructor_types: &constructor_types,
        function_types: &function_types,
        function_wrappers: &function_wrappers,
        generated_symbols,
        owner: module_id,
        warnings: Vec::new(),
        erased_function_types: HashMap::new(),
        generated: Vec::new(),
    };
    let mut assignments = Vec::new();
    let result = lowerer
        .lower_case(
            psrs_core::TypeId(0),
            crate::types::ValueId(0),
            &branches,
            ValueShape::Integer,
            TextRange::new(0, 20),
            &mut assignments,
        )
        .expect("single-constructor product should lower through the DAG");

    let projections = assignments
        .iter()
        .filter(|assignment| {
            matches!(
                assignment.kind,
                crate::cc::AssignmentKind::VariantGet { .. }
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(projections.len(), 1);
    assert_eq!(projections[0].span, branches[0].pattern.span);
    assert_eq!(projections[0].destination, result);
    assert_eq!(lowerer.warnings.len(), 1);
    assert_eq!(lowerer.warnings[0].span, branches[1].span);

    let function = Function {
        symbol: SymbolId::new(module_id, 1),
        name: "projectFirstProductField".into(),
        parameters: vec![crate::types::ValueId(0)],
        values: lowerer.values,
        assignments,
        result,
        result_type: ValueShape::Integer,
        span: TextRange::new(0, 20),
    };
    crate::cc::verify::verify_function(&function, &signatures, &representations)
        .expect("product DAG result should verify as CC");
}
