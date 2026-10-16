//! Typed test values. The reference semantics matches these variants directly;
//! only the plan interpreter uses the independent positional adapter below.
use super::super::ir::{Case, Family};
use crate::control::schema::{
    ComputationCase as C, ContextCase as X, ExtentCase as E, OwnerCase as O, SequenceCase as Q,
};

pub(super) type Value = usize;
pub(super) type Checked<T> = Result<T, Fault>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    Trap,
    CallbackTrap,
    InvalidModel,
    Exhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Node {
    Return(Value),
    Perform(Value),
    Bind(Value, Value),
    Resume(Value, Value, Value),
    Handle(Value, Value, Value),
    OwnedResume(Value, Value),
    Empty,
    Cons(Value, Value),
    Append(Value, Value),
    Finish(Value, Value, Value),
    RootContext(Value),
    ScopedContext {
        ret: Value,
        op: Value,
        saved: Value,
        parent: Value,
        caller: Value,
        extent: Value,
    },
    RootExtent(Value),
    ScopedExtent {
        status: Value,
        parent: Value,
    },
    Owner {
        callback: Value,
        status: Value,
    },
}

impl Node {
    pub(super) fn case(&self) -> Case {
        match self {
            Self::Return(_) => Case::Computation(C::Return),
            Self::Perform(_) => Case::Computation(C::Perform),
            Self::Bind(..) => Case::Computation(C::Bind),
            Self::Resume(..) => Case::Computation(C::Resume),
            Self::Handle(..) => Case::Computation(C::Handle),
            Self::OwnedResume(..) => Case::Computation(C::OwnedResume),
            Self::Empty => Case::Sequence(Q::Empty),
            Self::Cons(..) => Case::Sequence(Q::Cons),
            Self::Append(..) => Case::Sequence(Q::Append),
            Self::Finish(..) => Case::Sequence(Q::Finish),
            Self::RootContext(_) => Case::Context(X::Root),
            Self::ScopedContext { .. } => Case::Context(X::Scoped),
            Self::RootExtent(_) => Case::Extent(E::Root),
            Self::ScopedExtent { .. } => Case::Extent(E::Scoped),
            Self::Owner { .. } => Case::Owner(O::OneShot),
        }
    }

    pub(super) fn fields(&self) -> Vec<Value> {
        match *self {
            Self::Return(v) | Self::Perform(v) | Self::RootContext(v) | Self::RootExtent(v) => {
                vec![v]
            }
            Self::Bind(a, b) | Self::OwnedResume(a, b) | Self::Cons(a, b) | Self::Append(a, b) => {
                vec![a, b]
            }
            Self::Resume(a, b, c) | Self::Handle(a, b, c) | Self::Finish(a, b, c) => vec![a, b, c],
            Self::Empty => vec![],
            Self::ScopedContext {
                ret,
                op,
                saved,
                parent,
                caller,
                extent,
            } => vec![ret, op, saved, parent, caller, extent],
            Self::ScopedExtent { status, parent } => vec![status, parent],
            Self::Owner { callback, status } => vec![callback, status],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Object {
    Node(Node),
    Atom(&'static str),
    Status(i32),
    CallbackCell(Value),
    Function {
        label: &'static str,
        result: Checked<Value>,
    },
    Raw {
        queue: Value,
        checkpoint: Value,
    },
    Pair(Value, Value),
    Replay(Value, Value),
    Unknown(Family, u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Event {
    ReadStatus(Value, i32),
    WriteStatus(Value, i32),
    ReadCallback(Value, Value),
    WriteCallback(Value, Value),
    Apply(Value, Vec<Value>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Return(Value),
    Publish(Value),
    Advance(Vec<Value>),
}

#[derive(Clone, Debug, Default)]
pub(super) struct Heap {
    pub objects: Vec<Object>,
    pub events: Vec<Event>,
    /// The flat reference replay budgets the traversal nodes it does not build.
    /// The plan interpreter allocates those nodes instead of using this counter.
    pub queue_work: usize,
}

impl Heap {
    pub(super) fn alloc(&mut self, object: Object) -> Value {
        let id = self.objects.len();
        self.objects.push(object);
        id
    }
    pub(super) fn node(&mut self, node: Node) -> Value {
        self.alloc(Object::Node(node))
    }
    pub(super) fn get(&self, value: Value) -> Checked<&Object> {
        self.objects.get(value).ok_or(Fault::InvalidModel)
    }
    pub(super) fn get_node(&self, value: Value) -> Checked<Node> {
        match self.get(value)? {
            Object::Node(node) => Ok(node.clone()),
            Object::Unknown(..) => Err(Fault::Trap),
            _ => Err(Fault::InvalidModel),
        }
    }
    pub(super) fn extent(&mut self, parent: Option<Value>) -> Value {
        let status = self.alloc(Object::Status(0));
        self.node(match parent {
            Some(parent) => Node::ScopedExtent { status, parent },
            None => Node::RootExtent(status),
        })
    }
    pub(super) fn status(&mut self, cell: Value) -> Checked<i32> {
        let Object::Status(value) = self.get(cell)? else {
            return Err(Fault::InvalidModel);
        };
        let value = *value;
        self.events.push(Event::ReadStatus(cell, value));
        Ok(value)
    }
    pub(super) fn set_status(&mut self, cell: Value, value: i32) -> Checked<()> {
        let Some(Object::Status(status)) = self.objects.get_mut(cell) else {
            return Err(Fault::InvalidModel);
        };
        *status = value;
        self.events.push(Event::WriteStatus(cell, value));
        Ok(())
    }
    pub(super) fn callback(&mut self, cell: Value) -> Checked<Value> {
        let Object::CallbackCell(value) = self.get(cell)? else {
            return Err(Fault::InvalidModel);
        };
        let value = *value;
        self.events.push(Event::ReadCallback(cell, value));
        Ok(value)
    }
    pub(super) fn set_callback(&mut self, cell: Value, value: Value) -> Checked<()> {
        let Some(Object::CallbackCell(slot)) = self.objects.get_mut(cell) else {
            return Err(Fault::InvalidModel);
        };
        *slot = value;
        self.events.push(Event::WriteCallback(cell, value));
        Ok(())
    }
    pub(super) fn invoke(&mut self, function: Value, args: Vec<Value>) -> Checked<Value> {
        self.events.push(Event::Apply(function, args));
        match self.get(function)? {
            Object::Function { result, .. } => *result,
            _ => Err(Fault::InvalidModel),
        }
    }
}
