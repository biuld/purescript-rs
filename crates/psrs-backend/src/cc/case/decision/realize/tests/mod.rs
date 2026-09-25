use super::*;
use crate::cc::lower::{FunctionLowerer, GeneratedSymbolAllocator};
use crate::cc::{
    Assignment, AssignmentKind, Function, Module as CcModule, RepresentationTable, ValueDecl,
};
use psrs_core::{
    ConstructorInfo, Expr, ExprKind, Module, Pattern, PatternKind, Type, TypeConstructor,
};
use psrs_hir::{ModuleId, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

mod nested_tests;
mod newtype_tests;
mod parameterized_tests;
mod product_tests;
mod record_tests;

#[test]
fn compiled_root_switch_resolves_the_realizer_root_slot() {
    let module_id = ModuleId(0);
    let type_id = HirTypeId::new(module_id, 0);
    let true_symbol = SymbolId::new(module_id, 0);
    let false_symbol = SymbolId::new(module_id, 1);
    let module = Module {
        id: module_id,
        name: "DecisionRealizeTest".into(),
        externals: Vec::new(),
        types: vec![Type::Constructor(TypeConstructor::User(type_id))],
        newtype_ids: Vec::new(),
        constructors: vec![
            ConstructorInfo {
                symbol: true_symbol,
                name: "True".into(),
                type_id,
                tag: 0,
                field_count: 0,
                field_types: Vec::new(),
            },
            ConstructorInfo {
                symbol: false_symbol,
                name: "False".into(),
                type_id,
                tag: 1,
                field_count: 0,
                field_types: Vec::new(),
            },
        ],
        declarations: Vec::new(),
        entry: None,
        span: TextRange::new(0, 30),
    };
    let pattern = Pattern {
        kind: PatternKind::Constructor {
            symbol: true_symbol,
            arguments: Vec::new(),
        },
        ty: psrs_core::TypeId(0),
        span: TextRange::new(1, 5),
    };
    let branches = vec![
        CaseBranch {
            pattern,
            value: Expr {
                kind: ExprKind::Integer(7),
                ty: psrs_core::TypeId(0),
                span: TextRange::new(6, 7),
            },
            span: TextRange::new(1, 7),
        },
        CaseBranch {
            pattern: Pattern {
                kind: PatternKind::Constructor {
                    symbol: true_symbol,
                    arguments: Vec::new(),
                },
                ty: psrs_core::TypeId(0),
                span: TextRange::new(8, 12),
            },
            value: Expr {
                kind: ExprKind::Integer(8),
                ty: psrs_core::TypeId(0),
                span: TextRange::new(13, 14),
            },
            span: TextRange::new(8, 14),
        },
        CaseBranch {
            pattern: Pattern {
                kind: PatternKind::Constructor {
                    symbol: false_symbol,
                    arguments: Vec::new(),
                },
                ty: psrs_core::TypeId(0),
                span: TextRange::new(15, 20),
            },
            value: Expr {
                kind: ExprKind::Integer(9),
                ty: psrs_core::TypeId(0),
                span: TextRange::new(21, 22),
            },
            span: TextRange::new(15, 22),
        },
    ];

    let signatures = HashMap::new();
    let representations = RepresentationTable::default();
    let enum_types = HashSet::from([type_id]);
    let aggregate_types = HashSet::new();
    let newtype_ids = HashSet::new();
    let array_types = HashMap::new();
    let record_types = HashMap::new();
    let constructor_tags = HashMap::from([(true_symbol, 0), (false_symbol, 1)]);
    let constructors_by_type = HashMap::new();
    let constructor_types = HashMap::new();
    let function_types = HashMap::new();
    let function_wrappers = HashMap::new();
    let generated_symbols = Rc::new(RefCell::new(GeneratedSymbolAllocator::new(&module)));
    let mut lowerer = FunctionLowerer {
        next_value: 1,
        values: vec![ValueDecl {
            id: ValueId(0),
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
    let mut assignments = vec![Assignment {
        destination: ValueId(0),
        kind: AssignmentKind::Constant(2),
        span: TextRange::new(0, 1),
    }];

    let result = lowerer
        .lower_case(
            psrs_core::TypeId(0),
            ValueId(0),
            &branches,
            ValueShape::Integer,
            TextRange::new(0, 22),
            &mut assignments,
        )
        .expect("production enum dispatch should compile and realize the root switch");

    assert_eq!(
        assignments.last().map(|assignment| assignment.destination),
        Some(result)
    );
    assert!(
        assignments
            .iter()
            .any(|assignment| matches!(assignment.kind, AssignmentKind::TagSwitch { .. }))
    );
    assert_eq!(lowerer.warnings.len(), 1);
    assert_eq!(lowerer.warnings[0].span, branches[1].span);
    let AssignmentKind::TagSwitch { cases, .. } = &assignments[1].kind else {
        panic!("nullary pattern matrix should realize as one tag switch");
    };
    assert_eq!(
        cases.iter().map(|case| case.tag).collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(matches!(
        cases[0].assignments[0].kind,
        AssignmentKind::Constant(7)
    ));
    assert!(matches!(
        cases[1].assignments[0].kind,
        AssignmentKind::Constant(9)
    ));
    let has_trapping_default = assignments.iter().any(|assignment| {
        let AssignmentKind::TagSwitch {
            default_assignments,
            ..
        } = &assignment.kind
        else {
            return false;
        };
        default_assignments
            .iter()
            .any(|item| matches!(item.kind, AssignmentKind::Unreachable))
    });
    assert!(has_trapping_default, "missing constructor tags must trap");

    let function = Function {
        symbol: SymbolId::new(module_id, 10),
        name: "malformedTagTraps".into(),
        parameters: Vec::new(),
        values: lowerer.values,
        assignments,
        result,
        result_type: ValueShape::Integer,
        span: TextRange::new(0, 8),
    };
    crate::cc::verify::verify_function(&function, &signatures, &representations)
        .expect("realized decision should verify as CC");
    let cc_module = CcModule {
        name: "DecisionRealizeTest".into(),
        externals: Vec::new(),
        representations,
        functions: vec![function],
        entry: Some(SymbolId::new(module_id, 10)),
        span: TextRange::new(0, 8),
    };
    let (mir, mut wasi) =
        crate::mir::lower_module(cc_module).expect("realized decision should lower to MIR");
    assert!(mir.functions[0].blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, crate::mir::Instruction::Unreachable { .. }))
    }));
    let wasm =
        crate::wasm::lower_module(&mir, &mut wasi).expect("realized decision should lower to Wasm");
    let binary = crate::wasm::encode_module(&wasm).expect("realized decision should encode");
    crate::validator()
        .validate_all(&binary)
        .expect("encoded decision module should validate");
}
