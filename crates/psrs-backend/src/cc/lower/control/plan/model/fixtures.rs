//! Distinct values for every semantic role; execution, dispatch checkpoint and
//! restoration parent deliberately differ. Cases are generated from inputs,
//! not from the plan or its action-path registry.
use super::heap::*;

#[derive(Clone)]
pub(super) struct Fixture {
    pub heap: Heap,
    pub state: Value,
    pub handler: Value,
    pub answer: Value,
    pub request: Value,
    pub next: Value,
    pub ret: Value,
    pub op: Value,
    pub callback: Value,
    pub empty: Value,
    pub consumer: Value,
    pub saved: Value,
    pub active: Value,
    pub checkpoint: Value,
    pub parent: Value,
    pub root: Value,
    pub scoped: Value,
    pub owner: Value,
    pub raw: Value,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let mut heap = Heap::default();
        let state = heap.alloc(Object::Atom("state"));
        let next_state = heap.alloc(Object::Atom("next-state"));
        let answer = heap.alloc(Object::Atom("answer"));
        let request = heap.alloc(Object::Atom("request"));
        let payload = heap.alloc(Object::Atom("callback-answer"));
        let next = heap.node(Node::Return(payload));
        let pair = heap.alloc(Object::Pair(next_state, next));
        let handler = heap.alloc(Object::Function {
            label: "root-handler",
            result: Ok(pair),
        });
        let ret = heap.alloc(Object::Function {
            label: "return-clause",
            result: Ok(next),
        });
        let op = heap.alloc(Object::Function {
            label: "operation-clause",
            result: Ok(next),
        });
        let callback = heap.alloc(Object::Function {
            label: "queue-callback",
            result: Ok(next),
        });
        let empty = heap.node(Node::Empty);
        let active = heap.extent(None);
        let checkpoint = heap.extent(None);
        let parent = heap.extent(None);
        let root = heap.node(Node::RootContext(checkpoint));
        let consumer = heap.node(Node::Cons(op, empty));
        let saved = heap.node(Node::Cons(ret, empty));
        let scoped = heap.node(Node::ScopedContext {
            ret,
            op,
            saved,
            parent,
            caller: root,
            extent: checkpoint,
        });
        let callback_slot = heap.alloc(Object::CallbackCell(callback));
        let status = heap.alloc(Object::Status(0));
        let owner = heap.node(Node::Owner {
            callback: callback_slot,
            status,
        });
        let raw = heap.alloc(Object::Raw {
            queue: saved,
            checkpoint,
        });
        Self {
            heap,
            state,
            handler,
            answer,
            request,
            next,
            ret,
            op,
            callback,
            empty,
            consumer,
            saved,
            active,
            checkpoint,
            parent,
            root,
            scoped,
            owner,
            raw,
        }
    }
    pub(super) fn args(&self, computation: Value, queue: Value, context: Value) -> Vec<Value> {
        vec![
            self.state,
            self.handler,
            computation,
            queue,
            self.active,
            context,
        ]
    }
    pub(super) fn owner_status(&mut self, n: i32) {
        let Node::Owner { status, .. } = self.heap.get_node(self.owner).unwrap() else {
            unreachable!()
        };
        self.heap.objects[status] = Object::Status(n);
    }
    pub(super) fn expire(&mut self, extent: Value) {
        let status = match self.heap.get_node(extent).unwrap() {
            Node::RootExtent(status) | Node::ScopedExtent { status, .. } => status,
            _ => unreachable!(),
        };
        self.heap.objects[status] = Object::Status(1);
    }
    pub(super) fn queues(&mut self) -> Vec<Value> {
        let cons = self.heap.node(Node::Cons(self.callback, self.saved));
        let finish = self
            .heap
            .node(Node::Finish(self.checkpoint, self.saved, self.parent));
        let nested = self.heap.node(Node::Append(self.saved, self.consumer));
        let mut queues = vec![self.empty, cons, finish];
        for left in [self.empty, cons, nested, finish] {
            queues.push(self.heap.node(Node::Append(left, self.consumer)));
        }
        queues
    }
}

/// Queue expressions through depth three, with two shared callback identities
/// and repeated Finish extent occurrences. Both Append subtrees are generated.
#[derive(Clone, Debug)]
pub(super) enum Queue {
    Empty,
    Cons(bool, Box<Self>),
    Finish(Box<Self>),
    Append(Box<Self>, Box<Self>),
}

pub(super) fn queues(depth: usize) -> Vec<Queue> {
    if depth == 0 {
        return vec![Queue::Empty];
    }
    let previous = queues(depth - 1);
    let mut result = vec![Queue::Empty];
    for rest in &previous {
        result.push(Queue::Cons(false, Box::new(rest.clone())));
        result.push(Queue::Cons(true, Box::new(rest.clone())));
        result.push(Queue::Finish(Box::new(rest.clone())));
    }
    for left in &previous {
        for right in &previous {
            result.push(Queue::Append(
                Box::new(left.clone()),
                Box::new(right.clone()),
            ));
        }
    }
    result
}

impl Queue {
    pub(super) fn counts(&self) -> (usize, usize, usize) {
        match self {
            Self::Empty => (0, 0, 0),
            Self::Cons(_, rest) => {
                let (c, f, a) = rest.counts();
                (c + 1, f, a)
            }
            Self::Finish(rest) => {
                let (c, f, a) = rest.counts();
                (c, f + 1, a)
            }
            Self::Append(left, right) => {
                let (c, f, a) = left.counts();
                let (c2, f2, a2) = right.counts();
                (c + c2, f + f2, a + a2 + 1)
            }
        }
    }
}

impl Fixture {
    pub(super) fn queue(&mut self, queue: &Queue) -> Value {
        match queue {
            Queue::Empty => self.empty,
            Queue::Cons(alternative, rest) => {
                let rest = self.queue(rest);
                self.heap.node(Node::Cons(
                    if *alternative { self.op } else { self.callback },
                    rest,
                ))
            }
            Queue::Finish(rest) => {
                let rest = self.queue(rest);
                self.heap
                    .node(Node::Finish(self.checkpoint, rest, self.parent))
            }
            Queue::Append(left, right) => {
                let left = self.queue(left);
                let right = self.queue(right);
                self.heap.node(Node::Append(left, right))
            }
        }
    }
}
