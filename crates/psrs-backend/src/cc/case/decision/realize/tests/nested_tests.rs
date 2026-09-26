use crate::cc::lower::{FunctionLowerer, GeneratedSymbolAllocator};
use crate::cc::{
    Assignment, AssignmentKind, Function, RefShape, Reference, Representation, RepresentationTable,
    ValueDecl, ValueShape, VariantCase,
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
fn nested_sum_patterns_project_once_per_selected_constructor_and_trap_missing_tags() {
    let module_id = ModuleId(0);
    let outer_type = HirTypeId::new(module_id, 0);
    let bool_type = HirTypeId::new(module_id, 1);
    let pick = SymbolId::new(module_id, 0);
    let skip = SymbolId::new(module_id, 1);
    let true_symbol = SymbolId::new(module_id, 2);
    let false_symbol = SymbolId::new(module_id, 3);
    let module = Module {
        id: module_id,
        name: "NestedDecisionTest".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::User(outer_type)),
            Type::Constructor(TypeConstructor::User(bool_type)),
            Type::I32,
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: vec![
            ConstructorInfo {
                symbol: pick,
                name: "Pick".into(),
                type_id: outer_type,
                tag: 0,
                field_count: 1,
                field_types: vec![psrs_core::TypeId(1)],
            },
            ConstructorInfo {
                symbol: skip,
                name: "Skip".into(),
                type_id: outer_type,
                tag: 1,
                field_count: 1,
                field_types: vec![psrs_core::TypeId(2)],
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
        span: TextRange::new(0, 60),
    };
    let bool_pattern = |symbol, span| Pattern {
        kind: PatternKind::Constructor {
            symbol,
            arguments: Vec::new(),
        },
        ty: psrs_core::TypeId(1),
        span,
    };
    let pick_pattern = |inner, start| Pattern {
        kind: PatternKind::Constructor {
            symbol: pick,
            arguments: vec![inner],
        },
        ty: psrs_core::TypeId(0),
        span: TextRange::new(start, start + 8),
    };
    let skip_local = LocalId(71);
    let branches = vec![
        CaseBranch {
            pattern: pick_pattern(bool_pattern(true_symbol, TextRange::new(6, 10)), 1),
            value: Expr {
                kind: ExprKind::Integer(11),
                ty: psrs_core::TypeId(2),
                span: TextRange::new(11, 13),
            },
            span: TextRange::new(1, 13),
        },
        CaseBranch {
            pattern: pick_pattern(bool_pattern(false_symbol, TextRange::new(21, 26)), 16),
            value: Expr {
                kind: ExprKind::Integer(22),
                ty: psrs_core::TypeId(2),
                span: TextRange::new(27, 29),
            },
            span: TextRange::new(16, 29),
        },
        CaseBranch {
            pattern: Pattern {
                kind: PatternKind::Constructor {
                    symbol: skip,
                    arguments: vec![Pattern {
                        kind: PatternKind::Var {
                            id: skip_local,
                            ty: psrs_core::TypeId(2),
                        },
                        ty: psrs_core::TypeId(2),
                        span: TextRange::new(38, 39),
                    }],
                },
                ty: psrs_core::TypeId(0),
                span: TextRange::new(32, 40),
            },
            value: Expr {
                kind: ExprKind::Local(skip_local),
                ty: psrs_core::TypeId(2),
                span: TextRange::new(41, 42),
            },
            span: TextRange::new(32, 42),
        },
    ];

    let mut representations = RepresentationTable::default();
    let representation = representations.reserve();
    representations.set(
        representation,
        Representation::Variant {
            cases: vec![
                VariantCase {
                    tag: 0,
                    fields: vec![ValueShape::Integer],
                },
                VariantCase {
                    tag: 1,
                    fields: vec![ValueShape::Integer],
                },
            ],
        },
    );
    let signatures = HashMap::new();
    let enum_types = HashSet::from([bool_type]);
    let aggregate_types = HashSet::from([outer_type]);
    let newtype_ids = HashSet::new();
    let array_types = HashMap::new();
    let record_types = HashMap::new();
    let constructor_tags = HashMap::from([(pick, 0), (skip, 1)]);
    let constructors_by_type = HashMap::from([(outer_type, vec![(pick, 0), (skip, 1)])]);
    let constructor_types = HashMap::from([(pick, representation), (skip, representation)]);
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
            TextRange::new(0, 45),
            &mut assignments,
        )
        .expect("production aggregate dispatch should compile and realize nested patterns");
    let function = Function {
        symbol: SymbolId::new(module_id, 8),
        name: "nestedConstructorCase".into(),
        parameters: vec![crate::types::ValueId(0)],
        values: lowerer.values,
        assignments,
        result,
        result_type: ValueShape::Integer,
        span: TextRange::new(0, 45),
    };
    crate::cc::verify::verify_function(&function, &signatures, &representations)
        .expect("nested DAG result should verify as CC");

    let mut assignments = Vec::new();
    flatten(&function.assignments, &mut assignments);
    assert_eq!(
        assignments
            .iter()
            .filter(|assignment| matches!(assignment.kind, AssignmentKind::VariantTag { .. }))
            .count(),
        1
    );
    assert_eq!(
        assignments
            .iter()
            .filter(|assignment| matches!(assignment.kind, AssignmentKind::VariantGet { .. }))
            .count(),
        2
    );
    assert_eq!(
        assignments
            .iter()
            .filter(|assignment| matches!(assignment.kind, AssignmentKind::TagSwitch { .. }))
            .count(),
        1
    );
    assert!(
        assignments
            .iter()
            .any(|assignment| matches!(assignment.kind, AssignmentKind::Unreachable))
    );
}

fn flatten<'a>(assignments: &'a [Assignment], output: &mut Vec<&'a Assignment>) {
    for assignment in assignments {
        output.push(assignment);
        match &assignment.kind {
            AssignmentKind::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                flatten(then_assignments, output);
                flatten(else_assignments, output);
            }
            AssignmentKind::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                for case in cases {
                    flatten(&case.assignments, output);
                }
                flatten(default_assignments, output);
            }
            _ => {}
        }
    }
}
