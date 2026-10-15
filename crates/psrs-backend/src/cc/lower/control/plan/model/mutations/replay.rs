use super::*;

#[test]
fn perform_captures_the_dispatch_checkpoint_not_the_execution_extent() {
    let mut f = Fixture::new();
    let computation = f.heap.node(Node::Perform(f.request));
    let args = f.args(computation, f.consumer, f.root);
    detects(f, Program::Executor, args, |plan| {
        assert!(edit(arm(plan, C::Perform), &mut |op| {
            if let Op::ResumeClosure { captures } = op {
                captures[1] = Operand::State(State::Extent);
                return true;
            }
            false
        }));
    });
}

#[test]
fn resume_checks_the_saved_checkpoint_not_the_current_execution_extent() {
    let mut f = Fixture::new();
    f.expire(f.checkpoint);
    let computation = f.heap.node(Node::Resume(f.answer, f.saved, f.checkpoint));
    let args = f.args(computation, f.consumer, f.scoped);
    detects(f, Program::Executor, args, |plan| {
        assert!(edit(arm(plan, C::Resume), &mut |op| {
            if let Op::CheckBoundary { extent } = op {
                *extent = Operand::State(State::Extent);
                return true;
            }
            false
        }));
    });
}

#[test]
fn clone_cannot_drop_the_left_queue_when_pending_right_work_exists() {
    let mut f = Fixture::new();
    let queue = f.heap.node(Node::Append(f.saved, f.consumer));
    let args = vec![queue, f.empty, f.empty, f.active, f.parent];
    detects(f, Program::CloneSequence, args, |plan| {
        assert!(edit(plan, &mut |op| {
            if let Op::Project {
                case: Case::Sequence(Q::Append),
                source: Operand::Param(0),
                field,
                ..
            } = op
                && *field == 0
            {
                *field = 1;
                return true;
            }
            false
        }));
    });
}

#[test]
fn resume_appends_captured_work_before_consumer_work() {
    let mut f = Fixture::new();
    let computation = f.heap.node(Node::Resume(f.answer, f.saved, f.checkpoint));
    let args = f.args(computation, f.consumer, f.scoped);
    detects(f, Program::Executor, args, |plan| {
        assert!(edit(arm(plan, C::Resume), &mut |op| {
            if let Op::Node {
                case: Case::Sequence(Q::Append),
                fields,
            } = op
            {
                fields.swap(0, 1);
                return true;
            }
            false
        }));
    });
}

#[test]
fn replay_parents_fresh_activations_to_dispatch_not_execution() {
    let mut f = Fixture::new();
    let queue = f.heap.node(Node::Finish(f.checkpoint, f.empty, f.parent));
    let args = vec![queue, f.empty, f.active, f.parent];
    detects(f, Program::RebuildSequence, args, |plan| {
        assert!(edit(plan, &mut |op| {
            if let Op::FreshExtent { parent } = op {
                *parent = Some(Operand::Param(2));
                return true;
            }
            false
        }));
    });
}

#[test]
fn replay_must_not_reuse_a_saved_extent_as_a_fresh_activation() {
    let mut f = Fixture::new();
    let queue = f.heap.node(Node::Finish(f.checkpoint, f.empty, f.parent));
    let args = vec![queue, f.empty, f.active, f.parent];
    detects(f, Program::RebuildSequence, args, |plan| {
        assert!(edit(plan, &mut |op| {
            if matches!(op, Op::FreshExtent { .. }) {
                *op = Op::Project {
                    source: Operand::Param(0),
                    case: Case::Sequence(Q::Finish),
                    field: 0,
                };
                return true;
            }
            false
        }));
    });
}

#[test]
fn replay_result_keeps_the_rebuilt_queue_and_matching_activation() {
    let f = Fixture::new();
    let args = vec![f.empty, f.consumer, f.active, f.parent];
    detects(f, Program::RebuildSequence, args, |plan| {
        assert!(edit(plan, &mut |op| {
            if let Op::Product { fields, .. } = op {
                fields[0] = Operand::Param(0);
                return true;
            }
            false
        }));
    });
}
