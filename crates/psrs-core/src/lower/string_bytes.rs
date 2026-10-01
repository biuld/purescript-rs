//! Typed Core lowering of the `String`/`Array Int` byte conversions.
//!
//! Both are unary intrinsics, so each is recognized by its saturated
//! application and becomes a dedicated expression: `stringToBytes` reads a
//! source `String` and produces the UTF-8 bytes as an `Array Int`, and
//! `bytesToString` reads those bytes back as a `String`
//! ([DEC-16](../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).

use super::{Expr, ExprKind, ExternalKind, TypeId, flatten_intrinsic};
use psrs_span::TextRange;
use std::collections::HashMap;

/// Lowers either conversion, or returns `None` when `function` is not one.
pub(super) fn lower_conversion(
    function: &Expr,
    final_argument: &Expr,
    externals: &HashMap<psrs_hir::SymbolId, ExternalKind>,
    ty: TypeId,
    span: TextRange,
) -> Option<Expr> {
    let (symbol, args) = flatten_intrinsic(function, final_argument.clone(), externals)?;
    if args.len() != 1 {
        return None;
    }
    if !matches!(
        externals.get(&symbol),
        Some(ExternalKind::Intrinsic(
            psrs_hir::Intrinsic::StringToBytes | psrs_hir::Intrinsic::BytesToString
        ))
    ) {
        return None;
    }
    let value = Box::new(args[0].clone());
    let kind = match externals.get(&symbol) {
        Some(ExternalKind::Intrinsic(psrs_hir::Intrinsic::StringToBytes)) => {
            ExprKind::StringToBytes(value)
        }
        _ => ExprKind::BytesToString(value),
    };
    Some(Expr { kind, ty, span })
}
