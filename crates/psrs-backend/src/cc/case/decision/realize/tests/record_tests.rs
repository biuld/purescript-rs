use crate::cc::lower::{FunctionLowerer, GeneratedSymbolAllocator};
use crate::cc::{
    AssignmentKind, Function, RefShape, Reference, Representation, RepresentationTable, ValueDecl,
    ValueShape,
};
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
fn nested_record_patterns_share_one_product_projection_and_keep_source_spans() {
    let module_id = ModuleId(0);
    let bool_type = HirTypeId::new(module_id, 0);
    let true_symbol = SymbolId::new(module_id, 0);
    let false_symbol = SymbolId::new(module_id, 1);
    let module = Module {
        id: module_id,
        name: "RecordDecisionTest".into(),
        externals: Vec::new(),
        types: vec![
            Type::Record(vec![
                ("z".into(), psrs_core::TypeId(2)),
                ("active".into(), psrs_core::TypeId(1)),
            ]),
            Type::Constructor(TypeConstructor::User(bool_type)),
            Type::I32,
        ],
        newtype_ids: Vec::new(),
        constructors: vec![
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
    let record_pattern = |symbol, start| Pattern {
        kind: PatternKind::Record {
            fields: vec![(
                "active".into(),
                Pattern {
                    kind: PatternKind::Constructor {
                        symbol,
                        arguments: Vec::new(),
                    },
                    ty: psrs_core::TypeId(1),
                    span: TextRange::new(start + 2, start + 5),
                },
            )],
        },
        ty: psrs_core::TypeId(0),
        span: TextRange::new(start, start + 7),
    };
    let branches = vec![
        CaseBranch {
            pattern: record_pattern(true_symbol, 1),
            value: Expr {
                kind: ExprKind::Integer(1),
                ty: psrs_core::TypeId(2),
                span: TextRange::new(9, 10),
            },
            span: TextRange::new(1, 10),
        },
        CaseBranch {
            pattern: record_pattern(false_symbol, 12),
            value: Expr {
                kind: ExprKind::Integer(0),
                ty: psrs_core::TypeId(2),
                span: TextRange::new(20, 21),
            },
            span: TextRange::new(12, 21),
        },
    ];

    let mut representations = RepresentationTable::default();
    let record_repr = representations.reserve();
    representations.set(
        record_repr,
        Representation::Product {
            fields: vec![ValueShape::Integer, ValueShape::Integer],
        },
    );
    representations.set_product_labels(record_repr, vec!["active".into(), "z".into()]);
    let signatures = HashMap::new();
    let enum_types = HashSet::from([bool_type]);
    let aggregate_types = HashSet::new();
    let newtype_ids = HashSet::new();
    let array_types = HashMap::new();
    let record_types = HashMap::from([(psrs_core::TypeId(0), record_repr)]);
    let constructor_tags = HashMap::from([(true_symbol, 0), (false_symbol, 1)]);
    let constructors_by_type = HashMap::new();
    let constructor_types = HashMap::new();
    let function_types = HashMap::new();
    let function_wrappers = HashMap::new();
    let generated_symbols = Rc::new(RefCell::new(GeneratedSymbolAllocator::new(&module)));
    let root_shape = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(record_repr),
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
            TextRange::new(0, 24),
            &mut assignments,
        )
        .expect("record case should lower through the pattern DAG");
    let function = Function {
        symbol: SymbolId::new(module_id, 4),
        name: "nestedRecordCase".into(),
        parameters: vec![crate::types::ValueId(0)],
        values: lowerer.values,
        assignments,
        result,
        result_type: ValueShape::Integer,
        span: TextRange::new(0, 24),
    };
    crate::cc::verify::verify_function(&function, &signatures, &representations)
        .expect("record DAG result should verify as CC");

    assert_eq!(
        function
            .assignments
            .iter()
            .filter(|assignment| matches!(assignment.kind, AssignmentKind::ProductGet { .. }))
            .count(),
        1
    );
    assert!(
        function.assignments.iter().any(|assignment| matches!(
            assignment.kind,
            AssignmentKind::ProductGet { field: 0, .. }
        ))
    );
    assert_eq!(function.assignments[0].span, branches[0].pattern.span);
    assert!(
        function
            .assignments
            .iter()
            .any(|assignment| matches!(assignment.kind, AssignmentKind::TagSwitch { .. }))
    );
}
