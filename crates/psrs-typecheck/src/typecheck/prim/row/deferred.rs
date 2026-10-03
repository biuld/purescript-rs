//! `Prim.Row.Lacks` and `Prim.Row.Union`: the relations whose row rules make
//! progress by re-entering the shared wanted solver on a residual obligation.
//!
//! Their evidence records the parent relation and their deferred constraints
//! carry the part an open tail still needs to prove. Both use the shared row
//! normalizer and kind solver; neither reconstructs a row or its element kind
//! from source syntax.

use super::{EvidenceClass, PrimitiveArgs, PrimitiveEvidence, PrimitiveOutcome, PrimitiveRule};
use crate::typecheck::*;

/// The two row rules that may re-enter wanted solving with a residual relation.
pub(in crate::typecheck) const RULES: [PrimitiveRule; 2] = [LACKS, UNION];

const LACKS: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_ROW_LACKS,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 2,
    solve: solve_lacks,
};

const UNION: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_ROW_UNION,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 3,
    solve: solve_union,
};

/// `Lacks label row` is decided by the known part of the row.
///
/// An empty closed row lacks every label, including an unresolved label. A
/// known present label makes the relation impossible even when the tail is
/// open. A known absent label on a closed row is complete; on an open row it is
/// proved for the known fields and deferred to the tail only when those fields
/// made progress. An open row with no fields gives the rule nothing to decide.
fn solve_lacks(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [label, row] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let span = args.span();
    let Ok(flat) = checker.normalize_row(row.clone(), span) else {
        return PrimitiveOutcome::Undecided;
    };

    if flat.fields.is_empty() && flat.tail == RowTail::Closed {
        return solved(&arguments, Vec::new(), Vec::new());
    }

    let Some(label) = super::known_label(label) else {
        return PrimitiveOutcome::Undecided;
    };
    if flat.fields.iter().any(|(known, _)| known == label) {
        return PrimitiveOutcome::Failed {
            code: TypeCheckErrorKind::NoInstance,
            detail: format!(
                "{} does not hold: label {label:?} is present in the row",
                checker.display_constraint(hir::TypeId::PRIM_ROW_LACKS, &arguments)
            ),
        };
    }

    match flat.tail {
        RowTail::Closed => solved(&arguments, Vec::new(), Vec::new()),
        RowTail::Open(tail) if !flat.fields.is_empty() => {
            let residual = checker.build_constraint(
                hir::TypeId::PRIM_ROW_LACKS,
                vec![
                    InferType::TypeLevelString(label.to_owned()),
                    InferType::Variable(tail),
                ],
                span,
            );
            solved(&arguments, Vec::new(), vec![residual])
        }
        RowTail::Open(_) => PrimitiveOutcome::Undecided,
    }
}

/// `Union left right output` follows the three decisions in official
/// `unionRows`: merge a closed left row, split a closed right/output pair, or
/// move the known left fields into the output and defer the remainder.
fn solve_union(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [left, right, output] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let span = args.span();
    let Ok(left_row) = checker.normalize_row(left.clone(), span) else {
        return PrimitiveOutcome::Undecided;
    };

    if left_row.tail == RowTail::Closed {
        let merged = row_from_ordered_fields(left_row.fields, right.clone());
        return solved(&arguments, vec![(2, merged)], Vec::new());
    }

    // If right and output are both closed, official partitions output by the
    // labels present in right. A leftover right label means this rule cannot
    // solve the relation and it declines without a partial answer.
    if let (Ok(right_row), Ok(output_row)) = (
        checker.normalize_row(right.clone(), span),
        checker.normalize_row(output.clone(), span),
    ) && right_row.tail == RowTail::Closed
        && output_row.tail == RowTail::Closed
    {
        let partition = partition_output(output_row.fields, &right_row.fields);
        if !partition.leftover.is_empty() {
            return PrimitiveOutcome::Undecided;
        }
        let inferred_left = row_from_ordered_fields(partition.left_fields, InferType::RowEmpty);
        let inferred_right = row_from_ordered_fields(partition.right_fields, InferType::RowEmpty);
        return solved(
            &arguments,
            vec![(0, inferred_left), (1, inferred_right)],
            Vec::new(),
        );
    }

    if left_row.fields.is_empty() {
        return PrimitiveOutcome::Undecided;
    }
    let RowTail::Open(remainder) = left_row.tail else {
        unreachable!("closed rows were handled above")
    };
    let tail = InferType::Variable(remainder);
    let Some(fresh_tail) = fresh_row_tail(checker, left, span) else {
        return PrimitiveOutcome::Undecided;
    };
    let output = row_from_ordered_fields(left_row.fields, fresh_tail.clone());
    let residual = checker.build_constraint(
        hir::TypeId::PRIM_ROW_UNION,
        vec![tail, right.clone(), fresh_tail],
        span,
    );
    solved(&arguments, vec![(2, output)], vec![residual])
}

