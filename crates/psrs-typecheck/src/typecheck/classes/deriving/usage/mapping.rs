use super::super::syntax::{
    apply_expr, field_expr, global_expr, lambda, local_expr, record_update,
};
use super::*;

#[derive(Clone, Copy)]
pub(in crate::typecheck::classes::deriving) struct MappingMethods {
    pub mono: Option<SymbolId>,
    pub bi: Option<SymbolId>,
    pub contra: Option<SymbolId>,
    pub pro: Option<SymbolId>,
    pub pro_left: Option<SymbolId>,
}

impl Checker {
    /// Builds exactly the mapping accepted by field-usage analysis. An inert
    /// nested argument receives identity, including the unused side of bimap.
    pub(in crate::typecheck::classes::deriving) fn usage_function(
        &mut self,
        usage: &FieldUsage,
        methods: &MappingMethods,
        left: &hir::Expr,
        right: &hir::Expr,
        span: TextRange,
    ) -> Option<hir::Expr> {
        Some(match usage {
            FieldUsage::Inert => {
                let binder = self.fresh_deriving_binder("__derived_identity", span);
                lambda(binder.clone(), local_expr(binder.id, span), span)
            }
            FieldUsage::Param => right.clone(),
            FieldUsage::LParam => left.clone(),
            FieldUsage::Mono(inner) => apply_expr(
                global_expr(methods.mono?, span),
                self.usage_function(inner, methods, left, right, span)?,
                span,
            ),
            FieldUsage::Contra(inner) => apply_expr(
                global_expr(methods.contra?, span),
                self.usage_function(inner, methods, left, right, span)?,
                span,
            ),
            FieldUsage::Bi(l, r) => apply_expr(
                apply_expr(
                    global_expr(methods.bi?, span),
                    self.usage_function(l, methods, left, right, span)?,
                    span,
                ),
                self.usage_function(r, methods, left, right, span)?,
                span,
            ),
            FieldUsage::Pro(l, r) if **r == FieldUsage::Inert && methods.pro_left.is_some() => {
                apply_expr(
                    global_expr(methods.pro_left?, span),
                    self.usage_function(l, methods, left, right, span)?,
                    span,
                )
            }
            FieldUsage::Pro(l, r) => apply_expr(
                apply_expr(
                    global_expr(methods.pro?, span),
                    self.usage_function(l, methods, left, right, span)?,
                    span,
                ),
                self.usage_function(r, methods, left, right, span)?,
                span,
            ),
            FieldUsage::Record(fields) => {
                let binder = self.fresh_deriving_binder("__derived_record", span);
                let value = local_expr(binder.id, span);
                let mut updates = Vec::new();
                for (label, usage) in fields {
                    if *usage != FieldUsage::Inert {
                        let field = field_expr(value.clone(), label, span);
                        updates.push((
                            label.clone(),
                            self.map_field_usage(usage, methods, left, right, &field, span)?,
                        ));
                    }
                }
                lambda(binder, record_update(value, updates, span), span)
            }
        })
    }

    pub(in crate::typecheck::classes::deriving) fn map_field_usage(
        &mut self,
        usage: &FieldUsage,
        methods: &MappingMethods,
        left: &hir::Expr,
        right: &hir::Expr,
        value: &hir::Expr,
        span: TextRange,
    ) -> Option<hir::Expr> {
        match usage {
            FieldUsage::Inert => Some(value.clone()),
            _ => Some(apply_expr(
                self.usage_function(usage, methods, left, right, span)?,
                value.clone(),
                span,
            )),
        }
    }
}
