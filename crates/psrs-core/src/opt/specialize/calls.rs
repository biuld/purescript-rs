use super::State;
use crate::{Declaration, Expr, ExprKind, Module};
use psrs_hir::SymbolId;
use std::collections::HashMap;

pub(super) fn rewrite(
    expression: Expr,
    module: &mut Module,
    declarations: &HashMap<SymbolId, Declaration>,
    state: &mut State,
    pending: &mut Vec<Declaration>,
) -> Expr {
    let mut expression = expression;
    expression.kind = match expression.kind {
        ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
            symbol,
            arguments: arguments
                .into_iter()
                .map(|argument| rewrite(argument, module, declarations, state, pending))
                .collect(),
        },
        ExprKind::Array { elements } => ExprKind::Array {
            elements: elements
                .into_iter()
                .map(|element| rewrite(element, module, declarations, state, pending))
                .collect(),
        },
        ExprKind::Record { fields } => ExprKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, rewrite(value, module, declarations, state, pending)))
                .collect(),
        },
        ExprKind::RecordUpdate { record, fields } => ExprKind::RecordUpdate {
            record: Box::new(rewrite(*record, module, declarations, state, pending)),
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, rewrite(value, module, declarations, state, pending)))
                .collect(),
        },
        ExprKind::FieldAccess { record, field } => ExprKind::FieldAccess {
            record: Box::new(rewrite(*record, module, declarations, state, pending)),
            field,
        },
        ExprKind::ArrayLength(array) => ExprKind::ArrayLength(Box::new(rewrite(
            *array,
            module,
            declarations,
            state,
            pending,
        ))),
        ExprKind::ArrayIndex { array, index } => ExprKind::ArrayIndex {
            array: Box::new(rewrite(*array, module, declarations, state, pending)),
            index: Box::new(rewrite(*index, module, declarations, state, pending)),
        },
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => ExprKind::ArrayUpdate {
            array: Box::new(rewrite(*array, module, declarations, state, pending)),
            index: Box::new(rewrite(*index, module, declarations, state, pending)),
            value: Box::new(rewrite(*value, module, declarations, state, pending)),
        },
        ExprKind::Primitive { op, left, right } => ExprKind::Primitive {
            op,
            left: Box::new(rewrite(*left, module, declarations, state, pending)),
            right: Box::new(rewrite(*right, module, declarations, state, pending)),
        },
        ExprKind::UnaryPrimitive { op, value } => ExprKind::UnaryPrimitive {
            op,
            value: Box::new(rewrite(*value, module, declarations, state, pending)),
        },
        ExprKind::Application(function, argument) => {
            let mut application = Expr {
                kind: ExprKind::Application(
                    Box::new(rewrite(*function, module, declarations, state, pending)),
                    Box::new(rewrite(*argument, module, declarations, state, pending)),
                ),
                ty: expression.ty,
                span: expression.span,
            };
            if let Some((symbol, call_type)) = application_head(&application)
                && let Some(declaration) = declarations.get(&symbol)
                && let Some(specialized) = state.target(module, declaration, call_type, pending)
            {
                replace_application_head(&mut application, symbol, specialized);
            }
            return application;
        }
        ExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder,
            body: Box::new(rewrite(*body, module, declarations, state, pending)),
        },
        ExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|mut binding| {
                    binding.value = rewrite(binding.value, module, declarations, state, pending);
                    binding
                })
                .collect(),
            body: Box::new(rewrite(*body, module, declarations, state, pending)),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(rewrite(*condition, module, declarations, state, pending)),
            then_branch: Box::new(rewrite(*then_branch, module, declarations, state, pending)),
            else_branch: Box::new(rewrite(*else_branch, module, declarations, state, pending)),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(rewrite(*scrutinee, module, declarations, state, pending)),
            branches: branches
                .into_iter()
                .map(|mut branch| {
                    branch.value = rewrite(branch.value, module, declarations, state, pending);
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

fn application_head(expression: &Expr) -> Option<(SymbolId, crate::TypeId)> {
    let mut current = expression;
    while let ExprKind::Application(function, _) = &current.kind {
        current = function;
    }
    match current.kind {
        ExprKind::Global(symbol) => Some((symbol, current.ty)),
        _ => None,
    }
}

fn replace_application_head(expression: &mut Expr, from: SymbolId, to: SymbolId) {
    match &mut expression.kind {
        ExprKind::Application(function, _) => replace_application_head(function, from, to),
        ExprKind::Global(symbol) if *symbol == from => *symbol = to,
        _ => {}
    }
}
