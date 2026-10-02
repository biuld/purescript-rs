use super::super::{Action, ColumnKey, Decision, DecisionDag, DecisionEdge, NodeId, Test};
use super::array_projection_guards;
use psrs_core::TypeId;
use psrs_span::TextRange;

fn dag(length_test: Test, index: u32) -> DecisionDag {
    let root = ColumnKey::root();
    DecisionDag {
        root: NodeId(0),
        nodes: vec![
            Decision::Switch {
                column: root.clone(),
                ty: TypeId(0),
                edges: vec![DecisionEdge {
                    test: length_test,
                    actions: vec![Action::ArrayGet {
                        source: root,
                        target: ColumnKey::root(),
                        index,
                        source_type: TypeId(0),
                        target_type: TypeId(0),
                        span: TextRange::new(0, 0),
                    }],
                    target: NodeId(1),
                    span: TextRange::new(0, 0),
                }],
                default_actions: Vec::new(),
                default: None,
                span: TextRange::new(0, 0),
            },
            Decision::Leaf {
                branch: 0,
                actions: Vec::new(),
                span: TextRange::new(0, 0),
            },
        ],
    }
}

fn empty_leaf() -> Decision {
    Decision::Leaf {
        branch: 0,
        actions: Vec::new(),
        span: TextRange::new(0, 0),
    }
}

#[test]
fn array_projection_requires_matching_length_guard() {
    assert_eq!(
        array_projection_guards(&dag(Test::Irrefutable, 0)),
        Err("array projection is not dominated by a sufficient length test")
    );
}

#[test]
fn array_projection_index_must_fit_exact_length_guard() {
    assert_eq!(
        array_projection_guards(&dag(Test::ArrayLength { length: 1 }, 1)),
        Err("array projection is not dominated by a sufficient length test")
    );
    assert_eq!(
        array_projection_guards(&dag(Test::ArrayLength { length: 2 }, 1)),
        Ok(())
    );
}

#[test]
fn fallback_maps_preserve_inherited_array_length_facts() {
    let root = ColumnKey::root();
    let other = ColumnKey::slot(1);
    let alias = ColumnKey::slot(2);
    let span = TextRange::new(0, 0);
    let projection = Action::ArrayGet {
        source: alias.clone(),
        target: ColumnKey::slot(3),
        index: 0,
        source_type: TypeId(0),
        target_type: TypeId(0),
        span,
    };
    let dag = DecisionDag {
        root: NodeId(0),
        nodes: vec![
            Decision::Switch {
                column: root.clone(),
                ty: TypeId(0),
                edges: vec![DecisionEdge {
                    test: Test::ArrayLength { length: 1 },
                    actions: Vec::new(),
                    target: NodeId(1),
                    span,
                }],
                default_actions: Vec::new(),
                default: None,
                span,
            },
            Decision::Switch {
                column: other,
                ty: TypeId(0),
                edges: vec![DecisionEdge {
                    test: Test::Literal {
                        value: psrs_core::Literal::Integer(0),
                        ty: TypeId(0),
                    },
                    actions: Vec::new(),
                    target: NodeId(3),
                    span,
                }],
                default_actions: vec![Action::Map {
                    source: root,
                    target: alias.clone(),
                    span,
                }],
                default: Some(NodeId(2)),
                span,
            },
            Decision::Switch {
                column: alias,
                ty: TypeId(0),
                edges: vec![DecisionEdge {
                    test: Test::Irrefutable,
                    actions: vec![projection],
                    target: NodeId(4),
                    span,
                }],
                default_actions: Vec::new(),
                default: None,
                span,
            },
            empty_leaf(),
            empty_leaf(),
        ],
    };
    assert_eq!(array_projection_guards(&dag), Ok(()));
}

#[test]
fn remapping_an_unknown_array_clears_a_stale_length_fact() {
    let root = ColumnKey::root();
    let alias = ColumnKey::slot(1);
    let span = TextRange::new(0, 0);
    let dag = DecisionDag {
        root: NodeId(0),
        nodes: vec![
            Decision::Switch {
                column: alias.clone(),
                ty: TypeId(0),
                edges: vec![DecisionEdge {
                    test: Test::ArrayLength { length: 2 },
                    actions: Vec::new(),
                    target: NodeId(1),
                    span,
                }],
                default_actions: Vec::new(),
                default: None,
                span,
            },
            Decision::Switch {
                column: root.clone(),
                ty: TypeId(0),
                edges: vec![DecisionEdge {
                    test: Test::Literal {
                        value: psrs_core::Literal::Integer(0),
                        ty: TypeId(0),
                    },
                    actions: vec![Action::Map {
                        source: root,
                        target: alias.clone(),
                        span,
                    }],
                    target: NodeId(2),
                    span,
                }],
                default_actions: Vec::new(),
                default: None,
                span,
            },
            Decision::Switch {
                column: alias,
                ty: TypeId(0),
                edges: vec![DecisionEdge {
                    test: Test::Irrefutable,
                    actions: vec![Action::ArrayGet {
                        source: ColumnKey::slot(1),
                        target: ColumnKey::slot(2),
                        index: 0,
                        source_type: TypeId(0),
                        target_type: TypeId(0),
                        span,
                    }],
                    target: NodeId(3),
                    span,
                }],
                default_actions: Vec::new(),
                default: None,
                span,
            },
            empty_leaf(),
        ],
    };
    assert_eq!(
        array_projection_guards(&dag),
        Err("array projection is not dominated by a sufficient length test")
    );
}
