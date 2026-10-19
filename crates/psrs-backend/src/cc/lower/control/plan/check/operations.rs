//! Logical type and provenance preconditions of construction operations.
use super::super::ir::{Op, Product};
use super::*;
use crate::control::schema::{ExtentCase, SequenceCase};

impl Scope {
    pub(super) fn op(&self, op: &Op) -> Checked<Info> {
        use FieldKind as K;
        let kind = match op {
            Op::Protect {
                action,
                ops,
                result,
                cleanup,
            } => {
                if action.program() != self.program {
                    return Err("exception cleanup has the wrong machine action owner");
                }
                let mut body = self.clone();
                for op in ops {
                    let info = body.op(op)?;
                    body.temps.push(info);
                }
                let result = body.info(result)?;
                let mut handler = self.clone();
                for op in cleanup {
                    if !matches!(op, Op::CloseBoundary { .. } | Op::SetStatus { .. }) {
                        return Err("exception cleanup must contain only private lifecycle stores");
                    }
                    let info = handler.op(op)?;
                    handler.temps.push(info);
                }
                return Ok(result);
            }
            Op::Project {
                source,
                case,
                field,
            } => return self.projection(source, *case, *field),
            Op::Node { case, fields } => {
                self.arguments(fields, case.fields())?;
                return Ok(Info::node(*case));
            }
            Op::Capture {
                closure,
                index,
                case,
            } => {
                if self.program != Program::RawResume {
                    return Err("only raw resume may read machine closure captures");
                }
                self.require(closure, K::Callable)?;
                let expected = match *index {
                    0 => K::Sequence,
                    1 => K::Extent,
                    _ => return Err("raw resume capture index is outside its protocol"),
                };
                if case.kind() != expected {
                    return Err("raw resume capture has the wrong carrier role");
                }
                expected
            }
            Op::Apply { callee, args } => {
                self.require(callee, K::Callable)?;
                if args.is_empty() {
                    return Err("machine callback application requires an argument");
                }
                for arg in args {
                    self.info(arg)?;
                }
                K::Payload
            }
            Op::Helper { role, args } => {
                self.arguments(args, role.parameters())?;
                role.result()
            }
            Op::ResumeClosure { captures } => {
                self.arguments(captures, &[K::Sequence, K::Extent])?;
                K::Callable
            }
            Op::Pair { state, value } => {
                self.info(state)?;
                self.info(value)?;
                K::ResultPair
            }
            Op::PairField { pair, field } => {
                self.require(pair, K::ResultPair)?;
                if *field > 1 {
                    return Err("machine result pair field is out of range");
                }
                K::Payload
            }
            Op::ReplayField { result, field } => {
                self.require(result, K::ReplayResult)?;
                match *field {
                    0 => K::Sequence,
                    1 => K::Extent,
                    _ => return Err("machine replay result field is out of range"),
                }
            }
            Op::FreshExtent { parent } => {
                if let Some(parent) = parent {
                    self.require(parent, K::Extent)?;
                }
                return Ok(Info::node(Case::Extent(if parent.is_some() {
                    ExtentCase::Scoped
                } else {
                    ExtentCase::Root
                })));
            }
            Op::ContextExtent { context } => {
                self.require(context, K::Context)?;
                K::Extent
            }
            Op::CheckBoundary { extent } | Op::CloseBoundary { extent } => {
                self.require(extent, K::Extent)?;
                K::Extent
            }
            Op::GuardPending { cell } => {
                self.require(cell, K::Status)?;
                K::Status
            }
            Op::SetStatus { cell, value } => {
                self.require(cell, K::Status)?;
                if !(0..=3).contains(value) {
                    return Err("machine status write is outside the lifecycle domain");
                }
                K::Status
            }
            Op::ReadCallback { cell } => {
                self.require(cell, K::Callback)?;
                K::Payload
            }
            Op::WriteCallback { cell, value } => {
                self.require(cell, K::Callback)?;
                self.info(value)?;
                K::Callback
            }
            Op::NewCallbackCell { value } => {
                self.require(value, K::Callable)?;
                K::Callback
            }
            Op::NewStatus => K::Status,
            Op::Product {
                product: Product::ReplayResult,
                fields,
            } => {
                self.arguments(fields, &[K::Sequence, K::Extent])?;
                K::ReplayResult
            }
            Op::EmptySequence => return Ok(Info::node(Case::Sequence(SequenceCase::Empty))),
        };
        Ok(Info::new(kind))
    }
}
