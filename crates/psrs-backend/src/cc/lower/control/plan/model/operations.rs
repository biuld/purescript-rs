//! Interpret plan operands and instructions; no reference transition is used.
use super::super::ir::{Case, Op, Product};
use super::heap::*;
use super::interpret::{Interpreter, program};
use crate::control::execution::Program;
use crate::control::schema::{
    ComputationCase as C, ContextCase as X, ExtentCase as E, OwnerCase as O, SequenceCase as Q,
};

impl Interpreter {
    pub(super) fn op(&mut self, op: &Op, params: &[Value], env: &[Value]) -> Checked<Value> {
        self.tick()?;
        let value = |o| Self::operand(o, params, env);
        match op {
            Op::Protect {
                action,
                ops,
                result,
                cleanup,
            } => {
                let mut scope = env.to_vec();
                let attempt = (|| {
                    for op in ops {
                        let value = self.op(op, params, &scope)?;
                        scope.push(value);
                    }
                    Self::operand(result, params, &scope)
                })();
                if matches!(attempt, Err(Fault::Channel(_))) {
                    self.exceptional_actions.insert(*action);
                    let mut scope = env.to_vec();
                    for op in cleanup {
                        let value = self.op(op, params, &scope)?;
                        scope.push(value);
                    }
                }
                attempt
            }
            Op::Project {
                source,
                case,
                field,
            } => {
                let node = self.heap.get_node(value(source)?)?;
                if node.case() != *case {
                    return Err(Fault::InvalidModel);
                }
                node.fields()
                    .get(*field as usize)
                    .copied()
                    .ok_or(Fault::InvalidModel)
            }
            Op::Node { case, fields } => {
                let fields = Self::operands(fields, params, env)?;
                let node = node(*case, &fields)?;
                Ok(self.heap.node(node))
            }
            Op::Capture { closure, index, .. } => {
                let Object::Raw { queue, checkpoint } = self.heap.get(value(closure)?)? else {
                    return Err(Fault::InvalidModel);
                };
                match *index {
                    0 => Ok(*queue),
                    1 => Ok(*checkpoint),
                    _ => Err(Fault::InvalidModel),
                }
            }
            Op::Apply { callee, args } => {
                self.apply(value(callee)?, Self::operands(args, params, env)?)
            }
            Op::Helper { role, args } => {
                self.call(program(*role), Self::operands(args, params, env)?)
            }
            Op::ResumeClosure { captures } => {
                let args = Self::operands(captures, params, env)?;
                let [queue, checkpoint] = args.as_slice() else {
                    return Err(Fault::InvalidModel);
                };
                Ok(self.heap.alloc(Object::Raw {
                    queue: *queue,
                    checkpoint: *checkpoint,
                }))
            }
            Op::Pair {
                state,
                value: answer,
            } => Ok(self.heap.alloc(Object::Pair(value(state)?, value(answer)?))),
            Op::PairField { pair, field } => {
                let Object::Pair(state, answer) = self.heap.get(value(pair)?)? else {
                    return Err(Fault::InvalidModel);
                };
                match *field {
                    0 => Ok(*state),
                    1 => Ok(*answer),
                    _ => Err(Fault::InvalidModel),
                }
            }
            Op::ReplayField { result, field } => {
                let Object::Replay(queue, active) = self.heap.get(value(result)?)? else {
                    return Err(Fault::InvalidModel);
                };
                match *field {
                    0 => Ok(*queue),
                    1 => Ok(*active),
                    _ => Err(Fault::InvalidModel),
                }
            }
            Op::FreshExtent { parent } => {
                Ok(self.heap.extent(parent.as_ref().map(value).transpose()?))
            }
            Op::ContextExtent { context } => match self.heap.get_node(value(context)?)? {
                Node::RootContext(extent) | Node::ScopedContext { extent, .. } => Ok(extent),
                _ => Err(Fault::InvalidModel),
            },
            Op::CheckBoundary { extent } => self.call(Program::Check, vec![value(extent)?]),
            Op::CloseBoundary { extent } => self.call(Program::Close, vec![value(extent)?]),
            Op::GuardPending { cell } => {
                let cell = value(cell)?;
                if self.heap.status(cell)? != 0 {
                    return Err(Fault::Trap);
                }
                Ok(cell)
            }
            Op::SetStatus {
                cell,
                value: status,
            } => {
                let cell = value(cell)?;
                self.heap.set_status(cell, *status)?;
                Ok(cell)
            }
            Op::ReadCallback { cell } => self.heap.callback(value(cell)?),
            Op::WriteCallback {
                cell,
                value: replacement,
            } => {
                let cell = value(cell)?;
                self.heap.set_callback(cell, value(replacement)?)?;
                Ok(cell)
            }
            Op::NewCallbackCell { value: callback } => {
                Ok(self.heap.alloc(Object::CallbackCell(value(callback)?)))
            }
            Op::NewStatus => Ok(self.heap.alloc(Object::Status(0))),
            Op::Product {
                product: Product::ReplayResult,
                fields,
            } => {
                let fields = Self::operands(fields, params, env)?;
                let [queue, active] = fields.as_slice() else {
                    return Err(Fault::InvalidModel);
                };
                Ok(self.heap.alloc(Object::Replay(*queue, *active)))
            }
            Op::EmptySequence => Ok(self.heap.node(Node::Empty)),
        }
    }
}

fn node(case: Case, fields: &[Value]) -> Checked<Node> {
    Ok(match (case, fields) {
        (Case::Computation(C::Return), [a]) => Node::Return(*a),
        (Case::Computation(C::Perform), [a]) => Node::Perform(*a),
        (Case::Computation(C::Bind), [a, b]) => Node::Bind(*a, *b),
        (Case::Computation(C::Resume), [a, b, c]) => Node::Resume(*a, *b, *c),
        (Case::Computation(C::Handle), [a, b, c]) => Node::Handle(*a, *b, *c),
        (Case::Computation(C::OwnedResume), [a, b]) => Node::OwnedResume(*a, *b),
        (Case::Sequence(Q::Empty), []) => Node::Empty,
        (Case::Sequence(Q::Cons), [a, b]) => Node::Cons(*a, *b),
        (Case::Sequence(Q::Append), [a, b]) => Node::Append(*a, *b),
        (Case::Sequence(Q::Finish), [a, b, c]) => Node::Finish(*a, *b, *c),
        (Case::Context(X::Root), [a]) => Node::RootContext(*a),
        (Case::Context(X::Scoped), [a, b, c, d, e, f]) => Node::ScopedContext {
            ret: *a,
            op: *b,
            saved: *c,
            parent: *d,
            caller: *e,
            extent: *f,
        },
        (Case::Extent(E::Root), [a]) => Node::RootExtent(*a),
        (Case::Extent(E::Scoped), [a, b]) => Node::ScopedExtent {
            status: *a,
            parent: *b,
        },
        (Case::Owner(O::OneShot), [a, b]) => Node::Owner {
            callback: *a,
            status: *b,
        },
        _ => return Err(Fault::InvalidModel),
    })
}
