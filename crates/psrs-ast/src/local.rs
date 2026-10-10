use crate::{CaseBranch, Expr, ExprKind, LowerError, check_argument_names};
use psrs_cst as cst;
use psrs_span::TextRange;

/// Lowers local declarations in source order. Named declarations stay in
/// recursive groups; a pattern declaration introduces its binders only to
/// declarations and expressions that follow it.
pub(crate) fn lower_local_declarations(
    declarations: Vec<cst::Declaration>,
    body: Expr,
    span: TextRange,
) -> Result<Expr, LowerError> {
    if declarations.is_empty() {
        return Ok(body);
    }

    if let cst::Declaration::Pattern(pattern) = &declarations[0] {
        if let Some(error) = check_argument_names(std::slice::from_ref(&pattern.pattern)) {
            return Err(error);
        }
        let pattern_span = pattern.span;
        let mut scrutinee = crate::lower_expr(pattern.value.clone())?;
        if let Some(block) = &pattern.where_block {
            let span = TextRange::new(block.span.start, scrutinee.span.end);
            scrutinee = lower_local_declarations(block.declarations.clone(), scrutinee, span)?;
        }
        let scrutinee = Box::new(scrutinee);
        let pattern = crate::expr::lower_pattern(pattern.pattern.clone())?;
        let remaining = lower_local_declarations(declarations[1..].to_vec(), body, span)?;
        let branch_span = TextRange::new(pattern_span.start, remaining.span.end);
        return Ok(Expr {
            kind: ExprKind::Case {
                scrutinee,
                branches: vec![CaseBranch {
                    pattern,
                    value: remaining,
                    span: branch_span,
                }],
            },
            span: branch_span,
        });
    }

    let group_end = declarations
        .iter()
        .position(|declaration| matches!(declaration, cst::Declaration::Pattern(_)))
        .unwrap_or(declarations.len());
    let group = crate::lower_declarations(declarations[..group_end].to_vec())?;
    let remaining = lower_local_declarations(declarations[group_end..].to_vec(), body, span)?;
    let group_start = declarations[0].span().start;
    Ok(Expr {
        kind: ExprKind::Let {
            declarations: group,
            body: Box::new(remaining),
        },
        span: TextRange::new(group_start, span.end),
    })
}
