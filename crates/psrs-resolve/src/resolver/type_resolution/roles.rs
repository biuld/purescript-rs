use psrs_ast as ast;
use psrs_hir as hir;

pub(super) fn lower_role_declaration(
    declaration: Option<ast::RoleDeclaration>,
) -> Option<hir::RoleDeclaration> {
    declaration.map(|declaration| hir::RoleDeclaration {
        roles: declaration
            .roles
            .into_iter()
            .map(|annotation| {
                let role = match annotation.role {
                    ast::TypeRole::Nominal => hir::Role::Nominal,
                    ast::TypeRole::Representational => hir::Role::Representational,
                    ast::TypeRole::Phantom => hir::Role::Phantom,
                };
                (role, annotation.span)
            })
            .collect(),
        span: declaration.span,
    })
}
