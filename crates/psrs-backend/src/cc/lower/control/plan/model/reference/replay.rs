//! A flat occurrence-list replay specification, independent of the plan's
//! traversal-stack/reversed-node construction. Callback and owner references
//! are never copied; only activation extents and queue structure are fresh.
use super::super::heap::*;
use super::Reference;

#[derive(Clone, Copy, Debug)]
pub(super) enum Frame {
    Callback(Value),
    Finish,
}

impl Reference<'_> {
    fn flatten(&mut self, queue: Value, output: &mut Vec<Frame>) -> Checked<()> {
        self.tick()?;
        match self.heap.get_node(queue)? {
            Node::Empty => {}
            Node::Cons(callback, rest) => {
                self.heap.queue_work += 1;
                output.push(Frame::Callback(callback));
                self.flatten(rest, output)?;
            }
            Node::Append(left, right) => {
                self.heap.queue_work += 1;
                self.flatten(left, output)?;
                self.flatten(right, output)?;
            }
            Node::Finish(extent, rest, _) => {
                self.check(extent)?;
                self.heap.queue_work += 1;
                output.push(Frame::Finish);
                self.flatten(rest, output)?;
            }
            _ => return Err(Fault::InvalidModel),
        }
        Ok(())
    }

    fn linear(&mut self, mut queue: Value) -> (Vec<Frame>, Checked<()>) {
        let mut frames = Vec::new();
        loop {
            if let Err(error) = self.tick() {
                return (frames, Err(error));
            }
            match self.heap.get_node(queue) {
                Ok(Node::Empty) => return (frames, Ok(())),
                Ok(Node::Cons(callback, rest)) => {
                    frames.push(Frame::Callback(callback));
                    queue = rest;
                }
                Ok(Node::Finish(_, rest, _)) => {
                    frames.push(Frame::Finish);
                    queue = rest;
                }
                Ok(Node::Append(..)) => return (frames, Err(Fault::Trap)),
                Err(error) => return (frames, Err(error)),
                _ => return (frames, Err(Fault::InvalidModel)),
            }
        }
    }

    pub(super) fn clone_sequence(
        &mut self,
        queue: Value,
        mut todo: Value,
        reversed: Value,
        active: Value,
        parent: Value,
    ) -> Checked<Value> {
        let mut frames = Vec::new();
        self.flatten(queue, &mut frames)?;
        loop {
            self.tick()?;
            match self.heap.get_node(todo)? {
                Node::Empty => break,
                Node::Append(next, rest) => {
                    self.flatten(next, &mut frames)?;
                    todo = rest;
                }
                Node::Cons(..) | Node::Finish(..) => return Err(Fault::Trap),
                _ => return Err(Fault::InvalidModel),
            }
        }
        let mut reversed_frames = frames.into_iter().rev().collect::<Vec<_>>();
        let (prefix, end) = self.linear(reversed);
        reversed_frames.extend(prefix);
        self.rebuild_frames(&reversed_frames, todo, active, parent, end)
    }

    pub(super) fn rebuild_sequence(
        &mut self,
        reversed: Value,
        result: Value,
        active: Value,
        parent: Value,
    ) -> Checked<Value> {
        let (frames, end) = self.linear(reversed);
        self.rebuild_frames(&frames, result, active, parent, end)
    }

    fn rebuild_frames(
        &mut self,
        frames: &[Frame],
        mut result: Value,
        mut active: Value,
        mut parent: Value,
        end: Checked<()>,
    ) -> Checked<Value> {
        for frame in frames {
            self.tick()?;
            result = match *frame {
                Frame::Callback(callback) => self.heap.node(Node::Cons(callback, result)),
                Frame::Finish => {
                    let fresh = self.heap.extent(Some(parent));
                    let next = self.heap.node(Node::Finish(fresh, result, active));
                    active = fresh;
                    parent = fresh;
                    next
                }
            };
        }
        end?;
        Ok(self.heap.alloc(Object::Replay(result, active)))
    }
}
