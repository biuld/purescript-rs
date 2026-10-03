//! Lowering of the Core expressions that carry their value outright.
//!
//! A literal needs no operand and no layout decision, so each one becomes the
//! assignment its representation fixes. The trap is the opposite case: it has a
//! type but never produces a value, so it becomes the assignment that ends the
//! path instead of one that fills a destination.

use super::super::{Assignment, AssignmentKind, ValueId, ValueShape};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    /// Lowers one literal to a constant assignment of the shape its type fixes.
    ///
    /// The value arrives as the assignment kind so that deciding *which*
    /// constant a Core expression is stays in the dispatch that matched it, and
    /// this helper stays a single place where a value becomes a destination.
    pub(super) fn lower_literal(
        &mut self,
        kind: AssignmentKind,
        span: TextRange,
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> ValueId {
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            kind,
            span,
        });
        destination
    }

    /// Lowers the trap expression.
    ///
    /// The destination exists only so the assignment has the shape its context
    /// expects; the MIR instruction behind it is `Unreachable`, which supplies
    /// the result type on a path that never runs.
    pub(super) fn lower_trap(
        &mut self,
        span: TextRange,
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        Ok(self.lower_literal(AssignmentKind::Unreachable, span, ty, assignments))
    }
}
