//! Checked pattern conversion from THIR to Core.
use super::{LowerError, TypeId};

pub(super) fn lower_pattern(pattern: psrs_thir::Pattern) -> Result<crate::Pattern, LowerError> {
    let span = pattern.span;
    let kind = match pattern.kind {
        psrs_thir::PatternKind::Wildcard => crate::PatternKind::Wildcard,
        psrs_thir::PatternKind::Literal { literal } => crate::PatternKind::Literal {
            value: match literal {
                psrs_thir::PatternLiteral::Integer(value) => crate::Literal::Integer(value),
                psrs_thir::PatternLiteral::Number(value) => crate::Literal::Number(value),
                psrs_thir::PatternLiteral::String(value) => crate::Literal::String(value),
                psrs_thir::PatternLiteral::Char(value) => crate::Literal::Char(value),
                psrs_thir::PatternLiteral::Boolean(value) => crate::Literal::Boolean(value),
            },
        },
        psrs_thir::PatternKind::Array { elements } => crate::PatternKind::Array {
            elements: elements
                .into_iter()
                .map(lower_pattern)
                .collect::<Result<Vec<_>, _>>()?,
        },
        psrs_thir::PatternKind::Named { id, pattern } => crate::PatternKind::Named {
            id,
            pattern: Box::new(lower_pattern(*pattern)?),
        },
        psrs_thir::PatternKind::Var { id, ty } => crate::PatternKind::Var {
            id,
            ty: TypeId(ty.0),
        },
        psrs_thir::PatternKind::Constructor { symbol, arguments } => {
            crate::PatternKind::Constructor {
                symbol,
                arguments: arguments
                    .into_iter()
                    .map(lower_pattern)
                    .collect::<Result<Vec<_>, _>>()?,
            }
        }
        psrs_thir::PatternKind::Record { fields } => crate::PatternKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, pattern)| Ok((label, lower_pattern(pattern)?)))
                .collect::<Result<Vec<_>, LowerError>>()?,
        },
    };
    Ok(crate::Pattern {
        kind,
        ty: TypeId(pattern.ty.0),
        span,
    })
}
