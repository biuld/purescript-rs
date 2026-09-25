use crate::{Expr, ExprKind, Primitive};

/// Conservative evaluation effects relevant to call-by-value rewrites.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Effects {
    pub may_call: bool,
    pub may_trap: bool,
}

impl Effects {
    fn combine(self, other: Self) -> Self {
        Self {
            may_call: self.may_call || other.may_call,
            may_trap: self.may_trap || other.may_trap,
        }
    }

    pub fn inert(self) -> bool {
        !self.may_call && !self.may_trap
    }
}

pub(super) fn summarize(expression: &Expr) -> Effects {
    match &expression.kind {
        ExprKind::Local(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => Effects::default(),
        // A non-function global may lower to a direct zero-argument call,
        // including an imported value. Without consulting declaration types
        // here, keep every global read conservatively observable.
        ExprKind::Global(_) => Effects {
            may_call: true,
            may_trap: true,
        },
        // Creating a closure does not run its body. Function identity is not
        // observable in Core, and capture reads are inert local lookups.
        ExprKind::Lambda { .. } => Effects::default(),
        ExprKind::Constructor { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => combine_all(arguments.iter().map(summarize)),
        ExprKind::Record { fields } => {
            combine_all(fields.iter().map(|(_, value)| summarize(value)))
        }
        ExprKind::RecordUpdate { record, fields } => summarize(record).combine(combine_all(
            fields.iter().map(|(_, value)| summarize(value)),
        )),
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => summarize(record),
        ExprKind::UnaryPrimitive { value, .. } => summarize(value),
        ExprKind::ArrayIndex { array, index } => Effects {
            may_trap: true,
            ..summarize(array).combine(summarize(index))
        },
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => Effects {
            may_trap: true,
            ..summarize(array)
                .combine(summarize(index))
                .combine(summarize(value))
        },
        ExprKind::Primitive { op, left, right } => Effects {
            // The Euclidean operators lower through helpers that still trap
            // on a zero divisor; the truncating operators trap directly.
            may_trap: matches!(
                op,
                Primitive::IntQuot | Primitive::IntRem | Primitive::IntDiv | Primitive::IntMod
            ),
            ..summarize(left).combine(summarize(right))
        },
        ExprKind::Application(_, _) => Effects {
            may_call: true,
            may_trap: true,
        },
        ExprKind::Let { bindings, body } => combine_all(
            bindings
                .iter()
                .map(|binding| summarize(&binding.value))
                .chain(std::iter::once(summarize(body))),
        ),
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => summarize(condition)
            .combine(summarize(then_branch))
            .combine(summarize(else_branch)),
        // The source type is exhaustive, but Core::verify does not currently
        // establish decision-tree exhaustiveness. Treat a failed match as a
        // possible trap so dropping a case cannot suppress that behavior.
        ExprKind::Case {
            scrutinee,
            branches,
        } => Effects {
            may_trap: true,
            ..summarize(scrutinee).combine(combine_all(
                branches.iter().map(|branch| summarize(&branch.value)),
            ))
        },
    }
}

fn combine_all(effects: impl IntoIterator<Item = Effects>) -> Effects {
    effects
        .into_iter()
        .fold(Effects::default(), Effects::combine)
}
