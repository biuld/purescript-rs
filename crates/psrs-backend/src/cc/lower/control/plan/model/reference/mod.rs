//! Operational semantics over typed values. No plan, Op, Cursor, field-offset
//! constants or emitter is inspected here. Replay uses the separate flat model.
mod executor;
mod replay;
use super::heap::*;
use crate::control::execution::Program;

pub(super) struct Reference<'a> {
    pub heap: &'a mut Heap,
    fuel: usize,
}

impl<'a> Reference<'a> {
    pub(super) fn new(heap: &'a mut Heap) -> Self {
        Self {
            heap,
            fuel: 100_000,
        }
    }
    pub(super) fn tick(&mut self) -> Checked<()> {
        self.fuel = self.fuel.checked_sub(1).ok_or(Fault::Exhausted)?;
        Ok(())
    }
    pub(super) fn run(&mut self, program: Program, args: &[Value]) -> Checked<Step> {
        self.tick()?;
        let returned = match (program, args) {
            (Program::Run, [state, handler, computation]) => {
                let empty = self.heap.node(Node::Empty);
                let root = self.heap.extent(None);
                let context = self.heap.node(Node::RootContext(root));
                let mut current = vec![*state, *handler, *computation, empty, root, context];
                let result = loop {
                    let args: [Value; 6] = current.try_into().map_err(|_| Fault::InvalidModel)?;
                    match self.step(args)? {
                        Step::Advance(next) => current = next,
                        Step::Publish(result) => break result,
                        _ => return Err(Fault::InvalidModel),
                    }
                };
                self.close(root)?;
                result
            }
            (Program::Executor, [s, h, c, q, e, x]) => return self.step([*s, *h, *c, *q, *e, *x]),
            (Program::CloneSequence, [queue, todo, reversed, active, parent]) => {
                self.clone_sequence(*queue, *todo, *reversed, *active, *parent)?
            }
            (Program::RebuildSequence, [reversed, result, active, parent]) => {
                self.rebuild_sequence(*reversed, *result, *active, *parent)?
            }
            (Program::Check, [extent]) => self.check(*extent)?,
            (Program::Close, [extent]) => {
                self.close(*extent)?;
                *extent
            }
            (Program::Start, []) => self.heap.extent(None),
            (Program::Once, [callback]) => {
                let callback = self.heap.alloc(Object::CallbackCell(*callback));
                let status = self.heap.alloc(Object::Status(0));
                self.heap.node(Node::Owner { callback, status })
            }
            (Program::Cancel, [owner]) => {
                let Node::Owner { callback, status } = self.heap.get_node(*owner)? else {
                    return Err(Fault::InvalidModel);
                };
                if self.heap.status(status)? != 0 {
                    return Err(Fault::Trap);
                }
                self.heap.set_status(status, 3)?;
                self.heap.set_callback(callback, status)?;
                *owner
            }
            (Program::RawResume, [receiver, value]) => {
                let Object::Raw { queue, checkpoint } = *self.heap.get(*receiver)? else {
                    return Err(Fault::InvalidModel);
                };
                self.check(checkpoint)?;
                self.heap.node(Node::Resume(*value, queue, checkpoint))
            }
            _ => return Err(Fault::InvalidModel),
        };
        Ok(Step::Return(returned))
    }

    pub(super) fn check(&mut self, mut extent: Value) -> Checked<Value> {
        loop {
            self.tick()?;
            let (status, parent) = match self.heap.get_node(extent)? {
                Node::RootExtent(status) => (status, None),
                Node::ScopedExtent { status, parent } => (status, Some(parent)),
                _ => return Err(Fault::InvalidModel),
            };
            if self.heap.status(status)? != 0 {
                return Err(Fault::Trap);
            }
            match parent {
                Some(parent) => extent = parent,
                None => return Ok(extent),
            }
        }
    }

    fn close(&mut self, extent: Value) -> Checked<()> {
        match self.heap.get_node(extent)? {
            Node::RootExtent(status) | Node::ScopedExtent { status, .. } => {
                self.heap.set_status(status, 1)
            }
            _ => Err(Fault::InvalidModel),
        }
    }

    fn dispatch_extent(&self, context: Value) -> Checked<Value> {
        match self.heap.get_node(context)? {
            Node::RootContext(extent) | Node::ScopedContext { extent, .. } => Ok(extent),
            _ => Err(Fault::InvalidModel),
        }
    }

    fn apply(&mut self, callee: Value, args: Vec<Value>) -> Checked<Value> {
        if matches!(self.heap.get(callee)?, Object::Raw { .. }) {
            self.heap.events.push(Event::Apply(callee, args.clone()));
            let Step::Return(value) = self.run(
                Program::RawResume,
                &[
                    callee,
                    *args
                        .first()
                        .filter(|_| args.len() == 1)
                        .ok_or(Fault::InvalidModel)?,
                ],
            )?
            else {
                return Err(Fault::InvalidModel);
            };
            Ok(value)
        } else {
            self.heap.invoke(callee, args)
        }
    }
}
