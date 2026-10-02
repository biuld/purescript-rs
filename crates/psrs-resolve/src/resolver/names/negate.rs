use super::{AstExprKind, Resolver, ast, hir};
use psrs_span::TextRange;

impl Resolver {
    pub(super) fn resolve_negate(
        &mut self,
        minus_span: TextRange,
        expression: ast::Expr,
    ) -> Option<hir::ExprKind> {
        let function = ast::Expr {
            kind: AstExprKind::Name(ast::Name {
                text: "negate".to_owned(),
                span: minus_span,
            }),
            span: minus_span,
        };
        let function = self.resolve_expr(function);
        let expression = self.resolve_expr(expression);
        Some(hir::ExprKind::Negate {
            function: Box::new(function?),
            minus_span,
            expression: Box::new(expression?),
        })
    }
}
