use super::*;

fn literal(value: psrs_core::Literal, ty: psrs_core::TypeId, span: TextRange) -> Pattern {
    Pattern {
        kind: PatternKind::Literal { value },
        ty,
        span,
    }
}

fn array(elements: Vec<Pattern>, ty: psrs_core::TypeId, span: TextRange) -> Pattern {
    Pattern {
        kind: PatternKind::Array { elements },
        ty,
        span,
    }
}

fn int_array_module() -> (Module, psrs_core::TypeId, psrs_core::TypeId) {
    let (mut module, _, _) = bool_module();
    let int = psrs_core::TypeId(module.types.len() as u32);
    module.types.push(Type::Constructor(TypeConstructor::Int));
    let array_head = psrs_core::TypeId(module.types.len() as u32);
    module.types.push(Type::Constructor(TypeConstructor::Array));
    let array_int = psrs_core::TypeId(module.types.len() as u32);
    module.types.push(Type::Application(array_head, int));
    (module, int, array_int)
}

#[test]
fn scalar_literal_keys_are_normalized_and_duplicate_rows_keep_the_first_branch() {
    let (mut module, _, _) = bool_module();
    let number = psrs_core::TypeId(module.types.len() as u32);
    module
        .types
        .push(Type::Constructor(TypeConstructor::Number));
    let branches = vec![
        branch(
            literal(
                psrs_core::Literal::Number("1.0".into()),
                number,
                TextRange::new(1, 4),
            ),
            1,
            TextRange::new(1, 8),
        ),
        branch(
            literal(
                psrs_core::Literal::Number("1e0".into()),
                number,
                TextRange::new(9, 12),
            ),
            2,
            TextRange::new(9, 16),
        ),
        branch(
            Pattern {
                kind: PatternKind::Wildcard,
                ty: number,
                span: TextRange::new(17, 18),
            },
            3,
            TextRange::new(17, 22),
        ),
    ];
    let dag = compile_dag(
        &module,
        &HashSet::new(),
        number,
        &branches,
        TextRange::new(0, 22),
    )
    .expect("number literal decision compilation");
    let Decision::Switch { edges, default, .. } = &dag.nodes[dag.root.0] else {
        panic!("number literals should compile to a scalar switch");
    };
    assert_eq!(
        edges.len(),
        1,
        "numeric spellings with equal f64 values share a test"
    );
    assert_eq!(
        edges[0].test,
        Test::Literal {
            value: psrs_core::Literal::Number("1".into()),
            ty: number,
        }
    );
    let Decision::Leaf { branch, .. } = &dag.nodes[edges[0].target.0] else {
        panic!("the literal edge should select the first row");
    };
    assert_eq!(*branch, 0);
    let Decision::Leaf { branch, .. } = &dag.nodes[default.unwrap().0] else {
        panic!("the infinite literal complement should use its wildcard");
    };
    assert_eq!(*branch, 2);
}

#[test]
fn array_projections_are_emitted_left_to_right_only_on_exact_length_edges() {
    let (module, int, array_int) = int_array_module();
    let branches = vec![
        branch(
            array(
                vec![
                    literal(psrs_core::Literal::Integer(7), int, TextRange::new(2, 3)),
                    literal(psrs_core::Literal::Integer(9), int, TextRange::new(5, 6)),
                ],
                array_int,
                TextRange::new(1, 7),
            ),
            1,
            TextRange::new(1, 11),
        ),
        branch(
            array(
                vec![
                    literal(psrs_core::Literal::Integer(7), int, TextRange::new(13, 14)),
                    Pattern {
                        kind: PatternKind::Wildcard,
                        ty: int,
                        span: TextRange::new(16, 17),
                    },
                ],
                array_int,
                TextRange::new(12, 18),
            ),
            2,
            TextRange::new(12, 22),
        ),
        branch(
            Pattern {
                kind: PatternKind::Wildcard,
                ty: array_int,
                span: TextRange::new(23, 24),
            },
            3,
            TextRange::new(23, 28),
        ),
    ];
    let dag = compile_dag(
        &module,
        &HashSet::new(),
        array_int,
        &branches,
        TextRange::new(0, 28),
    )
    .expect("array pattern decision compilation");
    let Decision::Switch { edges, default, .. } = &dag.nodes[dag.root.0] else {
        panic!("array rows should compile to a length switch");
    };
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].test, Test::ArrayLength { length: 2 });
    let projections = edges[0]
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::ArrayGet { index, .. } => Some(*index),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(projections, vec![0, 1]);
    let Decision::Leaf { branch, .. } = &dag.nodes[default.unwrap().0] else {
        panic!("all unobserved array lengths should reach the wildcard");
    };
    assert_eq!(*branch, 2);
}

#[test]
fn named_pattern_binding_reads_the_exact_current_column() {
    let (module, int, _) = int_array_module();
    let local = LocalId(91);
    let pattern = Pattern {
        kind: PatternKind::Named {
            id: local,
            pattern: Box::new(Pattern {
                kind: PatternKind::Wildcard,
                ty: int,
                span: TextRange::new(2, 3),
            }),
        },
        ty: int,
        span: TextRange::new(1, 3),
    };
    let dag = compile_dag(
        &module,
        &HashSet::new(),
        int,
        &[branch(pattern, 1, TextRange::new(1, 5))],
        TextRange::new(0, 5),
    )
    .expect("named pattern decision compilation");
    let Decision::Switch { edges, .. } = &dag.nodes[dag.root.0] else {
        panic!("an alias-wildcard should be an irrefutable column");
    };
    assert_eq!(edges.len(), 1);
    assert!(edges[0].actions.is_empty());
    let Decision::Leaf { actions, .. } = &dag.nodes[edges[0].target.0] else {
        panic!("the alias row should reach a leaf");
    };
    assert!(actions.contains(&Action::Bind {
        id: local,
        source: ColumnKey::root(),
    }));
}
