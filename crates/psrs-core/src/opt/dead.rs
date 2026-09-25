use super::effects;
use super::util::with_span;
use crate::{Binding, Expr, ExprKind, Module};
use psrs_hir::LocalId;
use std::collections::{HashMap, HashSet, VecDeque};

pub(super) fn run(mut module: Module) -> Module {
    for declaration in &mut module.declarations {
        declaration.value = eliminate_expr(declaration.value.clone());
    }
    module
}

fn eliminate_expr(mut expression: Expr) -> Expr {
    expression.kind = match expression.kind {
        ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
            symbol,
            arguments: arguments.into_iter().map(eliminate_expr).collect(),
        },
        ExprKind::Array { elements } => ExprKind::Array {
            elements: elements.into_iter().map(eliminate_expr).collect(),
        },
        ExprKind::Record { fields } => ExprKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, eliminate_expr(value)))
                .collect(),
        },
        ExprKind::RecordUpdate { record, fields } => ExprKind::RecordUpdate {
            record: Box::new(eliminate_expr(*record)),
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, eliminate_expr(value)))
                .collect(),
        },
        ExprKind::FieldAccess { record, field } => ExprKind::FieldAccess {
            record: Box::new(eliminate_expr(*record)),
            field,
        },
        ExprKind::ArrayLength(array) => ExprKind::ArrayLength(Box::new(eliminate_expr(*array))),
        ExprKind::UnaryPrimitive { op, value } => ExprKind::UnaryPrimitive {
            op,
            value: Box::new(eliminate_expr(*value)),
        },
        ExprKind::ArrayIndex { array, index } => ExprKind::ArrayIndex {
            array: Box::new(eliminate_expr(*array)),
            index: Box::new(eliminate_expr(*index)),
        },
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => ExprKind::ArrayUpdate {
            array: Box::new(eliminate_expr(*array)),
            index: Box::new(eliminate_expr(*index)),
            value: Box::new(eliminate_expr(*value)),
        },
        ExprKind::Primitive { op, left, right } => ExprKind::Primitive {
            op,
            left: Box::new(eliminate_expr(*left)),
            right: Box::new(eliminate_expr(*right)),
        },
        ExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(eliminate_expr(*function)),
            Box::new(eliminate_expr(*argument)),
        ),
        ExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder,
            body: Box::new(eliminate_expr(*body)),
        },
        ExprKind::Let { bindings, body } => {
            let bindings = bindings
                .into_iter()
                .map(|mut binding| {
                    binding.value = eliminate_expr(binding.value);
                    binding
                })
                .collect::<Vec<_>>();
            let body = eliminate_expr(*body);
            let bindings = live_bindings(&bindings, &body);
            if bindings.is_empty() {
                return with_span(body, expression.span);
            }
            ExprKind::Let {
                bindings,
                body: Box::new(body),
            }
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(eliminate_expr(*condition)),
            then_branch: Box::new(eliminate_expr(*then_branch)),
            else_branch: Box::new(eliminate_expr(*else_branch)),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(eliminate_expr(*scrutinee)),
            branches: branches
                .into_iter()
                .map(|mut branch| {
                    branch.value = eliminate_expr(branch.value);
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

fn live_bindings(bindings: &[Binding], body: &Expr) -> Vec<Binding> {
    let indices = bindings
        .iter()
        .enumerate()
        .map(|(index, binding)| (binding.binder.id, index))
        .collect::<HashMap<_, _>>();
    let mut live = HashSet::new();
    let mut queue = VecDeque::new();

    let mut body_references = HashSet::new();
    collect_refs(body, &mut body_references);
    for id in body_references {
        if let Some(index) = indices.get(&id).copied()
            && live.insert(index)
        {
            queue.push_back(index);
        }
    }
    for (index, binding) in bindings.iter().enumerate() {
        if !effects::summarize(&binding.value).inert() && live.insert(index) {
            queue.push_back(index);
        }
    }

    while let Some(index) = queue.pop_front() {
        let mut references = HashSet::new();
        collect_refs(&bindings[index].value, &mut references);
        for id in references {
            if let Some(dependency) = indices.get(&id).copied()
                && live.insert(dependency)
            {
                queue.push_back(dependency);
            }
        }
    }

    bindings
        .iter()
        .enumerate()
        .filter(|(index, _)| live.contains(index))
        .map(|(_, binding)| binding.clone())
        .collect()
}

fn collect_refs(expression: &Expr, references: &mut HashSet<LocalId>) {
    match &expression.kind {
        ExprKind::Local(id) => {
            references.insert(*id);
        }
        ExprKind::Constructor { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => {
            for argument in arguments {
                collect_refs(argument, references);
            }
        }
        ExprKind::Record { fields } => {
            for (_, value) in fields {
                collect_refs(value, references);
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            collect_refs(record, references);
            for (_, value) in fields {
                collect_refs(value, references);
            }
        }
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            collect_refs(record, references)
        }
        ExprKind::UnaryPrimitive { value, .. } => collect_refs(value, references),
        ExprKind::ArrayIndex { array, index } => {
            collect_refs(array, references);
            collect_refs(index, references);
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            collect_refs(array, references);
            collect_refs(index, references);
            collect_refs(value, references);
        }
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            collect_refs(left, references);
            collect_refs(right, references);
        }
        ExprKind::Lambda { body, .. } => collect_refs(body, references),
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                collect_refs(&binding.value, references);
            }
            collect_refs(body, references);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_refs(condition, references);
            collect_refs(then_branch, references);
            collect_refs(else_branch, references);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_refs(scrutinee, references);
            for branch in branches {
                collect_refs(&branch.value, references);
            }
        }
        ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}
