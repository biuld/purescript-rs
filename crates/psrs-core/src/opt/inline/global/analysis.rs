use crate::{Expr, ExprKind, Type, TypeId};
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};

pub(super) fn application_parts(expression: &Expr) -> (&Expr, Vec<&Expr>) {
    let mut arguments = Vec::new();
    let mut head = expression;
    while let ExprKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function;
    }
    arguments.reverse();
    (head, arguments)
}

pub(super) fn function_arity(mut type_id: TypeId, types: &[Type]) -> Option<usize> {
    let mut count = 0;
    let mut visited = HashSet::new();
    while visited.insert(type_id) {
        match types.get(type_id.0 as usize)? {
            Type::Function { result, .. } => {
                count += 1;
                type_id = *result;
            }
            _ => return Some(count),
        }
    }
    None
}

pub(super) fn is_recursive(root: SymbolId, graph: &HashMap<SymbolId, Vec<SymbolId>>) -> bool {
    fn visit(
        root: SymbolId,
        symbol: SymbolId,
        graph: &HashMap<SymbolId, Vec<SymbolId>>,
        visited: &mut HashSet<SymbolId>,
    ) -> bool {
        if symbol == root {
            return true;
        }
        if !visited.insert(symbol) {
            return false;
        }
        graph.get(&symbol).is_some_and(|targets| {
            targets
                .iter()
                .any(|target| visit(root, *target, graph, visited))
        })
    }
    graph.get(&root).is_some_and(|targets| {
        targets
            .iter()
            .any(|target| visit(root, *target, graph, &mut HashSet::new()))
    })
}

pub(super) fn contains_case(expression: &Expr) -> bool {
    match &expression.kind {
        ExprKind::Case { .. } => true,
        ExprKind::Constructor { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => arguments.iter().any(contains_case),
        ExprKind::Record { fields } => fields.iter().any(|(_, value)| contains_case(value)),
        ExprKind::RecordUpdate { record, fields } => {
            contains_case(record) || fields.iter().any(|(_, value)| contains_case(value))
        }
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            contains_case(record)
        }
        ExprKind::UnaryPrimitive { value, .. } => contains_case(value),
        ExprKind::ArrayIndex { array, index } => contains_case(array) || contains_case(index),
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => contains_case(array) || contains_case(index) || contains_case(value),
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            contains_case(left) || contains_case(right)
        }
        ExprKind::Lambda { body, .. } => contains_case(body),
        ExprKind::Let { bindings, body } => {
            bindings.iter().any(|binding| contains_case(&binding.value)) || contains_case(body)
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => contains_case(condition) || contains_case(then_branch) || contains_case(else_branch),
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => false,
    }
}

pub(super) fn collect_globals(expression: &Expr, out: &mut Vec<SymbolId>) {
    match &expression.kind {
        ExprKind::Global(symbol) => out.push(*symbol),
        ExprKind::Constructor { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => {
            for argument in arguments {
                collect_globals(argument, out);
            }
        }
        ExprKind::Record { fields } => {
            for (_, value) in fields {
                collect_globals(value, out);
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            collect_globals(record, out);
            for (_, value) in fields {
                collect_globals(value, out);
            }
        }
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            collect_globals(record, out)
        }
        ExprKind::UnaryPrimitive { value, .. } => collect_globals(value, out),
        ExprKind::ArrayIndex { array, index } => {
            collect_globals(array, out);
            collect_globals(index, out);
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            collect_globals(array, out);
            collect_globals(index, out);
            collect_globals(value, out);
        }
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            collect_globals(left, out);
            collect_globals(right, out);
        }
        ExprKind::Lambda { body, .. } => collect_globals(body, out),
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                collect_globals(&binding.value, out);
            }
            collect_globals(body, out);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_globals(condition, out);
            collect_globals(then_branch, out);
            collect_globals(else_branch, out);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_globals(scrutinee, out);
            for branch in branches {
                collect_globals(&branch.value, out);
            }
        }
        ExprKind::Local(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}
