use super::matrix::{canonicalize, surface_pattern};
use super::*;
use psrs_core::{CaseBranch, Expr, ExprKind, Module, Pattern, PatternKind, Type, TypeConstructor};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

fn bool_module() -> (Module, SymbolId, SymbolId) {
    let type_id = HirTypeId::new(ModuleId(0), 0);
    let true_symbol = SymbolId::new(ModuleId(0), 0);
    let false_symbol = SymbolId::new(ModuleId(0), 1);
    let module = Module {
        id: ModuleId(0),
        name: "DecisionTest".into(),
        externals: Vec::new(),
        types: vec![Type::Constructor(TypeConstructor::User(type_id))],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: vec![
            psrs_core::ConstructorInfo {
                symbol: true_symbol,
                name: "True".into(),
                type_id,
                tag: 0,
                field_count: 0,
                field_types: Vec::new(),
            },
            psrs_core::ConstructorInfo {
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
        span: TextRange::new(0, 80),
    };
    (module, true_symbol, false_symbol)
}

fn record_bool_module() -> (Module, SymbolId, SymbolId) {
    let (mut module, true_symbol, false_symbol) = bool_module();
    module
        .types
        .push(Type::Record(vec![("active".into(), psrs_core::TypeId(0))]));
    (module, true_symbol, false_symbol)
}

fn constructor(symbol: SymbolId, ty: psrs_core::TypeId, span: TextRange) -> Pattern {
    Pattern {
        kind: PatternKind::Constructor {
            symbol,
            arguments: Vec::new(),
        },
        ty,
        span,
    }
}

fn branch(pattern: Pattern, index: i32, span: TextRange) -> CaseBranch {
    CaseBranch {
        pattern,
        value: Expr {
            kind: ExprKind::Integer(index),
            ty: psrs_core::TypeId(0),
            span,
        },
        span,
    }
}

#[test]
fn repeated_residual_matrix_is_shared_across_projection_paths() {
    let (module, true_symbol, _) = bool_module();
    let mut compiler = Compiler {
        module: &module,
        newtypes: HashSet::new(),
        memo: HashMap::new(),
        nodes: Vec::new(),
    };
    let local = LocalId(77);
    let first_path = ColumnKey::root().child(PathStep::ConstructorField(
        SymbolId::new(ModuleId(0), 10),
        0,
    ));
    let second_path = ColumnKey::root().child(PathStep::RecordField(2));
    let make_matrix = |path: ColumnKey| {
        let rows = vec![
            Row {
                patterns: vec![surface_pattern(&constructor(
                    true_symbol,
                    psrs_core::TypeId(0),
                    TextRange::new(2, 7),
                ))],
                bindings: vec![(local, path.clone())],
                branch: 0,
                span: TextRange::new(2, 12),
            },
            Row {
                patterns: vec![SurfacePattern::Any {
                    ty: psrs_core::TypeId(0),
                }],
                bindings: Vec::new(),
                branch: 1,
                span: TextRange::new(13, 19),
            },
        ];
        let columns = vec![Column {
            key: path,
            ty: psrs_core::TypeId(0),
        }];
        (columns, rows)
    };

    let (mut first_columns, mut first_rows) = make_matrix(first_path);
    canonicalize(&mut first_columns, &mut first_rows);
    let first = compiler
        .compile_matrix(first_columns, first_rows, TextRange::new(0, 30))
        .expect("first residual matrix");
    let nodes_after_first = compiler.nodes.len();

    let (mut second_columns, mut second_rows) = make_matrix(second_path);
    canonicalize(&mut second_columns, &mut second_rows);
    let second = compiler
        .compile_matrix(second_columns, second_rows, TextRange::new(0, 30))
        .expect("second residual matrix");

    assert_eq!(first, second);
    assert_eq!(compiler.nodes.len(), nodes_after_first);
    let Decision::Switch { edges, .. } = &compiler.nodes[first.0] else {
        panic!("residual matrix should dispatch on its constructor");
    };
    let Decision::Leaf {
        branch,
        actions,
        span,
    } = &compiler.nodes[edges[0].target.0]
    else {
        panic!("first matching row should reach a leaf");
    };
    assert_eq!(*branch, 0);
    assert_eq!(*span, TextRange::new(2, 12));
    assert!(actions.contains(&Action::Bind {
        id: local,
        source: ColumnKey::slot(0),
    }));
}

#[test]
fn duplicate_constructor_rows_keep_first_match_and_row_spans() {
    let (module, true_symbol, _) = bool_module();
    let branches = vec![
        branch(
            constructor(true_symbol, psrs_core::TypeId(0), TextRange::new(1, 5)),
            0,
            TextRange::new(1, 8),
        ),
        branch(
            constructor(true_symbol, psrs_core::TypeId(0), TextRange::new(9, 13)),
            1,
            TextRange::new(9, 16),
        ),
        branch(
            Pattern {
                kind: PatternKind::Wildcard,
                ty: psrs_core::TypeId(0),
                span: TextRange::new(17, 18),
            },
            2,
            TextRange::new(17, 22),
        ),
    ];
    let dag = compile_dag(
        &module,
        &HashSet::new(),
        psrs_core::TypeId(0),
        &branches,
        TextRange::new(0, 22),
    )
    .expect("decision compilation");
    let Decision::Switch { edges, default, .. } = &dag.nodes[dag.root.0] else {
        panic!("enum case should begin with a switch");
    };
    let Decision::Leaf { branch, span, .. } = &dag.nodes[edges[0].target.0] else {
        panic!("nullary constructor should select a leaf");
    };
    assert_eq!(*branch, 0);
    assert_eq!(*span, branches[0].span);
    let Decision::Leaf { branch, .. } = &dag.nodes[default.expect("wildcard default").0] else {
        panic!("wildcard should be the default leaf");
    };
    assert_eq!(*branch, 2);
}

#[test]
fn complete_nullary_signature_leaves_malformed_tag_as_fail() {
    let (module, true_symbol, false_symbol) = bool_module();
    let branches = vec![
        branch(
            constructor(true_symbol, psrs_core::TypeId(0), TextRange::new(1, 5)),
            0,
            TextRange::new(1, 8),
        ),
        branch(
            constructor(false_symbol, psrs_core::TypeId(0), TextRange::new(9, 14)),
            1,
            TextRange::new(9, 17),
        ),
    ];
    let dag = compile_dag(
        &module,
        &HashSet::new(),
        psrs_core::TypeId(0),
        &branches,
        TextRange::new(0, 17),
    )
    .expect("decision compilation");
    let Decision::Switch { default, edges, .. } = &dag.nodes[dag.root.0] else {
        panic!("enum case should produce a switch");
    };
    assert!(default.is_none());
    assert_eq!(edges.len(), 2);
}

#[test]
fn nested_record_constructor_has_one_projection_and_keeps_spans() {
    let (module, true_symbol, false_symbol) = record_bool_module();
    let record_pattern = |symbol, start| Pattern {
        kind: PatternKind::Record {
            fields: vec![(
                "active".into(),
                constructor(
                    symbol,
                    psrs_core::TypeId(0),
                    TextRange::new(start + 1, start + 4),
                ),
            )],
        },
        ty: psrs_core::TypeId(1),
        span: TextRange::new(start, start + 6),
    };
    let branches = vec![
        branch(record_pattern(true_symbol, 1), 0, TextRange::new(1, 9)),
        branch(record_pattern(false_symbol, 10), 1, TextRange::new(10, 19)),
    ];
    let dag = compile_dag(
        &module,
        &HashSet::new(),
        psrs_core::TypeId(1),
        &branches,
        TextRange::new(0, 20),
    )
    .expect("nested record decision compilation");
    let Decision::Switch { edges, .. } = &dag.nodes[dag.root.0] else {
        panic!("record should specialize as an irrefutable product");
    };
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].span, branches[0].pattern.span);
    assert_eq!(
        edges[0]
            .actions
            .iter()
            .filter(|action| matches!(action, Action::Project { field: 0, .. }))
            .count(),
        1
    );
    let Decision::Switch { edges: nested, .. } = &dag.nodes[edges[0].target.0] else {
        panic!("nested Boolean should retain its constructor switch");
    };
    assert_eq!(nested.len(), 2);
}
