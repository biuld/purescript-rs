//! Row alignment for `Coercible`, following `canonRow` in
//! `Entailment/Coercible.hs`.
//!
//! Matching labels are proved field by field. No leftover labels means the
//! tails are proved next, so `{ x :: Int | r }` against `{ x :: Int | s }`
//! becomes `Coercible r s`. Leftover labels are `TypesDoNotUnify` when both
//! sides are determined, and insoluble when one side is a bare flexible tail
//! with no labels of its own. That is official's `TUnknown` case, which the
//! suite reports as `NoInstanceFound`.

use crate::typecheck::*;
use std::collections::HashSet;

impl Checker {
    pub(super) fn prove_concrete_rows(
        &mut self,
        source: &InferType,
        target: &InferType,
        span: TextRange,
        depth: usize,
        path: &mut HashSet<(String, String)>,
        verdict: &mut super::CoercionVerdict<'_>,
    ) -> bool {
        let Ok(left) = self.normalize_row(source.clone(), span) else {
            return false;
        };
        let Ok(right) = self.normalize_row(target.clone(), span) else {
            return false;
        };
        let mut left_fields = left.fields;
        let mut right_fields = right.fields;
        left_fields.sort_by(|left, right| left.0.cmp(&right.0));
        right_fields.sort_by(|left, right| left.0.cmp(&right.0));
        let mut left_rest = Vec::new();
        let mut right_rest = Vec::new();
        let mut left_index = 0;
        let mut right_index = 0;
        while left_index < left_fields.len() && right_index < right_fields.len() {
            match left_fields[left_index].0.cmp(&right_fields[right_index].0) {
                std::cmp::Ordering::Equal => {
                    let left_ty = left_fields[left_index].1.clone();
                    let right_ty = right_fields[right_index].1.clone();
                    left_index += 1;
                    right_index += 1;
                    let mut nested = path.clone();
                    if !self.proves_coercible_inner(
                        &left_ty,
                        &right_ty,
                        span,
                        depth + 1,
                        &mut nested,
                        verdict,
                    ) {
                        return false;
                    }
                }
                std::cmp::Ordering::Less => {
                    left_rest.push(left_fields[left_index].clone());
                    left_index += 1;
                }
                std::cmp::Ordering::Greater => {
                    right_rest.push(right_fields[right_index].clone());
                    right_index += 1;
                }
            }
        }
        left_rest.extend(left_fields[left_index..].iter().cloned());
        right_rest.extend(right_fields[right_index..].iter().cloned());
        if left_rest.is_empty() && right_rest.is_empty() {
            return self.prove_row_tails(left.tail, right.tail);
        }
        if bare_flexible_tail(self, left_rest.is_empty(), left.tail)
            || bare_flexible_tail(self, right_rest.is_empty(), right.tail)
        {
            return false;
        }
        self.note_determined_disagreement(source, target, verdict.types);
        false
    }

    /// Identical tails are done. A given can discharge the two tails. Anything
    /// else is irreducible: `Coercible r s` for distinct skolems is
    /// `NoInstanceFound`, not `TypesDoNotUnify`. Recursing into row alignment
    /// would loop, because an open tail normalizes back into this function.
    fn prove_row_tails(&mut self, left: RowTail, right: RowTail) -> bool {
        match (left, right) {
            (RowTail::Closed, RowTail::Closed) => true,
            (RowTail::Open(left_variable), RowTail::Open(right_variable))
                if left_variable == right_variable =>
            {
                true
            }
            _ => self.given_coercible(&left.to_type(), &right.to_type()),
        }
    }
}

fn bare_flexible_tail(checker: &Checker, no_leftovers: bool, tail: RowTail) -> bool {
    no_leftovers
        && matches!(tail, RowTail::Open(variable) if !checker.state.rigid.contains(&variable))
}
