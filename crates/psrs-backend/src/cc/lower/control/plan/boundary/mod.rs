//! Independent, ordered validation of ordinary CC against its construction plan.
//!
//! Casts may bridge planned reference shapes; generic CC verification checks
//! their legality. Every other assignment must implement the next planned
//! operation, including unused effects and trapping defaults. This is a P8
//! construction check, not a validator for arbitrary optimized programs.

mod ops;
mod primitives;

use super::ir::{Case, Exec, Operand, state_index};
use crate::cc::branch_facts::Definitions;
use crate::cc::{
    Assignment, AssignmentKind as A, ControlSchema, Function, ReprId, RepresentationTable, ValueId,
};

type Checked<T> = Result<T, &'static str>;
const MISMATCH: &str =
    "emitted machine operands or ordered operations disagree with the checked plan";

pub(crate) fn check_boundary(
    function: &Function,
    table: &RepresentationTable,
    plan: &super::CheckedPlan,
) -> Checked<()> {
    let checker = Checker {
        table,
        schema: table
            .control_schema()
            .ok_or("boundary check has no checked machine schema")?,
        definitions: crate::cc::value_identity::definitions(&function.assignments),
        params: &function.parameters,
        shapes: function
            .values
            .iter()
            .map(|value| (value.id, value.ty))
            .collect(),
        used_casts: std::cell::RefCell::new(std::collections::HashSet::new()),
        pending_casts: std::cell::RefCell::new(Vec::new()),
    };
    let mut body = function.assignments.as_slice();
    let result = checker.exec(plan.exec(), &mut Vec::new(), &mut body)?;
    checker.equal(result, function.result)?;
    checker.end(&mut body)?;
    Ok(())
}

struct Checker<'a> {
    table: &'a RepresentationTable,
    schema: ControlSchema,
    definitions: Definitions<'a>,
    params: &'a [ValueId],
    shapes: std::collections::HashMap<ValueId, crate::cc::ValueShape>,
    used_casts: std::cell::RefCell<std::collections::HashSet<ValueId>>,
    pending_casts: std::cell::RefCell<Vec<ValueId>>,
}

