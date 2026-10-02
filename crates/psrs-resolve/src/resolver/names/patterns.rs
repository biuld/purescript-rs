use super::{ResolveErrorKind, Resolver};
use psrs_ast as ast;
use psrs_hir::{self as hir, ExprKind, LocalBinder, LocalBinding};
use std::collections::HashMap;

impl Resolver {
    pub(super) fn resolve_pattern(
        &mut self,
        pattern: ast::Pattern,
        scope: &mut HashMap<String, LocalBinder>,
    ) -> Option<hir::Pattern> {
        let span = pattern.span;
        let kind = match pattern.kind {
            ast::PatternKind::Wildcard => hir::PatternKind::Wildcard,
            ast::PatternKind::Var(binder) => {
                let binder = self.new_local(binder.name, binder.span);
                scope.insert(binder.name.clone(), binder.clone());
                hir::PatternKind::Var(binder)
            }
            ast::PatternKind::Constructor { name, arguments } => {
                let symbol = self.lookup_global(&name.text, name.span)?;
                hir::PatternKind::Constructor {
                    symbol,
                    name_span: name.span,
                    arguments: arguments
                        .into_iter()
                        .map(|argument| self.resolve_pattern(argument, scope))
                        .collect::<Option<Vec<_>>>()?,
                }
            }
            ast::PatternKind::OperatorChain {
                operands,
                operators,
            } => hir::PatternKind::OperatorChain {
                operands: operands
                    .into_iter()
                    .map(|operand| self.resolve_pattern(operand, scope))
                    .collect::<Option<Vec<_>>>()?,
                operators: operators
                    .into_iter()
                    .map(|operator| self.resolve_operator(&operator.name.text, operator.span))
                    .collect::<Option<Vec<_>>>()?,
            },
            ast::PatternKind::Record { fields } => hir::PatternKind::Record {
                fields: fields
                    .into_iter()
                    .map(|(label, pattern)| Some((label, self.resolve_pattern(pattern, scope)?)))
                    .collect::<Option<Vec<_>>>()?,
            },
        };
        Some(hir::Pattern { kind, span })
    }

    pub(super) fn resolve_let(
        &mut self,
        declarations: Vec<ast::Declaration>,
        body: ast::Expr,
    ) -> Option<ExprKind> {
        let mut scope = HashMap::new();
        let mut binders = Vec::with_capacity(declarations.len());
        for declaration in &declarations {
            let binder = self.new_local(declaration.name.text.clone(), declaration.name.span);
            if scope.insert(binder.name.clone(), binder.clone()).is_some() {
                self.report(
                    ResolveErrorKind::DuplicateLocalBinding,
                    binder.name.clone(),
                    binder.span,
                );
            }
            binders.push(binder);
        }

        self.scopes.push(scope);
        let bindings = declarations
            .into_iter()
            .zip(binders)
            .filter_map(|(declaration, binder)| {
                let value = self.resolve_expr(declaration.value)?;
                Some(LocalBinding {
                    binder,
                    value,
                    span: declaration.span,
                })
            })
            .collect::<Vec<_>>();
        let body = self.resolve_expr(body);
        self.scopes.pop();
        Some(ExprKind::Let {
            bindings,
            body: Box::new(body?),
        })
    }
}
