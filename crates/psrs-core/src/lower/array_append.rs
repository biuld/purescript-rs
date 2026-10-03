//! Typed Core lowering of `Array.append`.
//!
//! `arrayAppend` is a binary compiler intrinsic with no source spelling for
//! the array it creates, so a saturated application becomes its own expression.
//! Both operands are the same `Array a`, and the result is a fresh array; the
//! backend lowers the copy without mutating either operand.

use super::{Expr, ExprKind, ExternalKind, TypeId, flatten_intrinsic};
use psrs_span::TextRange;
use std::collections::HashMap;

/// Lowers `arrayAppend`, or returns `None` when `function` is not that
/// intrinsic or is not yet saturated.
pub(super) fn lower_append(
    function: &Expr,
    final_argument: &Expr,
    externals: &HashMap<psrs_hir::SymbolId, ExternalKind>,
    ty: TypeId,
    span: TextRange,
) -> Option<Expr> {
    let (symbol, args) = flatten_intrinsic(function, final_argument.clone(), externals)?;
    if args.len() != 2 {
        return None;
    }
    if !matches!(
        externals.get(&symbol),
        Some(ExternalKind::Intrinsic(psrs_hir::Intrinsic::ArrayAppend))
    ) {
        return None;
    }
    Some(Expr {
        kind: ExprKind::ArrayAppend {
            left: Box::new(args[0].clone()),
            right: Box::new(args[1].clone()),
        },
        ty,
        span,
    })
}