impl Checker<'_> {
    fn strip(&self, mut value: ValueId) -> Checked<ValueId> {
        let mut active = std::collections::HashSet::new();
        while let Some(A::RepresentationCast { value: source, .. }) =
            self.definitions.get(&value).copied()
        {
            if !active.insert(value) {
                return Err(MISMATCH);
            }
            value = *source;
        }
        Ok(value)
    }

    fn equal(&self, a: ValueId, b: ValueId) -> Checked<()> {
        self.used_casts.borrow_mut().extend([a, b]);
        if self.strip(a)? == self.strip(b)? {
            Ok(())
        } else {
            Err(MISMATCH)
        }
    }

    fn values(&self, actual: &[ValueId], expected: &[ValueId]) -> Checked<()> {
        if actual.len() != expected.len() {
            return Err(MISMATCH);
        }
        for (a, b) in actual.iter().zip(expected) {
            self.equal(*a, *b)?;
        }
        Ok(())
    }

    fn operand(&self, value: &Operand, env: &[ValueId]) -> Checked<ValueId> {
        match value {
            Operand::State(slot) => self.params.get(state_index(*slot)),
            Operand::Param(index) => self.params.get(*index),
            Operand::Temp(index) => env.get(*index),
        }
        .copied()
        .ok_or(MISMATCH)
    }

    fn operands(&self, values: &[Operand], env: &[ValueId]) -> Checked<Vec<ValueId>> {
        values
            .iter()
            .map(|value| self.operand(value, env))
            .collect()
    }

    fn representation(&self, case: Case) -> ReprId {
        match case {
            Case::Computation(_) => self.schema.computation,
            Case::Sequence(_) => self.schema.sequence,
            Case::Context(_) => self.schema.context,
            Case::Extent(_) => self.schema.extent,
            Case::Owner(_) => self.schema.owner,
        }
    }

    fn skip_casts(&self, body: &mut &[Assignment]) {
        while let Some(Assignment {
            destination,
            kind: A::RepresentationCast {
                value, reference, ..
            },
            ..
        }) = body.first()
        {
            let target = crate::cc::ValueShape::Reference(*reference);
            // Erasure and same-shape aliases are total. A recovery must feed
            // the immediately following checked operation or actual exit;
            // it cannot move across a guard, store or user callback.
            if target != crate::cc::payload::erased_shape()
                && self.shapes.get(value) != Some(&target)
            {
                self.pending_casts.borrow_mut().push(*destination);
            }
            *body = &body[1..];
        }
    }

    fn consumed_casts(&self) -> Checked<()> {
        if self
            .pending_casts
            .borrow()
            .iter()
            .any(|value| !self.used_casts.borrow().contains(value))
        {
            return Err(MISMATCH);
        }
        self.pending_casts.borrow_mut().clear();
        Ok(())
    }

    fn next<'a>(&self, body: &mut &'a [Assignment]) -> Checked<&'a Assignment> {
        self.consumed_casts()?;
        self.skip_casts(body);
        let (next, rest) = body.split_first().ok_or(MISMATCH)?;
        *body = rest;
        Ok(next)
    }

    fn end(&self, body: &mut &[Assignment]) -> Checked<()> {
        self.consumed_casts()?;
        self.skip_casts(body);
        self.consumed_casts()?;
        if body.is_empty() {
            Ok(())
        } else {
            Err(MISMATCH)
        }
    }

    fn reject(&self, body: &[Assignment], result: ValueId) -> Checked<()> {
        let mut body = body;
        let assignment = self.next(&mut body)?;
        if !matches!(assignment.kind, A::Unreachable) {
            return Err(MISMATCH);
        }
        self.equal(assignment.destination, result)?;
        self.end(&mut body)
    }

    fn exec(
        &self,
        plan: &Exec,
        env: &mut Vec<ValueId>,
        body: &mut &[Assignment],
    ) -> Checked<ValueId> {
        match plan {
            Exec::Steps { ops, next } => {
                let base = env.len();
                for op in ops {
                    let value = self.op(op, env, body)?;
                    env.push(value);
                }
                let value = self.exec(next, env, body)?;
                env.truncate(base);
                Ok(value)
            }
            Exec::Switch { source, case, arms } => {
                let source = self.operand(source, env)?;
                let tag = self.tag(source, self.representation(*case), body)?;
                let assignment = self.next(body)?;
                let A::TagSwitch {
                    value,
                    cases,
                    default_assignments,
                    default_value,
                } = &assignment.kind
                else {
                    return Err(MISMATCH);
                };
                self.equal(*value, tag)?;
                if cases.len() != arms.len() {
                    return Err(MISMATCH);
                }
                for (actual, (tag, arm)) in cases.iter().zip(arms) {
                    if actual.tag != *tag as i32 {
                        return Err(MISMATCH);
                    }
                    let mut branch = actual.assignments.as_slice();
                    let result = self.exec(arm, &mut env.clone(), &mut branch)?;
                    self.equal(result, actual.value)?;
                    self.end(&mut branch)?;
                }
                self.reject(default_assignments, *default_value)?;
                Ok(assignment.destination)
            }
            Exec::Advance {
                state,
                handler,
                computation,
                sequence,
                extent,
                context,
            } => {
                let args = [state, handler, computation, sequence, extent, context]
                    .iter()
                    .map(|value| self.operand(value, env))
                    .collect::<Checked<Vec<_>>>()?;
                self.call(crate::cc::ControlHelperRole::Executor, &args, body)
            }
            Exec::Tail { role, args } => self.call(*role, &self.operands(args, env)?, body),
            Exec::Return { value } | Exec::Publish { result: value } => self.operand(value, env),
            Exec::Reject { .. } => {
                let assignment = self.next(body)?;
                if !matches!(assignment.kind, A::Unreachable) {
                    return Err(MISMATCH);
                }
                Ok(assignment.destination)
            }
        }
    }
}
