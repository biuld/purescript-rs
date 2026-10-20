use super::super::ir::Family;
use super::compare::equivalent;
use super::fixtures::{Fixture, Queue, queues};
use super::heap::*;
use super::interpret::Interpreter;
use super::reference::Reference;
use crate::control::execution::{ActionId, Program};

pub(super) fn assert_matches(
    interpreter: &mut Interpreter,
    input: &Heap,
    program: Program,
    args: Vec<Value>,
) -> Checked<Step> {
    let mut expected = input.clone();
    let expected_step = Reference::new(&mut expected).run(program, &args);
    interpreter.reset(input.clone());
    let actual_step = interpreter.run(program, args.clone());
    assert!(
        equivalent(
            &interpreter.heap,
            &actual_step,
            &expected,
            &expected_step,
            input.objects.len()
        ),
        "{program:?} {args:?}: plan={actual_step:?}, reference={expected_step:?}\nplan events={:?}\nreference events={:?}\ninput={:?}",
        interpreter.heap.events,
        expected.events,
        input.objects
    );
    actual_step
}

#[test]
fn every_action_matches_operands_state_identity_and_ordered_behavior() {
    let mut f = Fixture::new();
    let mut interpreter = Interpreter::new(f.heap.clone());
    let queues = f.queues();
    assert_matches(
        &mut interpreter,
        &f.heap,
        Program::Run,
        vec![f.state, f.handler, f.next],
    )
    .unwrap();
    for context in [f.root, f.scoped] {
        let computation = f.heap.node(Node::Return(f.answer));
        for queue in &queues {
            assert_matches(
                &mut interpreter,
                &f.heap,
                Program::Executor,
                f.args(computation, *queue, context),
            )
            .unwrap();
        }
        let computation = f.heap.node(Node::Perform(f.request));
        assert_matches(
            &mut interpreter,
            &f.heap,
            Program::Executor,
            f.args(computation, f.consumer, context),
        )
        .unwrap();
        let computation = f.heap.node(Node::Handle(f.ret, f.op, f.next));
        for queue in &queues {
            assert_matches(
                &mut interpreter,
                &f.heap,
                Program::Executor,
                f.args(computation, *queue, context),
            )
            .unwrap();
        }
    }
    let computation = f.heap.node(Node::Bind(f.next, f.callback));
    assert_matches(
        &mut interpreter,
        &f.heap,
        Program::Executor,
        f.args(computation, f.consumer, f.scoped),
    )
    .unwrap();
    let saved = f.heap.node(Node::Finish(f.checkpoint, f.saved, f.parent));
    let computation = f.heap.node(Node::Resume(f.answer, saved, f.checkpoint));
    assert_matches(
        &mut interpreter,
        &f.heap,
        Program::Executor,
        f.args(computation, f.consumer, f.root),
    )
    .unwrap();
    let computation = f.heap.node(Node::OwnedResume(f.owner, f.answer));
    assert_matches(
        &mut interpreter,
        &f.heap,
        Program::Executor,
        f.args(computation, f.consumer, f.scoped),
    )
    .unwrap();

    for queue in &queues {
        assert_matches(
            &mut interpreter,
            &f.heap,
            Program::CloneSequence,
            vec![*queue, f.empty, f.empty, f.active, f.parent],
        )
        .unwrap();
    }
    let todo = f.heap.node(Node::Append(f.saved, f.empty));
    assert_matches(
        &mut interpreter,
        &f.heap,
        Program::CloneSequence,
        vec![f.empty, todo, f.saved, f.active, f.parent],
    )
    .unwrap();
    for invalid in [f.saved, saved] {
        assert_eq!(
            assert_matches(
                &mut interpreter,
                &f.heap,
                Program::CloneSequence,
                vec![f.empty, invalid, f.empty, f.active, f.parent]
            ),
            Err(Fault::Trap)
        );
    }
    for reversed in [f.empty, f.saved, saved] {
        assert_matches(
            &mut interpreter,
            &f.heap,
            Program::RebuildSequence,
            vec![reversed, f.consumer, f.active, f.parent],
        )
        .unwrap();
    }
    let append = f.heap.node(Node::Append(f.saved, f.consumer));
    assert_eq!(
        assert_matches(
            &mut interpreter,
            &f.heap,
            Program::RebuildSequence,
            vec![append, f.consumer, f.active, f.parent]
        ),
        Err(Fault::Trap)
    );
    let child = f.heap.extent(Some(f.checkpoint));
    for program in [Program::Check, Program::Close] {
        for extent in [f.checkpoint, child] {
            assert_matches(&mut interpreter, &f.heap, program, vec![extent]).unwrap();
        }
    }
    for (program, args) in [
        (Program::Start, vec![]),
        (Program::Once, vec![f.raw]),
        (Program::Cancel, vec![f.owner]),
        (Program::RawResume, vec![f.raw, f.answer]),
    ] {
        assert_matches(&mut interpreter, &f.heap, program, args).unwrap();
    }
    assert_eq!(interpreter.actions.len(), ActionId::ALL.len());
    for action in ActionId::ALL {
        assert!(
            interpreter.actions.contains(&action),
            "missing executed action {action:?}"
        );
    }
}

