use super::Budget;
use super::util::{FreshLocals, count_nodes, next_locals, substitute_locals};
use crate::{Binding, Expr, ExprKind, Module};
use std::collections::HashMap;

pub(super) fn run(mut module: Module, budget: Budget) -> Module {
    if budget.max_inline_nodes == 0 || budget.max_inline_sites == 0 {
        return module;
    }
    let mut fresh = next_locals(&module);
    let mut sites_left = budget.max_inline_sites;
    for (declaration, fresh) in module.declarations.iter_mut().zip(&mut fresh) {
        declaration.value = inline_expr(
            declaration.value.clone(),
            fresh,
            &mut sites_left,
            budget.max_inline_nodes,
        );
    }
    module
}

fn inline_expr(
    mut expression: Expr,
    fresh: &mut FreshLocals,
    sites_left: &mut usize,
    max_body_nodes: usize,
) -> Expr {
    expression.kind = match expression.kind {
        ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
            symbol,
            arguments: arguments
                .into_iter()
                .map(|argument| inline_expr(argument, fresh, sites_left, max_body_nodes))
                .collect(),
        },
        ExprKind::Array { elements } => ExprKind::Array {
            elements: elements
                .into_iter()
                .map(|element| inline_expr(element, fresh, sites_left, max_body_nodes))
                .collect(),
        },
        ExprKind::Record { fields } => ExprKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| {
                    (label, inline_expr(value, fresh, sites_left, max_body_nodes))
                })
                .collect(),
        },
        ExprKind::RecordUpdate { record, fields } => ExprKind::RecordUpdate {
            record: Box::new(inline_expr(*record, fresh, sites_left, max_body_nodes)),
            fields: fields
                .into_iter()
                .map(|(label, value)| {
                    (label, inline_expr(value, fresh, sites_left, max_body_nodes))
                })
                .collect(),
        },
        ExprKind::FieldAccess { record, field } => ExprKind::FieldAccess {
            record: Box::new(inline_expr(*record, fresh, sites_left, max_body_nodes)),
            field,
        },
        ExprKind::ArrayLength(array) => ExprKind::ArrayLength(Box::new(inline_expr(
            *array,
            fresh,
            sites_left,
            max_body_nodes,
        ))),
        ExprKind::UnaryPrimitive { op, value } => ExprKind::UnaryPrimitive {
            op,
            value: Box::new(inline_expr(*value, fresh, sites_left, max_body_nodes)),
        },
        ExprKind::ArrayIndex { array, index } => ExprKind::ArrayIndex {
            array: Box::new(inline_expr(*array, fresh, sites_left, max_body_nodes)),
            index: Box::new(inline_expr(*index, fresh, sites_left, max_body_nodes)),
        },
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => ExprKind::ArrayUpdate {
            array: Box::new(inline_expr(*array, fresh, sites_left, max_body_nodes)),
            index: Box::new(inline_expr(*index, fresh, sites_left, max_body_nodes)),
            value: Box::new(inline_expr(*value, fresh, sites_left, max_body_nodes)),
        },
        ExprKind::Primitive { op, left, right } => ExprKind::Primitive {
            op,
            left: Box::new(inline_expr(*left, fresh, sites_left, max_body_nodes)),
            right: Box::new(inline_expr(*right, fresh, sites_left, max_body_nodes)),
        },
        ExprKind::Application(function, argument) => {
            let function = inline_expr(*function, fresh, sites_left, max_body_nodes);
            let argument = inline_expr(*argument, fresh, sites_left, max_body_nodes);
            let can_inline = *sites_left > 0
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
            body: Box::new(inline_expr(*body, fresh, sites_left, max_body_nodes)),
        },
        ExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|mut binding| {
                    binding.value = inline_expr(binding.value, fresh, sites_left, max_body_nodes);
                    binding
                })
                .collect(),
            body: Box::new(inline_expr(*body, fresh, sites_left, max_body_nodes)),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(inline_expr(*condition, fresh, sites_left, max_body_nodes)),
            then_branch: Box::new(inline_expr(*then_branch, fresh, sites_left, max_body_nodes)),
            else_branch: Box::new(inline_expr(*else_branch, fresh, sites_left, max_body_nodes)),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(inline_expr(*scrutinee, fresh, sites_left, max_body_nodes)),
            branches: branches
                .into_iter()
                .map(|mut branch| {
                    branch.value = inline_expr(branch.value, fresh, sites_left, max_body_nodes);
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
    };
    expression
}
