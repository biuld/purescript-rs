//! `Prim.Row.Cons`, `Prim.Row.Nub`, and `Prim.RowList.RowToList`: the three
//! relations whose arguments are rows, and the three that build or canonicalise
//! one from the other.
//!
//! All three are relations whose dictionaries are empty and erase, so what a
//! rule produces is a decision about rows rather than a value. They read their
//! rows through the one normalizer in `typecheck/rows.rs` and bind what they
//! decide through [`Checker::unify`], so the arguments a rule's evidence
//! records are the arguments the constraint has *after* the decision. Nothing
//! here parses source syntax, walks a row by hand, or keeps a kind table.
//!
//! Official `TypeChecker/Entailment.hs` is the reference and each rule follows
//! its guard exactly, because the guard is what the rule decides:
//!
//! - `solveRowCons` matches on a known label and builds the extension, so an
//!   unknown label is not an obligation this relation can answer. Official does
//!   not read the tail: `Cons "a" value tail row` decides `row` for any tail,
//!   and an open tail is not a reason to decline.
//! - `nubRows` guards on `isREmpty rest`, so a closed row is canonicalised and
//!   an open one is not. There is no partial answer to give for an open row: the
//!   nubbed form of `( a :: Int | r )` depends on the labels inside `r`, so
//!   deciding anything about it would be a guess.
//! - `rowToRowList` has the same guard, and its result is the sorted spine of
//!   `RowList.Cons` over `RowList.Nil`.
//!
//! Official then unifies each rule's decided arguments against the goal's
//! arguments in order, and that step is where a wrong answer is rejected. This
//! rule reproduces it by binding the decided value through the shared unifier
//! and, when the unifier reports a disagreement, answering `Failed` under
//! `TypesDoNotUnify` — the code `purs` raises for its own decided argument. The
//! framework still refuses that answer while an argument is unsolved, because an
//! obligation with an unknown argument is undecided rather than impossible; the
//! one case that differs from official is a decided extension that contradicts
//! the wanted row while the tail is still unknown, and there the obligation
//! reaches instance search instead of being reported.
//!
//! Two shapes the shared model does not spell out are recorded here rather than
//! worked around. A row label is a `TypeLevelString`, so the relation's declared
//! kind is what makes reading the label as a `Symbol` the right reading; and the
//! element kind of a `RowList` cell is carried by the member's declared kind
//! (`forall k. Row k -> RowList k -> Constraint`) rather than by a spine node,
//! because the shared type spine has no kind application. A converted list is
//! therefore the same three-value-argument spine that source elaboration builds
//! for `RowList.Cons`, with the labels as type-level strings.

use super::{EvidenceClass, PrimitiveArgs, PrimitiveEvidence, PrimitiveOutcome, PrimitiveRule};
use crate::typecheck::*;

/// The `Prim.Row` and `Prim.RowList` relations this module decides, as the rule
/// table records them. `Lacks` and `Union` are not here: over an open tail both
/// have to defer the remainder to that tail, which is the one answer none of
/// these three returns.
pub(in crate::typecheck) const RULES: [PrimitiveRule; 3] = [CONS, NUB, ROW_TO_LIST];

/// `Cons`'s entry in the rule table. Its arity is the member's four declared
/// arguments, `label`, `value`, `tail`, and `row`.
const CONS: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_ROW_CONS,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 4,
    solve: solve_cons,
};

/// `Nub`'s entry in the rule table.
const NUB: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_ROW_NUB,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 2,
    solve: solve_nub,
};

/// `RowToList`'s entry in the rule table.
const ROW_TO_LIST: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_ROW_TO_LIST,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 2,
    solve: solve_row_to_list,
};

/// The position of `Cons`'s row, the argument the relation determines.
const CONS_ROW: usize = 3;
/// The position of `Nub`'s nubbed row.
const NUB_NUBBED: usize = 1;
/// The position of `RowToList`'s list. Its other argument is the row the
/// conversion reads, and no rule decides it.
const ROW_TO_LIST_LIST: usize = 1;