#[test]
fn failed_pending_and_callback_traps_preserve_the_executed_prefix() {
    let mut interpreter = Interpreter::new(Heap::default());
    for status in 0..=3 {
        for callback_traps in [false, true] {
            let mut f = Fixture::new();
            f.owner_status(status);
            if callback_traps {
                f.heap.objects[f.callback] = Object::Function {
                    label: "queue-callback",
                    result: Err(Fault::CallbackTrap),
                };
            }
            let computation = f.heap.node(Node::OwnedResume(f.owner, f.answer));
            let result = assert_matches(
                &mut interpreter,
                &f.heap,
                Program::Executor,
                f.args(computation, f.consumer, f.scoped),
            );
            if status != 0 {
                assert_eq!(result, Err(Fault::Trap));
                assert_eq!(interpreter.heap.events.len(), 1);
            } else if callback_traps {
                assert_eq!(result, Err(Fault::CallbackTrap));
                let Node::Owner { callback, status } = interpreter.heap.get_node(f.owner).unwrap()
                else {
                    unreachable!()
                };
                assert_eq!(interpreter.heap.objects[status], Object::Status(1));
                assert_eq!(
                    interpreter.heap.objects[callback],
                    Object::CallbackCell(status)
                );
            } else {
                result.unwrap();
            }
            let cancelled =
                assert_matches(&mut interpreter, &f.heap, Program::Cancel, vec![f.owner]);
            if status != 0 {
                assert_eq!(cancelled, Err(Fault::Trap));
                assert_eq!(interpreter.heap.events.len(), 1);
            } else {
                cancelled.unwrap();
            }
        }
    }
}

#[test]
fn callback_failures_cannot_complete_their_target_transition() {
    let mut interpreter = Interpreter::new(Heap::default());
    for callback in 0..4 {
        let mut f = Fixture::new();
        let callee = [f.handler, f.ret, f.op, f.callback][callback];
        let Object::Function { result, .. } = &mut f.heap.objects[callee] else {
            unreachable!()
        };
        *result = Err(Fault::CallbackTrap);
        let (computation, queue, context) = match callback {
            0 => (f.heap.node(Node::Perform(f.request)), f.consumer, f.root),
            1 => (f.heap.node(Node::Return(f.answer)), f.empty, f.scoped),
            2 => (f.heap.node(Node::Perform(f.request)), f.consumer, f.scoped),
            _ => (
                f.heap.node(Node::Return(f.answer)),
                f.heap.node(Node::Cons(f.callback, f.empty)),
                f.root,
            ),
        };
        assert_eq!(
            assert_matches(
                &mut interpreter,
                &f.heap,
                Program::Executor,
                f.args(computation, queue, context)
            ),
            Err(Fault::CallbackTrap)
        );
        assert_eq!(interpreter.heap.events.len(), 1);
    }
}

