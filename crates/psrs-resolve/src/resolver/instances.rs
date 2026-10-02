use super::names::Resolver;
use super::{ResolveError, ast};
use psrs_hir::{self as hir, ModuleId, SymbolId, TypeId};

/// Resolves one `instance` declaration into HIR. Its head must name a class
/// applied to type arguments; each where-block member becomes a resolved method
/// implementation.
pub(super) fn resolve_instance(
    resolver: &mut Resolver,
    module_id: ModuleId,
    instance: ast::InstanceDeclaration,
    next_symbol: &mut u32,
    generated_name: &str,
) -> Option<hir::InstanceDeclaration> {
    let context = instance
        .context
        .into_iter()
        .map(|constraint| resolver.resolve_type(constraint))
        .collect::<Option<Vec<_>>>()?;
    let head = resolver.resolve_type(instance.head)?;
    let Some(class_id) = head_class_id(&head) else {
        resolver.errors.push(ResolveError::invalid_hir(
            instance.span,
            "an instance head must apply a class to its type arguments",
        ));
        return None;
    };
    let members = instance
        .members
        .into_iter()
        .filter_map(|member| {
            let signature = match member.annotation {
                Some(signature) => Some(resolver.resolve_type(signature)?),
                None => None,
            };
            let value = resolver.resolve_expr(member.value)?;
            let value = match signature {
                Some(ty) => hir::Expr {
                    kind: hir::ExprKind::Typed {
                        expression: Box::new(value),
                        ty,
                    },
                    span: member.span,
                },
                None => value,
            };
            Some(hir::InstanceMember {
                name: member.name.text,
                name_span: member.name.span,
                value,
                span: member.span,
            })
        })
        .collect::<Vec<_>>();
    let (name, name_span) = if instance.name.text.is_empty() {
        (generated_name.to_owned(), instance.span)
    } else {
        (instance.name.text.clone(), instance.name.span)
    };
    let symbol = SymbolId::new(module_id, *next_symbol);
    *next_symbol += 1;
    Some(hir::InstanceDeclaration {
        symbol,
        name,
        name_span,
        class_id,
        chain_id: instance.chain_id,
        chain_position: instance.chain_position,
        context,
        head,
        members,
        derivation: instance.derivation.map(|derivation| match derivation {
            ast::DerivationStrategy::KnownClass => hir::DerivationStrategy::KnownClass,
            ast::DerivationStrategy::Newtype => hir::DerivationStrategy::Newtype,
        }),
        span: instance.span,
    })
}

/// The class a resolved instance head applies, taken from the head of its
/// application spine.
fn head_class_id(ty: &hir::Type) -> Option<TypeId> {
    let mut head = ty;
    while let hir::TypeKind::Application(function, _) = &head.kind {
        head = function;
    }
    match &head.kind {
        hir::TypeKind::Named(id) => Some(*id),
        _ => None,
    }
}
