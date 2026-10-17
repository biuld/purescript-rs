//! Interpret each planned operation as an independent emitted-CC obligation.
use super::super::ir::{Op, Product};
use super::*;
use crate::cc::ControlHelperRole as R;
use crate::control::schema::{ContextCase, ExtentCase, SequenceCase, context};

impl Checker<'_> {
    pub(super) fn op(
        &self,
        op: &Op,
        env: &[ValueId],
        body: &mut &[Assignment],
    ) -> Checked<ValueId> {
        let operand = |value| self.operand(value, env);
        match op {
            Op::Project {
                source,
                case,
                field,
            } => self.project(
                operand(source)?,
                self.representation(*case),
                case.tag(),
                *field,
                body,
            ),
            Op::Node { case, fields } => self.node(
                self.representation(*case),
                case.tag(),
                &self.operands(fields, env)?,
                body,
            ),
            Op::Capture {
                closure,
                index,
                case,
            } => {
                let a = self.next(body)?;
                let A::ClosureGetCapture {
                    closure: actual,
                    index: slot,
                } = &a.kind
                else {
                    return Err(MISMATCH);
                };
                if slot != index {
                    return Err(MISMATCH);
                }
                self.equal(*actual, operand(closure)?)?;
                let expected = crate::cc::ValueShape::Reference(crate::cc::Reference {
                    nullable: false,
                    heap: crate::cc::RefShape::Repr(self.representation(*case)),
                });
                if self.shapes.get(&a.destination) != Some(&expected) {
                    return Err(MISMATCH);
                }
                Ok(a.destination)
            }
            Op::Apply { callee, args } => {
                let mut callee = operand(callee)?;
                let signature = self.table.control_resumption().ok_or(MISMATCH)?.signature();
                for arg in args {
                    let a = self.next(body)?;
                    let A::IndirectCall {
                        function,
                        signature: actual,
                        arguments,
                    } = &a.kind
                    else {
                        return Err(MISMATCH);
                    };
                    if *actual != signature {
                        return Err(MISMATCH);
                    }
                    self.equal(*function, callee)?;
                    self.values(arguments, &[operand(arg)?])?;
                    callee = a.destination;
                }
                Ok(callee)
            }
            Op::Helper { role, args } => self.call(*role, &self.operands(args, env)?, body),
            Op::ResumeClosure { captures } => {
                let raw = self.table.control_resumption().ok_or(MISMATCH)?;
                let a = self.next(body)?;
                let A::FunctionRef {
                    function,
                    signature,
                    captures: actual,
                } = &a.kind
                else {
                    return Err(MISMATCH);
                };
                if (*function, *signature) != (raw.function(), raw.signature()) {
                    return Err(MISMATCH);
                }
                self.values(actual, &self.operands(captures, env)?)?;
                Ok(a.destination)
            }
            Op::Pair { state, value } => {
                let pair = self.product(
                    self.schema.result_pair,
                    &[operand(state)?, operand(value)?],
                    body,
                )?;
                self.pair_conversion(pair, true, body)
            }
            Op::PairField { pair, field } => {
                let pair = self.pair_conversion(operand(pair)?, false, body)?;
                self.product_field(pair, self.schema.result_pair, *field, body)
            }
            Op::ReplayField { result, field } => {
                self.product_field(operand(result)?, self.schema.replay_result, *field, body)
            }
            Op::FreshExtent { parent } => {
                let status = self.new_status(body)?;
                let (case, fields) = match parent {
                    Some(parent) => (ExtentCase::Scoped, vec![status, operand(parent)?]),
                    None => (ExtentCase::Root, vec![status]),
                };
                self.node(self.schema.extent, case.tag(), &fields, body)
            }
            Op::ContextExtent { context } => self.context_extent(operand(context)?, body),
            Op::CheckBoundary { extent } => self.call(R::Check, &[operand(extent)?], body),
            Op::CloseBoundary { extent } => self.call(R::Close, &[operand(extent)?], body),
            Op::GuardPending { cell } => {
                let cell = operand(cell)?;
                self.guard(cell, body)?;
                Ok(cell)
            }
            Op::SetStatus { cell, value } => {
                let cell = operand(cell)?;
                let status = self.constant(*value, body)?;
                self.write(self.schema.status, cell, status, body)?;
                Ok(cell)
            }
            Op::ReadCallback { cell } => self.read(self.schema.callback, operand(cell)?, body),
            Op::WriteCallback { cell, value } => {
                let cell = operand(cell)?;
                self.write(self.schema.callback, cell, operand(value)?, body)?;
                Ok(cell)
            }
            Op::NewCallbackCell { value } => {
                self.new_cell(self.schema.callback, operand(value)?, body)
            }
            Op::NewStatus => self.new_status(body),
            Op::Product {
                product: Product::ReplayResult,
                fields,
            } => self.product(
                self.schema.replay_result,
                &self.operands(fields, env)?,
                body,
            ),
            Op::EmptySequence => {
                self.node(self.schema.sequence, SequenceCase::Empty.tag(), &[], body)
            }
        }
    }

    fn context_extent(&self, source: ValueId, body: &mut &[Assignment]) -> Checked<ValueId> {
        let tag = self.tag(source, self.schema.context, body)?;
        let a = self.next(body)?;
        let A::TagSwitch {
            value,
            cases,
            default_assignments,
            default_value,
        } = &a.kind
        else {
            return Err(MISMATCH);
        };
        self.equal(*value, tag)?;
        if cases.len() != ContextCase::ALL.len() {
            return Err(MISMATCH);
        }
        for (actual, case) in cases.iter().zip(ContextCase::ALL) {
            if actual.tag != case.tag() as i32 {
                return Err(MISMATCH);
            }
            let field = match case {
                ContextCase::Root => context::ROOT_EXTENT,
                ContextCase::Scoped => context::SCOPED_EXTENT,
            };
            let mut branch = actual.assignments.as_slice();
            let result =
                self.project(source, self.schema.context, case.tag(), field, &mut branch)?;
            self.equal(result, actual.value)?;
            self.end(&mut branch)?;
        }
        self.reject(default_assignments, *default_value)?;
        Ok(a.destination)
    }
}