#[test]
fn expiry_checks_every_ancestor_and_raw_invocation_remains_inert() {
    let mut interpreter = Interpreter::new(Heap::default());
    for closed in 0..3 {
        let mut f = Fixture::new();
        let parent = f.heap.extent(Some(f.parent));
        let child = f.heap.extent(Some(parent));
        f.expire([child, parent, f.parent][closed]);
        assert_eq!(
            assert_matches(&mut interpreter, &f.heap, Program::Check, vec![child]),
            Err(Fault::Trap)
        );
        assert_eq!(interpreter.heap.events.len(), closed + 1);
        let raw = f.heap.alloc(Object::Raw {
            queue: f.saved,
            checkpoint: child,
        });
        assert_eq!(
            assert_matches(
                &mut interpreter,
                &f.heap,
                Program::RawResume,
                vec![raw, f.answer]
            ),
            Err(Fault::Trap)
        );
    }
    let mut f = Fixture::new();
    let raw_queue = f.heap.node(Node::Cons(f.raw, f.consumer));
    let computation = f.heap.node(Node::Return(f.answer));
    let result = assert_matches(
        &mut interpreter,
        &f.heap,
        Program::Executor,
        f.args(computation, raw_queue, f.scoped),
    )
    .unwrap();
    let Step::Advance(state) = result else {
        panic!("raw queue callback must construct the next computation");
    };
    assert_eq!(
        interpreter.heap.get_node(state[2]).unwrap(),
        Node::Resume(f.answer, f.saved, f.checkpoint)
    );
    assert_eq!(state[3], f.consumer);
    assert_eq!(state[5], f.scoped);
}

#[test]
fn unknown_machine_tags_reject_without_unaccounted_work() {
    let mut interpreter = Interpreter::new(Heap::default());
    for family in [
        Family::Computation,
        Family::Sequence,
        Family::Context,
        Family::Extent,
    ] {
        let mut f = Fixture::new();
        let unknown = f.heap.alloc(Object::Unknown(family, 99));
        let computation = f.heap.node(Node::Return(f.answer));
        let (program, args) = match family {
            Family::Computation => (Program::Executor, f.args(unknown, f.empty, f.root)),
            Family::Sequence => (Program::Executor, f.args(computation, unknown, f.root)),
            Family::Context => (Program::Executor, f.args(computation, f.empty, unknown)),
            Family::Extent => (Program::Check, vec![unknown]),
            Family::Owner => unreachable!(),
        };
        assert_eq!(
            assert_matches(&mut interpreter, &f.heap, program, args),
            Err(Fault::Trap)
        );
        assert!(interpreter.heap.events.is_empty());
    }
}

fn flatten_input(queue: &Queue, f: &Fixture, callbacks: &mut Vec<Value>, finishes: &mut usize) {
    match queue {
        Queue::Empty => {}
        Queue::Cons(alternative, rest) => {
            callbacks.push(if *alternative { f.op } else { f.callback });
            flatten_input(rest, f, callbacks, finishes);
        }
        Queue::Finish(rest) => {
            *finishes += 1;
            flatten_input(rest, f, callbacks, finishes);
        }
        Queue::Append(left, right) => {
            flatten_input(left, f, callbacks, finishes);
            flatten_input(right, f, callbacks, finishes);
        }
    }
}

fn replay_contents(heap: &Heap, result: &Step) -> (Vec<Value>, Vec<Value>, Value) {
    let Step::Return(value) = result else {
        panic!("replay must return a product");
    };
    let Object::Replay(mut queue, active) = *heap.get(*value).unwrap() else {
        panic!("replay result");
    };
    let mut callbacks = Vec::new();
    let mut finishes = Vec::new();
    loop {
        match heap.get_node(queue).unwrap() {
            Node::Empty => break,
            Node::Cons(callback, rest) => {
                callbacks.push(callback);
                queue = rest;
            }
            Node::Finish(extent, rest, _) => {
                finishes.push(extent);
                queue = rest;
            }
            other => panic!("replay must be linear, got {other:?}"),
        }
    }
    (callbacks, finishes, active)
}

