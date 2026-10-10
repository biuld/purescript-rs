use crate::{LowerError, Operator, lower_expr, lower_name};
use psrs_cst::{self as cst, ExprKind as CstExprKind};
use psrs_span::TextRange;

use super::{Expr, ExprKind};

pub(crate) fn lower_operator_chain(
    operator: cst::CstName,
    left: cst::Expr,
    right: cst::Expr,
    span: TextRange,
) -> Result<Expr, LowerError> {
    let mut operands = Vec::new();
    let mut operators = Vec::new();
    collect_operator_chain(left, &mut operands, &mut operators)?;
    operators.push(Operator {
        name: lower_name(operator.clone()),
        span: operator.span,
    });
    collect_operator_chain(right, &mut operands, &mut operators)?;
    Ok(Expr {
        kind: ExprKind::OperatorChain {
            operands,
            operators,
        },
        span,
    })
}

fn collect_operator_chain(
    expression: cst::Expr,
    operands: &mut Vec<Expr>,
    operators: &mut Vec<Operator>,
) -> Result<(), LowerError> {
    psrs_span::with_sufficient_stack(|| {
        collect_operator_chain_inner(expression, operands, operators)
    })
}

fn collect_operator_chain_inner(
    expression: cst::Expr,
    operands: &mut Vec<Expr>,
    operators: &mut Vec<Operator>,
) -> Result<(), LowerError> {
    let span = expression.span;
    match expression.kind {
        CstExprKind::Operator {
            operator,
            left,
            right,
        } => {
            collect_operator_chain(*left, operands, operators)?;
            operators.push(Operator {
                name: lower_name(operator.clone()),
                span: operator.span,
            });
            collect_operator_chain(*right, operands, operators)
        }
        kind => {
            operands.push(lower_expr(cst::Expr { kind, span })?);
            Ok(())
        }
    }
}
