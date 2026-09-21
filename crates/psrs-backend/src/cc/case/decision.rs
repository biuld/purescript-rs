//! Target-neutral pattern decision compilation.
//!
//! This step preserves source order and records only which branch is selected.
//! Representation tests, projections, bindings, and branch expressions remain
//! the responsibility of the CC lowering that consumes the decision.

use psrs_core::{CaseBranch, PatternKind};
use psrs_hir::SymbolId;
use psrs_span::TextRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Matcher {
    Constructor(SymbolId),
    Record,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Alternative {
    pub(super) branch: usize,
    pub(super) matcher: Matcher,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PatternDecision {
    /// All branch indices in source order, including default alternatives.
    pub(super) ordered: Vec<usize>,
    pub(super) alternatives: Vec<Alternative>,
    pub(super) fallback: Option<usize>,
}

pub(super) fn compile(
    branches: &[CaseBranch],
    _span: TextRange,
) -> Result<PatternDecision, &'static str> {
    let mut ordered = Vec::with_capacity(branches.len());
    let mut alternatives = Vec::new();
    let mut fallback = None;
    for (branch, case) in branches.iter().enumerate() {
        ordered.push(branch);
        match case.pattern.kind {
            PatternKind::Constructor { symbol, .. } => alternatives.push(Alternative {
                branch,
                matcher: Matcher::Constructor(symbol),
            }),
            PatternKind::Record { .. } => alternatives.push(Alternative {
                branch,
                matcher: Matcher::Record,
            }),
            PatternKind::Wildcard | PatternKind::Var { .. } => {
                fallback.get_or_insert(branch);
            }
        }
    }
    if ordered.is_empty() {
        return Err("case has no alternatives");
    }
    Ok(PatternDecision {
        ordered,
        alternatives,
        fallback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_core::{Expr, ExprKind, Pattern, TypeId};
    use psrs_hir::LocalId;

    fn branch(pattern: PatternKind) -> CaseBranch {
        let span = TextRange::new(0, 1);
        CaseBranch {
            pattern: Pattern {
                kind: pattern,
                ty: TypeId(0),
                span,
            },
            value: Expr {
                kind: ExprKind::Integer(0),
                ty: TypeId(0),
                span,
            },
            span,
        }
    }

    #[test]
    fn preserves_order_for_nested_constructor_and_record_patterns() {
        let constructor = SymbolId::new(psrs_hir::ModuleId(0), 1);
        let nested = PatternKind::Constructor {
            symbol: constructor,
            arguments: vec![Pattern {
                kind: PatternKind::Var {
                    id: LocalId(0),
                    ty: TypeId(0),
                },
                ty: TypeId(0),
                span: TextRange::new(0, 1),
            }],
        };
        let decision = compile(
            &[
                branch(nested),
                branch(PatternKind::Record { fields: Vec::new() }),
                branch(PatternKind::Wildcard),
            ],
            TextRange::new(0, 1),
        )
        .expect("decision compilation");
        assert_eq!(decision.ordered, vec![0, 1, 2]);
        assert_eq!(decision.alternatives[0].branch, 0);
        assert_eq!(decision.alternatives[1].matcher, Matcher::Record);
        assert_eq!(decision.fallback, Some(2));
    }
}
