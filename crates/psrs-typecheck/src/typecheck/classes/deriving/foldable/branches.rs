use super::*;
use crate::typecheck::classes::deriving::syntax::field_expr;
use crate::typecheck::classes::deriving::syntax::lambda;

impl Checker {
    fn require_fold_symbol(
        &mut self,
        symbol: Option<SymbolId>,
        span: TextRange,
        operation: &str,
    ) -> Option<SymbolId> {
        symbol.or_else(|| {
            self.deriving_error(
                TypeCheckErrorKind::CannotFindDerivingType,
                span,
                &format!("cannot find the `{operation}` value required to derive a fold"),
            )
        })
    }

    pub(super) fn fold_map_branch(
        &mut self,
        binders: &[hir::LocalBinder],
        usages: &[FieldUsage],
        context: &FoldContext<'_>,
    ) -> Option<hir::Expr> {
        let FoldContext {
            ref ops,
            left,
            right,
            span,
        } = *context;
        let mut contributions = Vec::new();
        for (binder, usage) in binders.iter().zip(usages) {
            if *usage == FieldUsage::Inert {
                continue;
            }
            let field = local_expr(binder.id, span);
            let function = self.fold_map_function(usage, ops, left, right, span)?;
            contributions.push(apply_expr(function, field, span));
        }
        self.fold_contributions(contributions, ops, span)
    }

    fn fold_contributions(
        &mut self,
        mut contributions: Vec<hir::Expr>,
        ops: &FoldOps,
        span: TextRange,
    ) -> Option<hir::Expr> {
        let mut body = match contributions.pop() {
            Some(last) => last,
            None => {
                return Some(global_expr(
                    self.require_fold_symbol(ops.mempty, span, "mempty")?,
                    span,
                ));
            }
        };
        for contribution in contributions.into_iter().rev() {
            body = apply_expr(
                apply_expr(
                    global_expr(self.require_fold_symbol(ops.append, span, "append")?, span),
                    contribution,
                    span,
                ),
                body,
                span,
            );
        }
        Some(body)
    }

    /// A function `x -> m` for a field occurrence.
    fn fold_map_function(
        &mut self,
        usage: &FieldUsage,
        ops: &FoldOps,
        left: &hir::Expr,
        right: &hir::Expr,
        span: TextRange,
    ) -> Option<hir::Expr> {
        Some(match usage {
            FieldUsage::Inert => {
                let ignored = self.fresh_deriving_binder("__derived_ignored", span);
                lambda(
                    ignored,
                    global_expr(self.require_fold_symbol(ops.mempty, span, "mempty")?, span),
                    span,
                )
            }
            FieldUsage::Record(fields) => {
                let binder = self.fresh_deriving_binder("__derived_record", span);
                let record = local_expr(binder.id, span);
                let mut contributions = Vec::new();
                for (label, usage) in fields {
                    if *usage != FieldUsage::Inert {
                        let function = self.fold_map_function(usage, ops, left, right, span)?;
                        contributions.push(apply_expr(
                            function,
                            field_expr(record.clone(), label, span),
                            span,
                        ));
                    }
                }
                let body = self.fold_contributions(contributions, ops, span)?;
                lambda(binder, body, span)
            }
            FieldUsage::Param => right.clone(),
            FieldUsage::LParam => left.clone(),
            FieldUsage::Mono(inner) => apply_expr(
                global_expr(
                    self.require_fold_symbol(ops.fold_map, span, "foldMap")?,
                    span,
                ),
                self.fold_map_function(inner, ops, left, right, span)?,
                span,
            ),
            FieldUsage::Bi(l, r) => apply_expr(
                apply_expr(
                    global_expr(
                        self.require_fold_symbol(ops.bifold_map, span, "bifoldMap")?,
                        span,
                    ),
                    self.fold_map_function(l, ops, left, right, span)?,
                    span,
                ),
                self.fold_map_function(r, ops, left, right, span)?,
                span,
            ),
            FieldUsage::Contra(inner) | FieldUsage::Pro(inner, _) => {
                self.fold_map_function(inner, ops, left, right, span)?
            }
        })
    }

