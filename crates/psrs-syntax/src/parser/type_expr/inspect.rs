use psrs_cst::{TypeExpr, TypeExprKind};

pub(crate) fn type_contains_wildcard(expression: &TypeExpr) -> bool {
    match &expression.kind {
        TypeExprKind::Wildcard(_) => true,
        TypeExprKind::Function { left, right, .. } => {
            type_contains_wildcard(left) || type_contains_wildcard(right)
        }
        TypeExprKind::Forall { body, .. } => type_contains_wildcard(body),
        TypeExprKind::Constrained {
            constraint, body, ..
        } => type_contains_wildcard(constraint) || type_contains_wildcard(body),
        TypeExprKind::Application(function, arguments) => {
            type_contains_wildcard(function) || arguments.iter().any(type_contains_wildcard)
        }
        TypeExprKind::Operator { left, right, .. } => {
            type_contains_wildcard(left) || type_contains_wildcard(right)
        }
        TypeExprKind::PrefixOperator { operand, .. } => type_contains_wildcard(operand),
        TypeExprKind::Parens { expression, .. } => type_contains_wildcard(expression),
        TypeExprKind::Tuple { items, .. } => items.iter().any(type_contains_wildcard),
        TypeExprKind::Row { fields, tail, .. } | TypeExprKind::Record { fields, tail, .. } => {
            fields
                .iter()
                .any(|field| type_contains_wildcard(&field.type_expr))
                || tail.as_deref().is_some_and(type_contains_wildcard)
        }
        TypeExprKind::KindAnnotation {
            expression, kind, ..
        } => type_contains_wildcard(expression) || type_contains_wildcard(kind),
        _ => false,
    }
}

pub(crate) fn type_contains_forall(expression: &TypeExpr) -> bool {
    match &expression.kind {
        TypeExprKind::Forall { .. } => true,
        TypeExprKind::Function { left, right, .. } => {
            type_contains_forall(left) || type_contains_forall(right)
        }
        TypeExprKind::Constrained {
            constraint, body, ..
        } => type_contains_forall(constraint) || type_contains_forall(body),
        TypeExprKind::Application(function, arguments) => {
            type_contains_forall(function) || arguments.iter().any(type_contains_forall)
        }
        TypeExprKind::Operator { left, right, .. } => {
            type_contains_forall(left) || type_contains_forall(right)
        }
        TypeExprKind::PrefixOperator { operand, .. } => type_contains_forall(operand),
        TypeExprKind::Parens { expression, .. } => type_contains_forall(expression),
        TypeExprKind::Tuple { items, .. } => items.iter().any(type_contains_forall),
        TypeExprKind::Row { fields, tail, .. } | TypeExprKind::Record { fields, tail, .. } => {
            fields
                .iter()
                .any(|field| type_contains_forall(&field.type_expr))
                || tail.as_deref().is_some_and(type_contains_forall)
        }
        TypeExprKind::KindAnnotation {
            expression, kind, ..
        } => type_contains_forall(expression) || type_contains_forall(kind),
        _ => false,
    }
}
