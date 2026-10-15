//! Exceptional paths retain the work before failure without treating any
//! failed execution as successful normal completion.
use super::super::ir::Family;
use super::{behavior::assert_matches, fixtures::Fixture, heap::*, interpret::Interpreter};
use crate::control::execution::Program;

#[test]
fn replay_checks_later_distinct_finish_extents_before_rebuilding_any_activation() {
    let mut interpreter = Interpreter::new(Heap::default());
    for expired_parent in [false, true] {
        let mut f = Fixture::new();
        let stale = f.heap.extent(None);
        let child = f.heap.extent(Some(stale));
        f.expire(if expired_parent { stale } else { child });
        let good = f.heap.node(Node::Finish(f.checkpoint, f.saved, f.parent));
        let bad = f.heap.node(Node::Finish(child, f.empty, f.parent));
        let captured = f.heap.node(Node::Append(good, bad));
        assert_eq!(
            assert_matches(
                &mut interpreter,
                &f.heap,
                Program::CloneSequence,
                vec![captured, f.empty, f.empty, f.active, f.parent]
            ),
            Err(Fault::Trap)
        );
        assert_eq!(
            interpreter.heap.events.len(),
            if expired_parent { 3 } else { 2 }
        );
        assert!(interpreter.heap.objects[f.heap.objects.len()..].iter().all(|object| matches!(object,Object::Node(node) if node.case().family() == Family::Sequence)),"failed clone must not publish fresh activations or a result product");
        assert_eq!(
            &interpreter.heap.objects[..f.heap.objects.len()],
            f.heap.objects.as_slice()
        );
    }
}

#[test]
fn rejected_reverse_tails_keep_the_normal_rebuild_prefix() {
    let mut interpreter = Interpreter::new(Heap::default());
    for unknown in [false, true] {
        let mut f = Fixture::new();
        let bad = if unknown {
            f.heap.alloc(Object::Unknown(Family::Sequence, 99))
        } else {
            f.heap.node(Node::Append(f.saved, f.consumer))
        };
        let reversed = f.heap.node(Node::Finish(f.checkpoint, bad, f.parent));
        for program in [Program::RebuildSequence, Program::CloneSequence] {
            let args = if program == Program::RebuildSequence {
                vec![reversed, f.empty, f.active, f.parent]
            } else {
                vec![f.saved, f.empty, reversed, f.active, f.parent]
            };
            assert_eq!(
                assert_matches(&mut interpreter, &f.heap, program, args),
                Err(Fault::Trap)
            );
            let new = &interpreter.heap.objects[f.heap.objects.len()..];
            assert_eq!(
                new.iter()
                    .filter(|object| matches!(object, Object::Node(Node::ScopedExtent { .. })))
                    .count(),
                1
            );
            assert!(
                !new.iter()
                    .any(|object| matches!(object, Object::Replay(..))),
                "a rejected prefix cannot publish a replay result"
            );
        }
    }
}
