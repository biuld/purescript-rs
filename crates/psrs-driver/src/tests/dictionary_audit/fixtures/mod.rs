//! Typed Core fixtures used by the dictionary audit. Each builder returns a
//! module plus its selected entry symbol; the module is valid Typed Core and
//! carries explicit class evidence or explicit dictionary products.

use psrs_hir::{LocalId, SymbolId};
use psrs_span::TextRange;
use psrs_thir as thir;

mod evidence;
mod generic;
mod methods;
mod ordering;

pub(super) use evidence::{dictionary_module, escaping_method_module};
pub(super) use generic::{erased_dictionary_module, polymorphic_method_module};
pub(super) use methods::{default_method_module, recursive_instance_module};
pub(super) use ordering::{ordered_dictionaries_module, shared_dictionary_module};

pub(super) fn typed(kind: thir::ExprKind, ty: thir::TypeId, span: TextRange) -> thir::Expr {
    thir::Expr { kind, ty, span }
}

pub(super) fn binder(id: u32, name: &str, ty: thir::TypeId, span: TextRange) -> thir::Binder {
    thir::Binder {
        id: LocalId(id),
        name: name.into(),
        ty,
        span,
    }
}

pub(super) fn declaration(
    symbol: SymbolId,
    name: &str,
    ty: thir::TypeId,
    value: thir::Expr,
    span: TextRange,
) -> thir::Declaration {
    thir::Declaration {
        symbol,
        name: name.into(),
        name_span: span,
        quantified: Vec::new(),
        ty,
        value,
        span,
    }
}
