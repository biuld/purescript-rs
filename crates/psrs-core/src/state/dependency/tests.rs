use super::*;

const SPAN: TextRange = TextRange::new(10, 20);
fn dependency(id: u32) -> Dependency {
    Dependency {
        id: DependencyId(id),
        region: RegionId(0),
    }
}
fn transition(input: u32, output: u32) -> Transition {
    Transition {
        operation: output,
        input: DependencyId(input),
        output: dependency(output),
        span: SPAN,
    }
}
fn edge(target: u32, input: u32) -> Edge {
    Edge {
        target,
        arguments: vec![DependencyId(input)],
    }
}
fn block(id: u32, incoming: u32, transitions: Vec<Transition>, terminator: Terminator) -> Block {
    Block {
        id,
        parameters: vec![dependency(incoming)],
        transitions,
        terminator,
        span: SPAN,
    }
}
fn graph(blocks: Vec<Block>) -> Graph {
    Graph {
        entry: 0,
        regions: vec![RegionId(0)],
        blocks,
    }
}

#[test]
fn rejects_stale_predecessors_and_discarded_final_state() {
    let mut graph = graph(vec![block(
        0,
        0,
        vec![transition(0, 1), transition(1, 2)],
        Terminator::Return(vec![DependencyId(2)]),
    )]);
    graph.verify().unwrap();
    graph.blocks[0].transitions[1].input = DependencyId(0);
    let error = graph.verify().unwrap_err();
    assert_eq!(error.span, SPAN);
    assert_eq!(
        error.message,
        "state operation uses a stale or foreign dependency"
    );
    graph.blocks[0].transitions[1].input = DependencyId(1);
    graph.blocks[0].terminator = Terminator::Return(vec![DependencyId(1)]);
    assert!(graph.verify().unwrap_err().message.contains("discards"));
}

#[test]
fn multiway_edges_transfer_current_dependencies_and_traps_have_no_join_edge() {
    let mut graph = graph(vec![
        block(
            0,
            0,
            vec![transition(0, 1)],
            Terminator::Switch {
                case_edges: vec![edge(1, 1), edge(2, 1)],
                default_edge: edge(3, 1),
            },
        ),
        block(1, 2, vec![transition(2, 5)], Terminator::Jump(edge(4, 5))),
        block(2, 3, vec![], Terminator::Trap),
        block(3, 4, vec![], Terminator::Jump(edge(4, 4))),
        block(4, 6, vec![], Terminator::Return(vec![DependencyId(6)])),
    ]);
    graph.verify().unwrap();
    for choice in 0..3 {
        let original = graph.blocks[0].terminator.clone();
        let Terminator::Switch {
            case_edges,
            default_edge,
        } = &mut graph.blocks[0].terminator
        else {
            panic!()
        };
        let edge = if choice == 2 {
            default_edge
        } else {
            &mut case_edges[choice]
        };
        edge.arguments[0] = DependencyId(0);
        assert!(graph.verify().is_err());
        graph.blocks[0].terminator = original;
    }
}

#[test]
fn branch_exclusive_operations_merge_the_executed_dependency() {
    let mut graph = graph(vec![
        block(
            0,
            0,
            vec![],
            Terminator::Branch {
                then_edge: edge(1, 0),
                else_edge: edge(2, 0),
            },
        ),
        block(1, 1, vec![transition(1, 3)], Terminator::Jump(edge(3, 3))),
        block(2, 2, vec![transition(2, 4)], Terminator::Jump(edge(3, 4))),
        block(
            3,
            5,
            vec![transition(5, 6)],
            Terminator::Return(vec![DependencyId(6)]),
        ),
    ]);
    graph.verify().unwrap();
    graph.blocks[2].terminator = Terminator::Jump(edge(3, 3));
    assert!(
        graph
            .verify()
            .unwrap_err()
            .message
            .contains("current dependency")
    );
}

#[test]
fn loop_carries_the_latest_state_and_traps_need_no_successor() {
    let mut graph = graph(vec![
        block(0, 0, vec![], Terminator::Jump(edge(1, 0))),
        block(
            1,
            1,
            vec![transition(1, 2)],
            Terminator::Branch {
                then_edge: edge(1, 2),
                else_edge: edge(2, 2),
            },
        ),
        block(2, 3, vec![], Terminator::Trap),
    ]);
    graph.verify().unwrap();
    let Terminator::Branch { then_edge, .. } = &mut graph.blocks[1].terminator else {
        unreachable!()
    };
    then_edge.arguments[0] = DependencyId(1);
    assert!(graph.verify().is_err());
}

#[test]
fn rejects_wrong_region_duplicate_definitions_and_missing_roots() {
    let mut graph = graph(vec![block(
        0,
        0,
        vec![transition(0, 1)],
        Terminator::Return(vec![DependencyId(1)]),
    )]);
    graph.blocks[0].transitions[0].output.region = RegionId(1);
    assert!(graph.verify().is_err());
    graph.blocks[0].transitions[0].output = dependency(0);
    assert!(
        graph
            .verify()
            .unwrap_err()
            .message
            .contains("multiple definitions")
    );
    graph.entry = 10;
    assert!(graph.verify().is_err());
}

#[test]
fn rejects_a_projection_that_reorders_actual_operations() {
    let mut graph = graph(vec![block(
        0,
        0,
        vec![transition(0, 1), transition(1, 2)],
        Terminator::Return(vec![DependencyId(2)]),
    )]);
    graph.blocks[0].transitions[0].operation = 5;
    assert!(
        graph
            .verify()
            .unwrap_err()
            .message
            .contains("operation order")
    );
}
