use super::Resolver;
use psrs_ast as ast;
use psrs_hir as hir;
use std::collections::HashMap;

impl Resolver {
    pub(super) fn resolve_guarded_exprs(
        &mut self,
        clauses: Vec<ast::GuardedExpr>,
    ) -> Option<Vec<hir::GuardedExpr>> {
        let mut lowered = Vec::with_capacity(clauses.len());
        for clause in clauses {
            let outer_depth = self.scopes.len();
            let resolved = (|| {
                let (where_bindings, where_scope) =
                    self.resolve_local_bindings(clause.where_declarations)?;
                self.scopes.push(where_scope);
                let mut guards = Vec::with_capacity(clause.guards.len());
                for guard in clause.guards {
                    match guard {
                        ast::Guard::Boolean(expression) => {
                            guards.push(hir::Guard::Boolean(self.resolve_expr(expression)?))
                        }
                        ast::Guard::Pattern { pattern, value } => {
                            let value = self.resolve_expr(value)?;
                            let mut scope = HashMap::new();
                            let pattern = self.resolve_pattern(pattern, &mut scope)?;
                            self.scopes.push(scope);
                            guards.push(hir::Guard::Pattern { pattern, value });
                        }
                        ast::Guard::Let { declarations, span } => {
                            let (bindings, scope) = self.resolve_local_bindings(declarations)?;
                            self.scopes.push(scope);
                            guards.push(hir::Guard::Let { bindings, span });
                        }
                    }
                }
                let value = self.resolve_expr(clause.value)?;
                Some(hir::GuardedExpr {
                    guards,
                    value,
                    where_bindings,
                    span: clause.span,
                })
            })();
            self.scopes.truncate(outer_depth);
            lowered.push(resolved?);
        }
        Some(lowered)
    }
}