    pub(super) fn fold_r_branch(
        &mut self,
        binders: &[hir::LocalBinder],
        usages: &[FieldUsage],
        accumulator: &hir::Expr,
        context: &FoldContext<'_>,
    ) -> Option<hir::Expr> {
        let FoldContext {
            ref ops,
            left,
            right,
            span,
        } = *context;
        let mut body = accumulator.clone();
        for (binder, usage) in binders.iter().zip(usages).rev() {
            if *usage == FieldUsage::Inert {
                continue;
            }
            let field = local_expr(binder.id, span);
            let step = self.fold_r_step(usage, ops, left, right, span)?;
            body = apply_expr(apply_expr(step, field, span), body, span);
        }
        Some(body)
    }

    /// A `foldr` step `x -> rest -> b`.
    fn fold_r_step(
        &mut self,
        usage: &FieldUsage,
        ops: &FoldOps,
        left: &hir::Expr,
        right: &hir::Expr,
        span: TextRange,
    ) -> Option<hir::Expr> {
        Some(match usage {
            FieldUsage::Inert => self.ignore_step(span, false),
            FieldUsage::Record(fields) => {
                self.record_fold_step(fields, ops, left, right, span, false)?
            }
            FieldUsage::Param => right.clone(),
            FieldUsage::LParam => left.clone(),
            FieldUsage::Mono(inner) => {
                let step = self.fold_r_step(inner, ops, left, right, span)?;
                let method = self.require_fold_symbol(ops.fold_r, span, "foldr")?;
                self.nested_fold_step(method, step, span, false)
            }
            FieldUsage::Bi(l, r) => {
                let l_step = self.fold_r_step(l, ops, left, right, span)?;
                let r_step = self.fold_r_step(r, ops, left, right, span)?;
                let method = self.require_fold_symbol(ops.bifold_r, span, "bifoldr")?;
                self.nested_bifold_step(method, l_step, r_step, span, false)
            }
            FieldUsage::Contra(inner) | FieldUsage::Pro(inner, _) => {
                let step = self.fold_r_step(inner, ops, left, right, span)?;
                let method = self.require_fold_symbol(ops.fold_r, span, "foldr")?;
                self.nested_fold_step(method, step, span, false)
            }
        })
    }

    pub(super) fn fold_l_branch(
        &mut self,
        binders: &[hir::LocalBinder],
        usages: &[FieldUsage],
        accumulator: &hir::Expr,
        context: &FoldContext<'_>,
    ) -> Option<hir::Expr> {
        let FoldContext {
            ref ops,
            left,
            right,
            span,
        } = *context;
        let mut body = accumulator.clone();
        for (binder, usage) in binders.iter().zip(usages) {
            if *usage == FieldUsage::Inert {
                continue;
            }
            let field = local_expr(binder.id, span);
            let step = self.fold_l_step(usage, ops, left, right, span)?;
            body = apply_expr(apply_expr(step, body, span), field, span);
        }
        Some(body)
    }

    /// A `foldl` step `rest -> x -> b`.
    fn fold_l_step(
        &mut self,
        usage: &FieldUsage,
        ops: &FoldOps,
        left: &hir::Expr,
        right: &hir::Expr,
        span: TextRange,
    ) -> Option<hir::Expr> {
        Some(match usage {
            FieldUsage::Inert => self.ignore_step(span, true),
            FieldUsage::Record(fields) => {
                self.record_fold_step(fields, ops, left, right, span, true)?
            }
            FieldUsage::Param => right.clone(),
            FieldUsage::LParam => left.clone(),
            FieldUsage::Mono(inner) => {
                let step = self.fold_l_step(inner, ops, left, right, span)?;
                let method = self.require_fold_symbol(ops.fold_l, span, "foldl")?;
                self.nested_fold_step(method, step, span, true)
            }
            FieldUsage::Bi(l, r) => {
                let l_step = self.fold_l_step(l, ops, left, right, span)?;
                let r_step = self.fold_l_step(r, ops, left, right, span)?;
                let method = self.require_fold_symbol(ops.bifold_l, span, "bifoldl")?;
                self.nested_bifold_step(method, l_step, r_step, span, true)
            }
            FieldUsage::Contra(inner) | FieldUsage::Pro(inner, _) => {
                let step = self.fold_l_step(inner, ops, left, right, span)?;
                let method = self.require_fold_symbol(ops.fold_l, span, "foldl")?;
                self.nested_fold_step(method, step, span, true)
            }
        })
    }

