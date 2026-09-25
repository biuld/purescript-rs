use super::super::super::util::{FreshLocals, count_nodes, substitute_locals};
use super::super::alpha::clone_with_fresh_locals;
use super::analysis::{application_parts, function_arity};
use crate::{Binder, Binding, Declaration, Expr, ExprKind, Type};
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};

#[allow(clippy::too_many_arguments)]
pub(super) fn inline_named_global(
    application: &Expr,
    fresh: &mut FreshLocals,
    max_body_nodes: usize,
    declarations: &HashMap<SymbolId, Declaration>,
    recursive: &HashSet<SymbolId>,
    types: &[Type],
) -> Option<Expr> {
    let (head, arguments) = application_parts(application);
    let ExprKind::Global(symbol) = head.kind else {
        return None;
    };
    let declaration = declarations.get(&symbol)?;
    if !declaration.quantified.is_empty()
        || recursive.contains(&symbol)
        || count_nodes(&declaration.value) > max_body_nodes
    {
        return None;
    }
    let parameter_count = function_arity(declaration.ty, types)?;
    if parameter_count != arguments.len() {
        return None;
    }

    let mut body = clone_with_fresh_locals(&declaration.value, fresh)?;
    let mut parameters = Vec::with_capacity(parameter_count);
    for _ in 0..parameter_count {
        let ExprKind::Lambda {
            binder,
            body: inner,
        } = body.kind
        else {
            return None;
        };
        parameters.push(binder);
        body = *inner;
    }
    if matches!(body.kind, ExprKind::Lambda { .. }) {
        return None;
    }

    let mut bindings = Vec::with_capacity(parameter_count);
    let mut substitutions = HashMap::with_capacity(parameter_count);
    for (parameter, argument) in parameters.into_iter().zip(arguments) {
        let id = fresh.fresh()?;
        substitutions.insert(
            parameter.id,
            Expr {
                kind: ExprKind::Local(id),
                ty: parameter.ty,
                span: application.span,
            },
        );
        bindings.push(Binding {
            binder: Binder {
                id,
                name: format!("$p7_inline_{}", parameter.name),
                ty: parameter.ty,
                span: parameter.span,
            },
            quantified: Vec::new(),
            value: argument.clone(),
            span: application.span,
        });
    }
    let body = substitute_locals(&body, &substitutions);
    Some(Expr {
        kind: ExprKind::Let {
            bindings,
            body: Box::new(body),
        },
        ty: application.ty,
        span: application.span,
    })
}
