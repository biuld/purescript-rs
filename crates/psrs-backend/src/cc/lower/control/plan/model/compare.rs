//! Compare observable transitions modulo fresh allocation IDs, preserving
//! input identity and aliasing. Replay traversal temporaries are budgeted by
//! the flat reference model; other allocations must agree exactly by role.
use super::heap::*;
use std::collections::{HashMap, HashSet};

fn queue_allocations(heap: &Heap, original: usize) -> usize {
    heap.objects[original..].iter().filter(|object| matches!(object,Object::Node(node) if node.case().family() == super::super::ir::Family::Sequence)).count()
}

fn allocations(
    heap: &Heap,
    original: usize,
) -> HashMap<
    (
        std::mem::Discriminant<Object>,
        Option<super::super::ir::Case>,
    ),
    usize,
> {
    let mut counts = HashMap::new();
    for object in &heap.objects[original..] {
        let case = match object {
            Object::Node(node) if node.case().family() == super::super::ir::Family::Sequence => {
                continue;
            }
            Object::Node(node) => Some(node.case()),
            _ => None,
        };
        *counts
            .entry((std::mem::discriminant(object), case))
            .or_default() += 1;
    }
    counts
}

pub(super) fn equivalent(
    a: &Heap,
    result_a: &Checked<Step>,
    b: &Heap,
    result_b: &Checked<Step>,
    original: usize,
) -> bool {
    let mut compare = Compare {
        a,
        b,
        original,
        forward: HashMap::new(),
        backward: HashMap::new(),
        visited: HashSet::new(),
    };
    let result = match (result_a, result_b) {
        (Ok(Step::Return(a)), Ok(Step::Return(b)))
        | (Ok(Step::Publish(a)), Ok(Step::Publish(b))) => compare.value(*a, *b),
        (Ok(Step::Advance(a)), Ok(Step::Advance(b))) => compare.values(a, b),
        (Err(a), Err(b)) => {
            a == b && matches!(a, Fault::Trap | Fault::CallbackTrap | Fault::Channel(_))
        }
        _ => false,
    };
    result
        && allocations(a, original) == allocations(b, original)
        && b.queue_work
            .checked_sub(a.queue_work)
            .and_then(|work| queue_allocations(b, original).checked_add(work))
            == Some(queue_allocations(a, original))
        && a.events.len() == b.events.len()
        && a.events
            .iter()
            .zip(&b.events)
            .all(|(a, b)| compare.event(a, b))
        && (0..original).all(|value| compare.value(value, value))
}

struct Compare<'a> {
    a: &'a Heap,
    b: &'a Heap,
    original: usize,
    forward: HashMap<Value, Value>,
    backward: HashMap<Value, Value>,
    visited: HashSet<(Value, Value)>,
}

impl Compare<'_> {
    fn values(&mut self, a: &[Value], b: &[Value]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(a, b)| self.value(*a, *b))
    }
    fn value(&mut self, a: Value, b: Value) -> bool {
        if a < self.original || b < self.original {
            if a != b {
                return false;
            }
        } else {
            if self.forward.get(&a).is_some_and(|other| *other != b)
                || self.backward.get(&b).is_some_and(|other| *other != a)
            {
                return false;
            }
            self.forward.insert(a, b);
            self.backward.insert(b, a);
        }
        if !self.visited.insert((a, b)) {
            return true;
        }
        let (Some(a), Some(b)) = (self.a.objects.get(a), self.b.objects.get(b)) else {
            return false;
        };
        match (a, b) {
            (Object::Node(a), Object::Node(b)) => {
                a.case() == b.case() && self.values(&a.fields(), &b.fields())
            }
            (Object::Atom(a), Object::Atom(b)) => a == b,
            (Object::Status(a), Object::Status(b)) => a == b,
            (Object::CallbackCell(a), Object::CallbackCell(b)) => self.value(*a, *b),
            (
                Object::Function {
                    label: a,
                    result: ar,
                },
                Object::Function {
                    label: b,
                    result: br,
                },
            ) => {
                a == b
                    && match (ar, br) {
                        (Ok(a), Ok(b)) => self.value(*a, *b),
                        (Err(a), Err(b)) => a == b,
                        _ => false,
                    }
            }
            (
                Object::Raw {
                    queue: a,
                    checkpoint: ae,
                },
                Object::Raw {
                    queue: b,
                    checkpoint: be,
                },
            ) => self.value(*a, *b) && self.value(*ae, *be),
            (Object::Pair(a, aa), Object::Pair(b, ba))
            | (Object::Replay(a, aa), Object::Replay(b, ba)) => {
                self.value(*a, *b) && self.value(*aa, *ba)
            }
            (Object::Unknown(a, at), Object::Unknown(b, bt)) => a == b && at == bt,
            _ => false,
        }
    }
    fn event(&mut self, a: &Event, b: &Event) -> bool {
        match (a, b) {
            (Event::ReadStatus(a, av), Event::ReadStatus(b, bv))
            | (Event::WriteStatus(a, av), Event::WriteStatus(b, bv)) => {
                av == bv && self.value(*a, *b)
            }
            (Event::ReadCallback(a, av), Event::ReadCallback(b, bv))
            | (Event::WriteCallback(a, av), Event::WriteCallback(b, bv)) => {
                self.value(*a, *b) && self.value(*av, *bv)
            }
            (Event::Apply(a, args_a), Event::Apply(b, args_b)) => {
                self.value(*a, *b) && self.values(args_a, args_b)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_id_normalization_preserves_aliasing_and_ignores_unreachable_work_nodes() {
        let mut a = Heap::default();
        let mut b = Heap::default();
        a.alloc(Object::Atom("input"));
        b.alloc(Object::Atom("input"));
        a.node(Node::Empty); // An unreachable traversal temporary.
        b.queue_work = 1;
        let ae = a.extent(None);
        let be = b.extent(None);
        a.extent(None);
        let other = b.extent(None);
        let ar = Ok(Step::Return(a.alloc(Object::Pair(ae, ae))));
        let br = Ok(Step::Return(b.alloc(Object::Pair(be, be))));
        assert!(equivalent(&a, &ar, &b, &br, 1));
        let Ok(Step::Return(pair)) = br else {
            unreachable!()
        };
        b.objects[pair] = Object::Pair(be, other);
        assert!(!equivalent(&a, &ar, &b, &br, 1));
    }

    #[test]
    fn equal_shapes_cannot_hide_input_replacement_or_an_incomplete_evaluation() {
        let mut a = Heap::default();
        let extent = a.extent(None);
        let original = a.objects.len();
        let mut b = a.clone();
        let copy = b.node(b.get_node(extent).unwrap());
        assert!(!equivalent(
            &a,
            &Ok(Step::Return(extent)),
            &b,
            &Ok(Step::Return(copy)),
            original
        ));
        for fault in [Fault::InvalidModel, Fault::Exhausted] {
            assert!(!equivalent(&a, &Err(fault), &a, &Err(fault), original));
        }
    }
}
