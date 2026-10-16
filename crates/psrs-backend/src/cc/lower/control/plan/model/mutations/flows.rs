use super::*;

#[test]
fn swapped_same_shaped_clauses_do_not_pass_the_behavioral_model() {
    let mut f = Fixture::new();
    let computation = f.heap.node(Node::Return(f.answer));
    let args = f.args(computation, f.empty, f.scoped);
    detects(f, Program::Executor, args, |plan| {
        assert!(edit(arm(plan, C::Return), &mut |op| {
            if let Op::Project {
                case: Case::Context(X::Scoped),
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
fn target_finish_restores_its_caller_extent_not_its_owned_extent() {
    let mut f = Fixture::new();
    let computation = f.heap.node(Node::Return(f.answer));
    let args = f.args(computation, f.empty, f.scoped);
    detects(f, Program::Executor, args, |plan| {
        assert!(edit(arm(plan, C::Return), &mut |op| {
            if let Op::Project {
                case: Case::Context(X::Scoped),
                field,
                ..
            } = op
                && *field == 3
            {
                *field = 5;
                return true;
            }
            false
        }));
    });
}

#[test]
fn callbacks_receive_the_current_answer_instead_of_the_root_state() {
    let mut f = Fixture::new();
    let queue = f.heap.node(Node::Cons(f.callback, f.empty));
    let computation = f.heap.node(Node::Return(f.answer));
    let args = f.args(computation, queue, f.root);
    detects(f, Program::Executor, args, |plan| {
        let Exec::Steps { next, .. } = arm(plan, C::Return) else {
            unreachable!()
        };
        let Exec::Switch { arms, .. } = next.as_mut() else {
            unreachable!()
        };
        let cons = &mut arms
            .iter_mut()
            .find(|(tag, _)| *tag == Q::Cons.tag())
            .unwrap()
            .1;
        assert!(edit(cons, &mut |op| {
            if let Op::Apply { args, .. } = op {
                args[0] = Operand::State(State::Value);
                return true;
            }
            false
        }));
    });
}

#[test]
fn bind_enters_its_inner_computation_instead_of_reentering_itself() {
    let mut f = Fixture::new();
    let computation = f.heap.node(Node::Bind(f.next, f.callback));
    let args = f.args(computation, f.consumer, f.scoped);
    detects(f, Program::Executor, args, |plan| {
        let Exec::Steps { next, .. } = arm(plan, C::Bind) else {
            unreachable!()
        };
        let Exec::Advance { computation, .. } = next.as_mut() else {
            unreachable!()
        };
        *computation = Operand::State(State::Computation);
    });
}

#[test]
fn root_handler_state_and_next_computation_do_not_share_a_projection() {
    let mut f = Fixture::new();
    let computation = f.heap.node(Node::Perform(f.request));
    let args = f.args(computation, f.consumer, f.root);
    detects(f, Program::Executor, args, |plan| {
        assert!(edit(arm(plan, C::Perform), &mut |op| {
            if let Op::PairField { field, .. } = op
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
fn finish_closes_its_owned_extent_not_its_restore_parent() {
    let mut f = Fixture::new();
    let computation = f.heap.node(Node::Return(f.answer));
    let queue = f.heap.node(Node::Finish(f.checkpoint, f.empty, f.parent));
    let args = f.args(computation, queue, f.scoped);
    detects(f, Program::Executor, args, |plan| {
        let Exec::Steps { next, .. } = arm(plan, C::Return) else {
            unreachable!()
        };
        let Exec::Switch { arms, .. } = next.as_mut() else {
            unreachable!()
        };
        let finish = &mut arms
            .iter_mut()
            .find(|(tag, _)| *tag == Q::Finish.tag())
            .unwrap()
            .1;
        assert!(edit(finish, &mut |op| {
            if let Op::CloseBoundary { extent } = op {
                *extent = Operand::State(State::Extent);
                return true;
            }
            false
        }));
    });
}

#[test]
fn handle_keeps_caller_and_owned_extents_distinct() {
    let mut f = Fixture::new();
    let computation = f.heap.node(Node::Handle(f.ret, f.op, f.next));
    let args = f.args(computation, f.empty, f.scoped);
    detects(f, Program::Executor, args, |plan| {
        assert!(edit(arm(plan, C::Handle), &mut |op| {
            if let Op::Node {
                case: Case::Context(X::Scoped),
                fields,
            } = op
            {
                fields[3] = fields[5].clone();
                return true;
            }
            false
        }));
    });
}
