//! Direct operational transitions over typed nodes.
use super::super::heap::*;
use super::Reference;

impl Reference<'_> {
    pub(super) fn step(&mut self, mut state: [Value; 6]) -> Checked<Step> {
        let [s, h, computation, queue, extent, context] = state;
        match self.heap.get_node(computation)? {
            Node::Return(value) => match self.heap.get_node(queue)? {
                Node::Empty => match self.heap.get_node(context)? {
                    Node::RootContext(_) => {
                        return Ok(Step::Publish(self.heap.alloc(Object::Pair(s, value))));
                    }
                    Node::ScopedContext { ret, .. } => {
                        let target = self.apply(ret, vec![value])?;
                        return self.enter_target(state, target);
                    }
                    _ => return Err(Fault::InvalidModel),
                },
                Node::Cons(callback, rest) => {
                    state[2] = self.apply(callback, vec![value])?;
                    state[3] = rest;
                }
                Node::Append(left, right) => return self.append_return(state, value, left, right),
                Node::Finish(owned, rest, parent) => {
                    self.close(owned)?;
                    state[3] = rest;
                    state[4] = parent;
                }
                _ => return Err(Fault::InvalidModel),
            },
            Node::Perform(request) => {
                let checkpoint = self.dispatch_extent(context)?;
                let raw = self.heap.alloc(Object::Raw { queue, checkpoint });
                match self.heap.get_node(context)? {
                    Node::RootContext(_) => {
                        let pair = self.apply(h, vec![s, request, raw])?;
                        let Object::Pair(next_state, next) = *self.heap.get(pair)? else {
                            return Err(Fault::InvalidModel);
                        };
                        state[0] = next_state;
                        state[2] = next;
                        state[3] = self.heap.node(Node::Empty);
                    }
                    Node::ScopedContext { op, .. } => {
                        let target = self.apply(op, vec![request, raw])?;
                        return self.enter_target(state, target);
                    }
                    _ => return Err(Fault::InvalidModel),
                }
            }
            Node::Bind(inner, callback) => {
                state[2] = inner;
                state[3] = self.heap.node(Node::Cons(callback, queue));
            }
            Node::Resume(value, saved, checkpoint) => {
                self.check(checkpoint)?;
                let parent = self.dispatch_extent(context)?;
                let empty = self.heap.node(Node::Empty);
                let replay = self.clone_sequence(saved, empty, empty, extent, parent)?;
                let Object::Replay(replayed, activation) = *self.heap.get(replay)? else {
                    return Err(Fault::InvalidModel);
                };
                state[2] = self.heap.node(Node::Return(value));
                state[3] = self.heap.node(Node::Append(replayed, queue));
                state[4] = activation;
            }
            Node::Handle(ret, op, inner) => {
                if let Node::Append(left, right) = self.heap.get_node(queue)? {
                    return self.normalize_handle(state, left, right);
                }
                let (saved, parent, owned) = match self.heap.get_node(queue)? {
                    Node::Finish(owned, rest, parent) => (rest, parent, owned),
                    Node::Empty | Node::Cons(..) => (queue, extent, self.heap.extent(Some(extent))),
                    _ => return Err(Fault::InvalidModel),
                };
                state[2] = inner;
                state[3] = self.heap.node(Node::Empty);
                state[4] = owned;
                state[5] = self.heap.node(Node::ScopedContext {
                    ret,
                    op,
                    saved,
                    parent,
                    caller: context,
                    extent: owned,
                });
            }
            Node::OwnedResume(owner, value) => {
                let Node::Owner { callback, status } = self.heap.get_node(owner)? else {
                    return Err(Fault::InvalidModel);
                };
                if self.heap.status(status)? != 0 {
                    return Err(Fault::Trap);
                }
                self.heap.set_status(status, 1)?;
                let callback_value = self.heap.callback(callback)?;
                self.heap.set_callback(callback, status)?;
                state[2] = self.apply(callback_value, vec![value])?;
                self.heap.set_status(status, 2)?;
            }
            _ => return Err(Fault::InvalidModel),
        }
        Ok(Step::Advance(state.into()))
    }

    fn enter_target(&mut self, mut state: [Value; 6], target: Value) -> Checked<Step> {
        let Node::ScopedContext {
            saved,
            parent,
            caller,
            extent,
            ..
        } = self.heap.get_node(state[5])?
        else {
            return Err(Fault::InvalidModel);
        };
        state[2] = target;
        state[3] = self.heap.node(Node::Finish(extent, saved, parent));
        state[4] = extent;
        state[5] = caller;
        Ok(Step::Advance(state.into()))
    }

    fn append_return(
        &mut self,
        mut state: [Value; 6],
        value: Value,
        left: Value,
        right: Value,
    ) -> Checked<Step> {
        state[3] = match self.heap.get_node(left)? {
            Node::Empty => right,
            Node::Cons(callback, rest) => {
                let queue = self.heap.node(Node::Append(rest, right));
                state[2] = self.apply(callback, vec![value])?;
                queue
            }
            Node::Append(first, second) => {
                let tail = self.heap.node(Node::Append(second, right));
                self.heap.node(Node::Append(first, tail))
            }
            Node::Finish(owned, rest, parent) => {
                let tail = self.heap.node(Node::Append(rest, right));
                self.heap.node(Node::Finish(owned, tail, parent))
            }
            _ => return Err(Fault::InvalidModel),
        };
        Ok(Step::Advance(state.into()))
    }

    fn normalize_handle(
        &mut self,
        mut state: [Value; 6],
        left: Value,
        right: Value,
    ) -> Checked<Step> {
        state[3] = match self.heap.get_node(left)? {
            Node::Empty => right,
            Node::Cons(callback, rest) => {
                let tail = self.heap.node(Node::Append(rest, right));
                self.heap.node(Node::Cons(callback, tail))
            }
            Node::Append(first, second) => {
                let tail = self.heap.node(Node::Append(second, right));
                self.heap.node(Node::Append(first, tail))
            }
            Node::Finish(owned, rest, parent) => {
                let tail = self.heap.node(Node::Append(rest, right));
                self.heap.node(Node::Finish(owned, tail, parent))
            }
            _ => return Err(Fault::InvalidModel),
        };
        Ok(Step::Advance(state.into()))
    }
}
