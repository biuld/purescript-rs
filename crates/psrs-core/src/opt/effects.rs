use crate::{Expr, ExprKind};

/// Conservative evaluation effects relevant to call-by-value rewrites.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Effects {
    pub may_call: bool,
    pub may_trap: bool,
    pub may_write: bool,
}

impl Effects {
    fn combine(self, other: Self) -> Self {
        Self {
            may_call: self.may_call || other.may_call,
            may_trap: self.may_trap || other.may_trap,
            may_write: self.may_write || other.may_write,
        }
    }

    pub fn inert(self) -> bool {
        !self.may_call && !self.may_trap && !self.may_write
    }
}

pub(super) fn summarize(expression: &Expr) -> Effects {
    psrs_span::with_sufficient_stack(|| summarize_inner(expression))
}

fn summarize_inner(expression: &Expr) -> Effects {
    match &expression.kind {
        ExprKind::Local(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => Effects::default(),
        // The unit value has no payload and no failure mode.
        ExprKind::Unit => Effects::default(),
        // The state token has no payload and no failure mode.
        ExprKind::StateToken => Effects::default(),
        // A trap is a failure by definition, so no rewrite may drop or move it
        // as if it were an inert value.
        ExprKind::Trap => Effects {
            may_trap: true,
            ..Effects::default()
        },
        // A non-function global may lower to a direct zero-argument call,
        // including an imported value. Without consulting declaration types
        // here, keep every global read conservatively observable.
        ExprKind::Global(_) => Effects {
            may_call: true,
            may_trap: true,
            may_write: true,
        },
        // Creating a closure does not run its body. Function identity is not
        // observable in Core, and capture reads are inert local lookups.
        ExprKind::Lambda { .. } => Effects::default(),
        ExprKind::Constructor { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => combine_all(arguments.iter().map(summarize)),
        // The operation itself can trap even when its arguments are inert, so
        // the summary is not only the arguments' effects.
        ExprKind::IntrinsicCall {
            intrinsic,
            arguments,
        } => {
            let arguments = combine_all(arguments.iter().map(summarize));
            let effects = intrinsic.descriptor().effects;
            Effects {
                may_trap: effects.may_trap || arguments.may_trap,
                may_write: effects.may_write || arguments.may_write,
                ..arguments
            }
        }
        ExprKind::Record { fields } => {
            combine_all(fields.iter().map(|(_, value)| summarize(value)))
        }
        ExprKind::RecordUpdate { record, fields } => summarize(record).combine(combine_all(
            fields.iter().map(|(_, value)| summarize(value)),
        )),
        ExprKind::FieldAccess { record, .. }
        | ExprKind::RepresentationCast { value: record, .. } => summarize(record),
        ExprKind::Application(_, _) => Effects {
            may_call: true,
            may_trap: true,
            may_write: true,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TypeId;
    use psrs_hir::Intrinsic;

    fn intrinsic(intrinsic: Intrinsic, arguments: Vec<Expr>) -> Expr {
        Expr {
            kind: ExprKind::IntrinsicCall {
                intrinsic,
                arguments,
            },
            ty: TypeId(0),
            span: psrs_span::TextRange::default(),
        }
    }

    #[test]
    fn a_byte_conversion_chain_is_not_inert() {
        let array = Expr {
            kind: ExprKind::Array {
                elements: Vec::new(),
            },
            ty: TypeId(0),
            span: psrs_span::TextRange::default(),
        };
        let bytes = intrinsic(Intrinsic::BytesToString, vec![array]);
        let string = intrinsic(Intrinsic::StringToBytes, vec![bytes]);
        let length = intrinsic(Intrinsic::ArrayLength, vec![string]);
        assert!(!summarize(&length).inert());
    }
}