/// Whether `Cons label value tail row` holds for one row extension.
///
/// Official `solveRowCons` has one arm and it is guarded on the label alone, so
/// this rule has one reading and one decline. A known label decides the row as
/// the extension of `tail` by that label and `value`, whatever `tail` is — an
/// open tail is decided, not declined, because the extension is still exactly
/// the row the relation names. An unknown label is a decline: there is no
/// reading of the arguments that names the extension, and answering with a
/// fresh label would be inventing a field.
fn solve_cons(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [label, value, tail, _row] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let Some(label) = known_label(label) else {
        return PrimitiveOutcome::Undecided;
    };
    let extension = InferType::RowExtend {
        label: label.to_owned(),
        ty: Box::new(value.clone()),
        tail: Box::new(tail.clone()),
    };
    decide(
        checker,
        args,
        hir::TypeId::PRIM_ROW_CONS,
        &arguments,
        CONS_ROW,
        extension,
        "row",
    )
}

/// Whether `Nub original nubbed` holds for two rows.
///
/// Official `nubRows` canonicalises a closed row and declines an open one: the
/// nubbed form of `( a :: Int | r )` depends on the labels inside `r`, so there
/// is no partial answer that is not a guess. Canonicalising means the labels in
/// ascending order with each label once, which is what `row_from_fields` builds
/// from the fields the normalizer collected.
///
/// A label that occurs twice in the row is this relation's own duplicate case,
/// and official resolves it by keeping the first occurrence: `rowToSortedList`
/// is a stable sort and `nubBy` drops every element equal to one it has already
/// kept. So the outer extension of a label wins over an inner one of the same
/// name, which is the only reading of `Nub` that is not "the row is a map keyed
/// by label" — a map would silently choose the inner one, and the order the row
/// was written in is not something a solver may throw away.
fn solve_nub(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [original, _nubbed] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let span = args.span();
    // A value that is not a row is a kind error the kind pass reports about the
    // argument itself. Reporting it again here would either duplicate that
    // diagnostic or, worse, decide a closed row out of a shape this compiler
    // does not understand.
    let Ok(FlatRow {
        fields,
        tail: RowTail::Closed,
    }) = checker.normalize_row(original.clone(), span)
    else {
        return PrimitiveOutcome::Undecided;
    };
    let nubbed = row_from_fields(nub(&fields), InferType::RowEmpty);
    decide(
        checker,
        args,
        hir::TypeId::PRIM_ROW_NUB,
        &arguments,
        NUB_NUBBED,
        nubbed,
        "nubbed",
    )
}

/// Whether `RowToList row list` holds for a row and the list that names its
/// labels.
///
/// Official `rowToRowList` guards on the row being closed and then builds
/// `RowList.Cons` over `RowList.Nil` from the row's labels in ascending order, so
/// the list is canonical and an open row declines for the same reason `Nub`
/// declines one. The two members read the same row and reach the same ordering,
/// which is what lets a caller convert a row and nub it interchangeably.
fn solve_row_to_list(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [row, _list] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let span = args.span();
    let Ok(FlatRow {
        mut fields,
        tail: RowTail::Closed,
    }) = checker.normalize_row(row.clone(), span)
    else {
        return PrimitiveOutcome::Undecided;
    };
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    let list = fields
        .iter()
        .rev()
        .fold(row_list_nil(), |tail, (label, field)| {
            row_list_cons(label, field.clone(), tail)
        });
    decide(
        checker,
        args,
        hir::TypeId::PRIM_ROW_TO_LIST,
        &arguments,
        ROW_TO_LIST_LIST,
        list,
        "list",
    )
}

