//! Composition after individual transitions: actual shared queue occurrences,
//! bounded execution to completion, and ownership retained across replay.
use super::{
    behavior::assert_matches,
    fixtures::{Fixture, queues},
    heap::*,
    interpret::Interpreter,
};
use crate::control::execution::Program;

fn drive(interpreter: &mut Interpreter, mut args: Vec<Value>) -> Checked<Step> {
    for _ in 0..256 {
        let input = interpreter.heap.clone();
        match assert_matches(interpreter, &input, Program::Executor, args)? {
            Step::Advance(next) => args = next,
            other => return Ok(other),
        }
    }
    panic!("bounded computation did not finish within 256 transitions");
}

#[test]
fn bounded_executor_resume_preserves_saved_checkpoint_and_current_dispatch() {
    let mut interpreter = Interpreter::new(Heap::default());
    for queue in queues(2) {
        for scoped in [false, true] {
            let mut f = Fixture::new();
            let saved = f.queue(&queue);
            let source_checkpoint = f.heap.extent(None);
            let computation = f
                .heap
                .node(Node::Resume(f.answer, saved, source_checkpoint));
            let context = if scoped { f.scoped } else { f.root };
            let args = f.args(computation, f.consumer, context);
            assert_matches(&mut interpreter, &f.heap, Program::Executor, args.clone()).unwrap();
            f.expire(source_checkpoint);
            assert_eq!(
                assert_matches(&mut interpreter, &f.heap, Program::Executor, args),
                Err(Fault::Trap)
            );
            assert_eq!(interpreter.heap.events.len(), 1);
        }
    }
}

#[test]
fn left_and_right_bind_association_complete_with_the_same_ordered_callbacks() {
    for depth in 0..16 {
        for right in [false, true] {
            let mut f = Fixture::new();
            let mut computation = f.heap.node(Node::Return(f.answer));
            for _ in 0..depth {
                computation = if right {
                    let callback = f.heap.alloc(Object::Function {
                        label: "right-bind",
                        result: Ok(computation),
                    });
                    let answer = f.heap.node(Node::Return(f.answer));
                    f.heap.node(Node::Bind(answer, callback))
                } else {
                    f.heap.node(Node::Bind(computation, f.callback))
                };
            }
            let mut interpreter = Interpreter::new(f.heap.clone());
            let result = drive(&mut interpreter, f.args(computation, f.empty, f.root)).unwrap();
            assert!(matches!(result, Step::Publish(_)));
            assert_eq!(
                interpreter
                    .heap
                    .events
                    .iter()
                    .filter(|event| matches!(event, Event::Apply(..)))
                    .count(),
                depth
            );
        }
    }
}

#[test]
fn shared_subqueues_replay_each_occurrence_without_cloning_owned_callbacks() {
    let mut f = Fixture::new();
    let owned_computation = f.heap.node(Node::OwnedResume(f.owner, f.answer));
    let callback = f.heap.alloc(Object::Function {
        label: "owned-callback",
        result: Ok(owned_computation),
    });
    let finish = f.heap.node(Node::Finish(f.checkpoint, f.empty, f.parent));
    let shared = f.heap.node(Node::Cons(callback, finish));
    let captured = f.heap.node(Node::Append(shared, shared));
    let args = vec![captured, f.empty, f.empty, f.active, f.parent];
    let mut interpreter = Interpreter::new(f.heap.clone());
    let Step::Return(replay) =
        assert_matches(&mut interpreter, &f.heap, Program::CloneSequence, args).unwrap()
    else {
        unreachable!()
    };
    let Object::Replay(queue, active) = *interpreter.heap.get(replay).unwrap() else {
        unreachable!()
    };
    assert_eq!(
        &interpreter.heap.objects[..f.heap.objects.len()],
        f.heap.objects.as_slice()
    );
    let computation = interpreter.heap.node(Node::Return(f.answer));
    let state = vec![f.state, f.handler, computation, queue, active, f.root];
    assert_eq!(drive(&mut interpreter, state), Err(Fault::Trap));
    let calls = interpreter
        .heap
        .events
        .iter()
        .filter(|event| matches!(event,Event::Apply(callee,_) if *callee == callback))
        .count();
    assert_eq!(
        calls, 2,
        "both shared occurrences must remain in the replay"
    );
    let Node::Owner {
        callback: slot,
        status,
    } = interpreter.heap.get_node(f.owner).unwrap()
    else {
        unreachable!()
    };
    assert_eq!(interpreter.heap.objects[status], Object::Status(2));
    assert_eq!(interpreter.heap.objects[slot], Object::CallbackCell(status));
    assert_eq!(
        interpreter
            .heap
            .events
            .iter()
            .filter(|event| matches!(event,Event::Apply(callee,_) if *callee == f.callback))
            .count(),
        1
    );
}

#[test]
fn scoped_target_completion_closes_its_extent_and_restores_caller_work() {
    let mut f = Fixture::new();
    let source = f.heap.node(Node::Perform(f.request));
    let body = f.heap.node(Node::Bind(source, f.callback));
    let computation = f.heap.node(Node::Handle(f.ret, f.op, body));
    let caller_queue = f.heap.node(Node::Cons(f.ret, f.empty));
    let mut interpreter = Interpreter::new(f.heap.clone());
    let result = drive(&mut interpreter, f.args(computation, caller_queue, f.root)).unwrap();
    assert!(matches!(result, Step::Publish(_)));
    assert!(
        !interpreter
            .heap
            .events
            .iter()
            .any(|event| matches!(event,Event::Apply(callee,_) if *callee == f.callback)),
        "abandoned source work must not run"
    );
    assert!(
        interpreter
            .heap
            .events
            .iter()
            .any(|event| matches!(event,Event::Apply(callee,_) if *callee == f.ret)),
        "caller work must run after target completion"
    );
    assert_eq!(
        interpreter
            .heap
            .events
            .iter()
            .filter(|event| matches!(event, Event::WriteStatus(_, 1)))
            .count(),
        1
    );
}
