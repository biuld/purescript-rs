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
            ast::PatternKind::Boolean(value) => hir::PatternKind::Boolean(value),
            ast::PatternKind::Integer(value) => hir::PatternKind::Integer(value),
            ast::PatternKind::Number(value) => hir::PatternKind::Number(value),
            ast::PatternKind::String(value) => hir::PatternKind::String(value),
            ast::PatternKind::Char(value) => hir::PatternKind::Char(value),
            ast::PatternKind::Array { elements } => hir::PatternKind::Array(
                elements
                    .into_iter()
                    .map(|element| self.resolve_pattern(element, scope))
                    .collect::<Option<Vec<_>>>()?,
            ),
            ast::PatternKind::Var(binder) => {
                let binder = self.new_local(binder.name, binder.span);
                // Every occurrence keeps a stable identity. Pattern guards
                // permit overlapping names and expose the first one, matching purs.
                scope
                    .entry(binder.name.clone())
                    .or_insert_with(|| binder.clone());
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
            ast::PatternKind::Record { fields, mode } => hir::PatternKind::Record {
                fields: fields
                    .into_iter()
                    .map(|(label, pattern)| Some((label, self.resolve_pattern(pattern, scope)?)))
                    .collect::<Option<Vec<_>>>()?,
                mode: match mode {
                    ast::RecordPatternMode::Partial => hir::RecordPatternMode::Partial,
                    ast::RecordPatternMode::Exact => hir::RecordPatternMode::Exact,
                },
            },
            ast::PatternKind::Named { binder, pattern } => {
                let binder = self.new_local(binder.name, binder.span);
                // The alias is inserted after nested binders, so `name@pattern`
                // resolves references to the alias when the names overlap.
                let pattern = self.resolve_pattern(*pattern, scope)?;
                scope.insert(binder.name.clone(), binder.clone());
                hir::PatternKind::Named {
                    binder,
                    pattern: Box::new(pattern),
                }
            }
            ast::PatternKind::Typed { pattern, ty } => hir::PatternKind::Typed {
                pattern: Box::new(self.resolve_pattern(*pattern, scope)?),
                ty: self.resolve_type(ty)?,
            },
        };
        Some(hir::Pattern { kind, span })
    }

    pub(super) fn resolve_let(
        &mut self,
        declarations: Vec<ast::Declaration>,
        body: ast::Expr,
    ) -> Option<ExprKind> {
        let (bindings, scope) = self.resolve_local_bindings(declarations)?;
        self.scopes.push(scope);
        let body = self.resolve_expr(body);
        self.scopes.pop();
        Some(ExprKind::Let {
            bindings,
            body: Box::new(body?),
        })
    }

    pub(super) fn resolve_local_bindings(
        &mut self,
        declarations: Vec<ast::Declaration>,
    ) -> Option<(Vec<LocalBinding>, HashMap<String, LocalBinder>)> {
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
        self.scopes.push(scope.clone());
        let bindings = declarations
            .into_iter()
            .zip(binders)
            .map(|(declaration, binder)| {
                let span = declaration.span;
                let value = self.resolve_expr(declaration.value)?;
                let value = if let Some(annotation) = declaration.annotation {
                    hir::Expr {
                        kind: hir::ExprKind::Typed {
                            expression: Box::new(value),
                            ty: self.resolve_type(annotation)?,
                        },
                        span,
                    }
                } else {
                    value
                };
                Some(LocalBinding {
                    binder,
                    value,
                    span,
                })
            })
            .collect::<Option<Vec<_>>>();
        self.scopes.pop();
        Some((bindings?, scope))
    }
}