/// Drops every entry whose label an earlier entry already carries, after a
/// stable sort by label.
///
/// The sort is what official `rowToSortedList` does, and its stability is what
/// decides the duplicate case: two entries with the same label stay in the order
/// the row was written in, so dropping the later one keeps the outer extension.
fn nub(fields: &[(String, InferType)]) -> Vec<(String, InferType)> {
    let mut sorted = fields.to_vec();
    sorted.sort_by(|left, right| left.0.cmp(&right.0));
    sorted.dedup_by(|later, earlier| earlier.0 == later.0);
    sorted
}

/// The empty `RowList`, which every converted list ends at.
///
/// It is the registry's `RowList.Nil` constructor and nothing else, so the kind
/// it is applied at comes from that member's declared kind rather than from a
/// node of its own.
fn row_list_nil() -> InferType {
    InferType::Constructor(TypeConstructor::User(hir::TypeId::PRIM_ROW_LIST_NIL))
}

/// One `RowList.Cons` cell: the label, the field type, and the rest of the list.
///
/// The registry's `RowList.Cons` constructor applied through ordinary
/// application, in the same three-value-argument spine that source elaboration
/// builds for it, so a decided list and a written one are the same type.
fn row_list_cons(label: &str, field: InferType, tail: InferType) -> InferType {
    apply(
        apply(
            apply(
                InferType::Constructor(TypeConstructor::User(hir::TypeId::PRIM_ROW_LIST_CONS)),
                InferType::TypeLevelString(label.to_owned()),
            ),
            field,
        ),
        tail,
    )
}

fn apply(head: InferType, argument: InferType) -> InferType {
    InferType::Application(Box::new(head), Box::new(argument))
}

/// The type-level string a label argument carries, or `None` when it does not.
///
/// An unsolved inference variable is `None` — unknown, not the empty label — and
/// so is anything that is not a literal, which is what makes the argument read
/// identically here and in the kind check that allowed it.
fn known_label(argument: &InferType) -> Option<&str> {
    match argument {
        InferType::TypeLevelString(value) => Some(value.as_str()),
        _ => None,
    }
}

/// Binds the argument at `position` to what a rule decided, through the shared
/// substitution, and turns the result into the relation's evidence.
///
/// The binding goes through [`Checker::unify`] rather than through a direct
/// assignment, so the decision is one the rest of inference can see, the
/// arguments' kinds are checked by the shared binder, and two rows are compared
/// by the shared row normalizer rather than structurally. That is also where
/// official rejects a wrong answer: `TypeChecker.Entailment` unifies each rule's
/// decided arguments against the goal's arguments for every dictionary it
/// produces, and the diagnostic a disagreement produces here comes from that
/// same step.
///
/// A disagreement is reported as `Failed` rather than left to the unifier's own
/// diagnostic, because the obligation itself is what cannot hold and `Failed` is
/// the answer that says so — and because the framework, not the rule, decides
/// whether an answer may be honoured: it refuses a `Failed` while an argument is
/// unsolved. The unifier's diagnostic for the attempt is dropped here, because
/// the rule's own reason is the one that names the relation.
fn decide(
    checker: &mut Checker,
    args: &PrimitiveArgs,
    class_id: hir::TypeId,
    arguments: &[InferType],
    position: usize,
    value: InferType,
    argument: &'static str,
) -> PrimitiveOutcome {
    let span = args.span();
    let Some(wanted) = arguments.get(position).cloned() else {
        return PrimitiveOutcome::Undecided;
    };
    let errors_before = checker.state.errors.len();
    checker.unify(value.clone(), wanted, span);
    if checker.state.errors.len() > errors_before {
        checker.state.errors.truncate(errors_before);
        return PrimitiveOutcome::Failed {
            code: TypeCheckErrorKind::TypeMismatch,
            detail: format!(
                "{} does not hold: the {argument} is {}",
                checker.display_constraint(class_id, arguments),
                checker.display_type(&value)
            ),
        };
    }
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Dictionary {
            arguments: args.resolved(checker),
        },
        deferred: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
