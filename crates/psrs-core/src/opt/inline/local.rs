use super::super::util::{FreshLocals, count_nodes, next_locals, substitute_locals};
use super::global::analysis::expr_introduces_type_binders;
use crate::{Binding, Expr, ExprKind, Module, Type};
use std::collections::HashMap;

pub(super) fn run(mut module: Module, max_inline_nodes: usize, sites_left: &mut usize) -> Module {
    if max_inline_nodes == 0 || *sites_left == 0 {
        return module;
    }
    let mut fresh = next_locals(&module);
    // Split the type table out so each declaration body can be rewritten
    // while the binder check still reads the types those bodies mention.
    let types = std::mem::take(&mut module.types);
    for (declaration, fresh) in module.declarations.iter_mut().zip(&mut fresh) {
        declaration.value = inline_expr(
            declaration.value.clone(),
            fresh,
            sites_left,
            max_inline_nodes,
            &types,
        );
    }
    module.types = types;
    module
}

fn inline_expr(
    expression: Expr,
    fresh: &mut FreshLocals,
    sites_left: &mut usize,
    max_body_nodes: usize,
    types: &[Type],
) -> Expr {
    psrs_span::with_sufficient_stack(|| {
        inline_expr_inner(expression, fresh, sites_left, max_body_nodes, types)
    })
}

fn inline_expr_inner(
    mut expression: Expr,
    fresh: &mut FreshLocals,
    sites_left: &mut usize,
    max_body_nodes: usize,
    types: &[Type],
) -> Expr {
    expression.kind = match expression.kind {
        ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
            symbol,
            arguments: arguments
                .into_iter()
                .map(|argument| inline_expr(argument, fresh, sites_left, max_body_nodes, types))
                .collect(),
        },
        ExprKind::IntrinsicCall {
            intrinsic,
            arguments,
        } => ExprKind::IntrinsicCall {
            intrinsic,
            arguments: arguments
                .into_iter()
                .map(|argument| inline_expr(argument, fresh, sites_left, max_body_nodes, types))
                .collect(),
        },
        ExprKind::Array { elements } => ExprKind::Array {
            elements: elements
                .into_iter()
                .map(|element| inline_expr(element, fresh, sites_left, max_body_nodes, types))
                .collect(),
        },
        ExprKind::Record { fields } => ExprKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| {
                    (
                        label,
                        inline_expr(value, fresh, sites_left, max_body_nodes, types),
                    )
                })
                .collect(),
        },
        ExprKind::RecordUpdate { record, fields } => ExprKind::RecordUpdate {
            record: Box::new(inline_expr(
                *record,
                fresh,
                sites_left,
                max_body_nodes,
                types,
            )),
            fields: fields
                .into_iter()
                .map(|(label, value)| {
                    (
                        label,
                        inline_expr(value, fresh, sites_left, max_body_nodes, types),
                    )
                })
                .collect(),
        },
        ExprKind::FieldAccess { record, field } => ExprKind::FieldAccess {
            record: Box::new(inline_expr(
                *record,
                fresh,
                sites_left,
                max_body_nodes,
                types,
            )),
            field,
        },
        ExprKind::RepresentationCast {
            value,
            source_type,
            target_type,
        } => ExprKind::RepresentationCast {
            value: Box::new(inline_expr(
                *value,
                fresh,
                sites_left,
                max_body_nodes,
                types,
            )),
            source_type,
            target_type,
        },
        ExprKind::Application(function, argument) => {
            let function = inline_expr(*function, fresh, sites_left, max_body_nodes, types);
            let argument = inline_expr(*argument, fresh, sites_left, max_body_nodes, types);
            // A lambda whose type binds quantifiers is the scope of those
            // variables. Copying its body into the call replaces that scope
            // with the instantiated result type.
            let can_inline = *sites_left > 0
                && !expr_introduces_type_binders(&function, types)
                && matches!(
                    &function.kind,
                    ExprKind::Lambda { body, .. } if count_nodes(body) <= max_body_nodes
                );
            if can_inline && let Some(id) = fresh.fresh() {
                let ExprKind::Lambda { binder, body } = function.kind else {
                    unreachable!("the lambda was checked above")
                };
                *sites_left -= 1;
                let replacement = Expr {
                    kind: ExprKind::Local(id),
                    ty: binder.ty,
                    span: expression.span,
                };
                let body = substitute_locals(&body, &HashMap::from([(binder.id, replacement)]));
                // The enclosing let's result quantifiers scope both its
                // argument binding and body; the binding stays monomorphic.
                ExprKind::Let {
                    bindings: vec![Binding {
                        binder: crate::Binder {
                            id,
                            name: format!("$p7_{}", binder.name),
                            ty: binder.ty,
                            span: binder.span,
                        },
                        quantified: Vec::new(),
                        value: argument,
                        span: expression.span,
                    }],
                    body: Box::new(body),
                }
            } else {
                ExprKind::Application(Box::new(function), Box::new(argument))
            }
        }
        ExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder,
            body: Box::new(inline_expr(*body, fresh, sites_left, max_body_nodes, types)),
        },
        ExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|mut binding| {
                    binding.value =
                        inline_expr(binding.value, fresh, sites_left, max_body_nodes, types);
                    binding
                })
                .collect(),
            body: Box::new(inline_expr(*body, fresh, sites_left, max_body_nodes, types)),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(inline_expr(
                *condition,
                fresh,
                sites_left,
                max_body_nodes,
                types,
            )),
            then_branch: Box::new(inline_expr(
                *then_branch,
                fresh,
                sites_left,
                max_body_nodes,
                types,
            )),
            else_branch: Box::new(inline_expr(
                *else_branch,
                fresh,
                sites_left,
                max_body_nodes,
                types,
            )),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(inline_expr(
                *scrutinee,
                fresh,
                sites_left,
                max_body_nodes,
                types,
            )),
            branches: branches
                .into_iter()
                .map(|mut branch| {
                    branch.value =
                        inline_expr(branch.value, fresh, sites_left, max_body_nodes, types);
                    branch
                })
                .collect(),
        },
        ExprKind::Local(id) => ExprKind::Local(id),
        ExprKind::Global(symbol) => ExprKind::Global(symbol),
        ExprKind::Integer(value) => ExprKind::Integer(value),
        ExprKind::Number(value) => ExprKind::Number(value),
        ExprKind::Boolean(value) => ExprKind::Boolean(value),
        ExprKind::String(value) => ExprKind::String(value),
        ExprKind::Char(value) => ExprKind::Char(value),
        ExprKind::Unit => ExprKind::Unit,
        ExprKind::StateToken => ExprKind::StateToken,
        ExprKind::Trap => ExprKind::Trap,
    };
    expression
}