#[test]
fn bounded_replay_preserves_order_sharing_freshness_and_three_extent_roles() {
    let expressions = queues(3);
    assert_eq!(expressions.len(), 1805);
    let mut interpreter = Interpreter::new(Heap::default());
    for expression in expressions {
        let mut f = Fixture::new();
        let queue = f.queue(&expression);
        let args = vec![queue, f.empty, f.empty, f.active, f.parent];
        let first = assert_matches(
            &mut interpreter,
            &f.heap,
            Program::CloneSequence,
            args.clone(),
        )
        .unwrap();
        let (callbacks, finishes, active) = replay_contents(&interpreter.heap, &first);
        let mut expected_callbacks = Vec::new();
        let mut count = 0;
        flatten_input(&expression, &f, &mut expected_callbacks, &mut count);
        assert_eq!(callbacks, expected_callbacks, "{expression:?}");
        assert_eq!(finishes.len(), count);
        let (cons_count, finish_count, append_count) = expression.counts();
        let queue_allocations = interpreter.heap.objects[f.heap.objects.len()..].iter().filter(|object| matches!(object,Object::Node(node) if node.case().family() == Family::Sequence)).count();
        assert_eq!(
            queue_allocations,
            2 * (cons_count + finish_count) + append_count
        );
        assert_eq!(active, finishes.first().copied().unwrap_or(f.active));
        let unique = finishes
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), count);
        for (index, extent) in finishes.iter().enumerate() {
            assert!(*extent >= f.heap.objects.len());
            let Node::ScopedExtent { status, parent } = interpreter.heap.get_node(*extent).unwrap()
            else {
                panic!("activation must be a fresh child");
            };
            assert_eq!(interpreter.heap.objects[status], Object::Status(0));
            assert_eq!(parent, finishes.get(index + 1).copied().unwrap_or(f.parent));
        }
        assert_eq!(
            &interpreter.heap.objects[..f.heap.objects.len()],
            f.heap.objects.as_slice()
        );
        let before_second = interpreter.heap.clone();
        let second = assert_matches(
            &mut interpreter,
            &before_second,
            Program::CloneSequence,
            args,
        )
        .unwrap();
        let (_, second_finishes, _) = replay_contents(&interpreter.heap, &second);
        assert!(
            second_finishes
                .iter()
                .all(|extent| !unique.contains(extent))
        );
        let mut expired = f.clone();
        expired.expire(f.checkpoint);
        let result = assert_matches(
            &mut interpreter,
            &expired.heap,
            Program::CloneSequence,
            vec![queue, f.empty, f.empty, f.active, f.parent],
        );
        if count == 0 {
            result.unwrap();
        } else {
            assert_eq!(result, Err(Fault::Trap));
        }
    }
}

#[test]
fn channel_abort_closes_the_runner_root_and_consumes_acquired_owners() {
    let mut f = Fixture::new();
    let Object::Function { result, .. } = &mut f.heap.objects[f.callback] else {
        unreachable!()
    };
    *result = Err(Fault::Channel(73));
    let computation = f.heap.node(Node::OwnedResume(f.owner, f.answer));
    let mut interpreter = Interpreter::new(f.heap.clone());
    assert_eq!(
        assert_matches(
            &mut interpreter,
            &f.heap,
            Program::Run,
            vec![f.state, f.handler, computation]
        ),
        Err(Fault::Channel(73))
    );
    let Node::Owner { callback, status } = interpreter.heap.get_node(f.owner).unwrap() else {
        unreachable!()
    };
    assert_eq!(interpreter.heap.status(status).unwrap(), 2);
    assert_eq!(interpreter.heap.callback(callback).unwrap(), status);
    // The newly allocated runner root is closed; roots that predate this run stay live.
    let roots = interpreter.heap.objects[f.heap.objects.len()..]
        .iter()
        .filter_map(|object| {
            if let Object::Node(Node::RootExtent(cell)) = object {
                Some(*cell)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        interpreter.exceptional_actions,
        std::collections::HashSet::from([
            crate::control::execution::ExceptionalAction::AcquiredCallbackExit,
            crate::control::execution::ExceptionalAction::RunAbort,
        ])
    );
    assert_eq!(roots.len(), 1);
    assert_eq!(interpreter.heap.status(roots[0]).unwrap(), 1);
}