/// Makes a fresh inference variable with the same `Row k` kind as `source`.
/// The shared kind solver records the relationship, so generalized residuals
/// retain the row element kind even when `k` is not `Type`.
fn fresh_row_tail(checker: &mut Checker, source: &InferType, span: TextRange) -> Option<InferType> {
    let expected = checker.kind_of_type(source, span)?;
    let fresh = checker.fresh();
    let actual = checker.kind_of_type(&fresh, span)?;
    checker.unify_kind(actual, expected, span).then_some(fresh)
}

/// Partitions `output` with the rightmost matching labels assigned to `right`.
/// This mirrors official's `foldr`: matching consumes one occurrence of a
/// duplicate label from the right row, and every other output field belongs to
/// the inferred left row.
struct OutputPartition {
    left_fields: Vec<(String, InferType)>,
    right_fields: Vec<(String, InferType)>,
    leftover: Vec<String>,
}

fn partition_output(
    output: Vec<(String, InferType)>,
    right: &[(String, InferType)],
) -> OutputPartition {
    let mut remaining = right
        .iter()
        .map(|(label, _)| label.clone())
        .collect::<Vec<_>>();
    let mut left_fields = Vec::new();
    let mut right_fields = Vec::new();

    for field in output.into_iter().rev() {
        if let Some(index) = remaining.iter().position(|label| label == &field.0) {
            remaining.remove(index);
            right_fields.push(field);
        } else {
            left_fields.push(field);
        }
    }
    left_fields.reverse();
    right_fields.reverse();
    OutputPartition {
        left_fields,
        right_fields,
        leftover: remaining,
    }
}

/// Builds a row without sorting it. `rowFromList` preserves the row order in
/// these two relations; canonical ordering belongs to `Nub` and `RowToList`.
fn row_from_ordered_fields(fields: Vec<(String, InferType)>, tail: InferType) -> InferType {
    fields
        .into_iter()
        .rev()
        .fold(tail, |tail, (label, ty)| InferType::RowExtend {
            label,
            ty: Box::new(ty),
            tail: Box::new(tail),
        })
}

/// States the row arguments the relation decided, then lets dispatch check
/// those arguments through the shared unifier.
fn solved(
    arguments: &[InferType],
    decisions: Vec<(usize, InferType)>,
    deferred: Vec<WantedConstraint>,
) -> PrimitiveOutcome {
    let mut decided = arguments.to_vec();
    for (position, value) in decisions {
        let Some(argument) = decided.get_mut(position) else {
            return PrimitiveOutcome::Undecided;
        };
        *argument = value;
    }
    let evidence = PrimitiveEvidence::Dictionary { arguments: decided };
    if deferred.is_empty() {
        PrimitiveOutcome::Solved { evidence, deferred }
    } else {
        PrimitiveOutcome::Deferred {
            evidence: Some(evidence),
            deferred,
        }
    }
}
