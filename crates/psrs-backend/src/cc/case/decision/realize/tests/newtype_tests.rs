use crate::cc::lower::{FunctionLowerer, GeneratedSymbolAllocator};
use crate::cc::{AssignmentKind, Function, RepresentationTable, ValueDecl, ValueShape};
use psrs_core::{
    CaseBranch, ConstructorInfo, Expr, ExprKind, Module, Pattern, PatternKind, Type,
    TypeConstructor,
};
use psrs_hir::{ModuleId, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[test]
fn newtype_constructor_erases_before_nested_enum_dispatch() {
    let module_id = ModuleId(0);
    let wrapper_type = HirTypeId::new(module_id, 0);
    let bool_type = HirTypeId::new(module_id, 1);
    let wrapper = SymbolId::new(module_id, 0);
    let true_symbol = SymbolId::new(module_id, 1);
    let false_symbol = SymbolId::new(module_id, 2);
    let module = Module {
        id: module_id,
        name: "NewtypeDecisionTest".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::User(wrapper_type)),
            Type::Constructor(TypeConstructor::User(bool_type)),
            Type::I32,
        ],
        newtype_ids: vec![wrapper_type],
        opaque_ids: Vec::new(),
        constructors: vec![
            ConstructorInfo {
                symbol: wrapper,
                name: "Wrapped".into(),
                type_id: wrapper_type,
                tag: 0,
                field_count: 1,
                field_types: vec![psrs_core::TypeId(1)],
            },
            ConstructorInfo {
                symbol: true_symbol,
                name: "True".into(),
                type_id: bool_type,
                tag: 0,
                field_count: 0,
                field_types: Vec::new(),
            },
            ConstructorInfo {
                symbol: false_symbol,
                name: "False".into(),
                type_id: bool_type,
                tag: 1,
                field_count: 0,
                field_types: Vec::new(),
            },
        ],
        declarations: Vec::new(),
        entry: None,
        span: TextRange::new(0, 40),
    };
    let wrapped_bool = |symbol, start| Pattern {
        kind: PatternKind::Constructor {
            symbol: wrapper,
            arguments: vec![Pattern {
                kind: PatternKind::Constructor {
                    symbol,
                    arguments: Vec::new(),
                },
                ty: psrs_core::TypeId(1),
                span: TextRange::new(start + 2, start + 5),
            }],
        },
        ty: psrs_core::TypeId(0),
        span: TextRange::new(start, start + 6),
    };
    let branches = vec![
        CaseBranch {
            pattern: wrapped_bool(true_symbol, 1),
            value: Expr {
                kind: ExprKind::Integer(1),
                ty: psrs_core::TypeId(2),
                span: TextRange::new(7, 8),
            },
            span: TextRange::new(1, 8),
        },
        CaseBranch {
            pattern: wrapped_bool(false_symbol, 10),
            value: Expr {
                kind: ExprKind::Integer(0),
                ty: psrs_core::TypeId(2),
                span: TextRange::new(16, 17),
            },
            span: TextRange::new(10, 17),
        },
    ];
    let signatures = HashMap::new();
    let representations = RepresentationTable::default();
    let enum_types = HashSet::from([bool_type]);
    let aggregate_types = HashSet::new();
    let newtype_ids = HashSet::from([wrapper_type]);
    let array_types = HashMap::new();
    let record_types = HashMap::new();
    let constructor_tags = HashMap::from([(wrapper, 0), (true_symbol, 0), (false_symbol, 1)]);
    let constructors_by_type = HashMap::new();
    let constructor_types = HashMap::new();
    let function_types = HashMap::new();
    let function_wrappers = HashMap::new();
    let generated_symbols = Rc::new(RefCell::new(GeneratedSymbolAllocator::new(&module)));
    let mut lowerer = FunctionLowerer {
        next_value: 1,
        values: vec![ValueDecl {
            id: crate::types::ValueId(0),
            ty: ValueShape::Integer,
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
        .expect("newtype pattern should erase and dispatch on its field");
    let function = Function {
        symbol: SymbolId::new(module_id, 5),
        name: "wrappedBoolCase".into(),
        parameters: vec![crate::types::ValueId(0)],
        values: lowerer.values,
        assignments,
        result,
        result_type: ValueShape::Integer,
        span: TextRange::new(0, 20),
    };
    crate::cc::verify::verify_function(&function, &signatures, &representations)
        .expect("newtype DAG result should verify as CC");

    assert!(
        function
            .assignments
            .iter()
            .any(|assignment| matches!(assignment.kind, AssignmentKind::TagSwitch { .. }))
    );
    assert!(!function.assignments.iter().any(|assignment| matches!(
        assignment.kind,
        AssignmentKind::ProductGet { .. } | AssignmentKind::VariantGet { .. }
    )));
}