    fn record_fold_step(
        &mut self,
        fields: &[(String, FieldUsage)],
        ops: &FoldOps,
        left: &hir::Expr,
        right: &hir::Expr,
        span: TextRange,
        forward: bool,
    ) -> Option<hir::Expr> {
        let record = self.fresh_deriving_binder("__derived_record", span);
        let acc = self.fresh_deriving_binder("__derived_acc", span);
        let mut body = local_expr(acc.id, span);
        let ordered: Box<dyn Iterator<Item = &(String, FieldUsage)>> = if forward {
            Box::new(fields.iter())
        } else {
            Box::new(fields.iter().rev())
        };
        for (label, usage) in ordered {
            if *usage == FieldUsage::Inert {
                continue;
            }
            let field = field_expr(local_expr(record.id, span), label, span);
            let step = if forward {
                self.fold_l_step(usage, ops, left, right, span)?
            } else {
                self.fold_r_step(usage, ops, left, right, span)?
            };
            body = if forward {
                apply_expr(apply_expr(step, body, span), field, span)
            } else {
                apply_expr(apply_expr(step, field, span), body, span)
            };
        }
        Some(if forward {
            lambda(acc, lambda(record, body, span), span)
        } else {
            lambda(record, lambda(acc, body, span), span)
        })
    }

    /// A step that ignores its element and returns the accumulator.
    fn ignore_step(&mut self, span: TextRange, left: bool) -> hir::Expr {
        let accumulator = self.fresh_deriving_binder("__derived_rest", span);
        let element = self.fresh_deriving_binder("__derived_ignored", span);
        let (first, second) = if left {
            (accumulator.clone(), element)
        } else {
            (element, accumulator.clone())
        };
        lambda(
            first,
            lambda(second, local_expr(accumulator.id, span), span),
            span,
        )
    }

    /// `\x rest -> method step rest x` (right) or `\rest x -> method step rest x` (left).
    fn nested_fold_step(
        &mut self,
        method: SymbolId,
        step: hir::Expr,
        span: TextRange,
        left: bool,
    ) -> hir::Expr {
        let accumulator = self.fresh_deriving_binder("__derived_rest", span);
        let element = self.fresh_deriving_binder("__derived_x", span);
        let (first, second) = if left {
            (accumulator.clone(), element.clone())
        } else {
            (element.clone(), accumulator.clone())
        };
        lambda(
            first,
            lambda(
                second,
                apply_expr(
                    apply_expr(
                        apply_expr(global_expr(method, span), step, span),
                        local_expr(accumulator.id, span),
                        span,
                    ),
                    local_expr(element.id, span),
                    span,
                ),
                span,
            ),
            span,
        )
    }

    /// The bipartite version of `nested_fold_step`.
    fn nested_bifold_step(
        &mut self,
        method: SymbolId,
        l_step: hir::Expr,
        r_step: hir::Expr,
        span: TextRange,
        left: bool,
    ) -> hir::Expr {
        let accumulator = self.fresh_deriving_binder("__derived_rest", span);
        let element = self.fresh_deriving_binder("__derived_x", span);
        let (first, second) = if left {
            (accumulator.clone(), element.clone())
        } else {
            (element.clone(), accumulator.clone())
        };
        lambda(
            first,
            lambda(
                second,
                apply_expr(
                    apply_expr(
                        apply_expr(
                            apply_expr(global_expr(method, span), l_step, span),
                            r_step,
                            span,
                        ),
                        local_expr(accumulator.id, span),
                        span,
                    ),
                    local_expr(element.id, span),
                    span,
                ),
                span,
            ),
            span,
        )
    }
}
