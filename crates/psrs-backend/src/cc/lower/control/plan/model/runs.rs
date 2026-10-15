//! Complete normal Run lifecycle, including fresh roots across invocations.
use super::{behavior::assert_matches, fixtures::Fixture, heap::*, interpret::Interpreter};
use crate::control::execution::Program;

#[test]
fn run_composes_dispatch_callbacks_and_normal_root_completion() {
    let mut f = Fixture::new();
    let performed = f.heap.node(Node::Perform(f.request));
    let bound = f.heap.node(Node::Bind(f.next, f.callback));
    let handled = f.heap.node(Node::Handle(f.ret, f.op, f.next));
    for computation in [f.next, performed, bound, handled] {
        let mut interpreter = Interpreter::new(f.heap.clone());
        assert_matches(
            &mut interpreter,
            &f.heap,
            Program::Run,
            vec![f.state, f.handler, computation],
        )
        .unwrap();
        let roots = interpreter.heap.objects[f.heap.objects.len()..]
            .iter()
            .filter_map(|object| {
                if let Object::Node(Node::RootExtent(status)) = object {
                    Some(*status)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(roots.len(), 1, "each Run owns exactly one fresh root");
        assert_eq!(
            interpreter.heap.status(roots[0]),
            Ok(1),
            "normal completion closes its root"
        );
        let after_first = interpreter.heap.clone();
        assert_matches(
            &mut interpreter,
            &after_first,
            Program::Run,
            vec![f.state, f.handler, computation],
        )
        .unwrap();
        let roots = interpreter.heap.objects[after_first.objects.len()..]
            .iter()
            .filter_map(|object| {
                if let Object::Node(Node::RootExtent(status)) = object {
                    Some(*status)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(roots.len(), 1, "reentry must allocate its own root");
        assert_eq!(interpreter.heap.status(roots[0]), Ok(1));
    }
}
