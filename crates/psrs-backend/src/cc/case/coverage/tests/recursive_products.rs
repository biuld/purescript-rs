use super::*;

#[test]
fn recursive_products_terminate_in_either_constructor_order() {
    for node_first in [true, false] {
        let mut constructors = vec![
            constructor(0, "Node", 0, vec![TypeId(1), TypeId(0), TypeId(0)]),
            constructor(1, "Leaf", 0, Vec::new()),
        ];
        if !node_first {
            constructors.reverse();
        }
        let module = module(
            vec![
                Type::Constructor(TypeConstructor::User(hir_type_id(0))),
                Type::Constructor(TypeConstructor::Int),
            ],
            constructors,
        );
        let node = || {
            pat(
                0,
                PatternKind::Constructor {
                    symbol: symbol(0),
                    arguments: vec![
                        pat(1, PatternKind::Wildcard),
                        pat(0, PatternKind::Wildcard),
                        pat(0, PatternKind::Wildcard),
                    ],
                },
            )
        };
        let incomplete = analyze(&module, TypeId(0), &[branch(node())]);
        assert!(!incomplete.exhaustive, "{incomplete:?}");
        assert_eq!(incomplete.witness.as_deref(), Some("Leaf"));
        assert!(incomplete.redundant_branches.is_empty());
        let complete = analyze(
            &module,
            TypeId(0),
            &[branch(node()), branch(nullary(1, 0)), branch(node())],
        );
        assert!(complete.exhaustive, "{complete:?}");
        assert_eq!(complete.redundant_branches, vec![2]);
    }
}

#[test]
fn an_uninhabited_recursive_product_does_not_invent_a_witness() {
    let module = module(
        vec![Type::Constructor(TypeConstructor::User(hir_type_id(0)))],
        vec![constructor(0, "Loop", 0, vec![TypeId(0), TypeId(0)])],
    );
    let report = analyze(&module, TypeId(0), &[]);
    assert!(report.exhaustive, "{report:?}");
    assert!(report.witness.is_none());
}

#[test]
fn missing_uninhabited_heads_do_not_hide_observed_field_gaps() {
    let module = module(
        vec![
            Type::Constructor(TypeConstructor::User(hir_type_id(0))),
            Type::Constructor(TypeConstructor::User(hir_type_id(1))),
            Type::Constructor(TypeConstructor::Boolean),
        ],
        vec![
            constructor(0, "Impossible", 0, vec![TypeId(1)]),
            constructor(1, "Present", 0, vec![TypeId(2)]),
            constructor(2, "Loop", 1, vec![TypeId(1)]),
        ],
    );
    let present_true = pat(
        0,
        PatternKind::Constructor {
            symbol: symbol(1),
            arguments: vec![pat(
                2,
                PatternKind::Literal {
                    value: psrs_core::Literal::Boolean(true),
                },
            )],
        },
    );
    let report = analyze(&module, TypeId(0), &[branch(present_true)]);
    assert!(!report.exhaustive, "{report:?}");
    assert_eq!(report.witness.as_deref(), Some("Present (false)"));
}
