use crate::{Expr, ExprKind, Module, Type};
use psrs_hir::TypeVariableId;

pub(super) fn variable_span(module: &Module) -> u32 {
    let mut maximum = None;
    for ty in &module.types {
        match ty {
            Type::Variable(variable) => include(variable, &mut maximum),
            Type::ForAll { variables, .. } => {
                for variable in variables {
                    include(variable, &mut maximum);
                }
            }
            _ => {}
        }
    }
    for constructor in &module.constructors {
        for variable in &constructor.parameters {
            include(variable, &mut maximum);
        }
    }
    for declaration in &module.declarations {
        for variable in &declaration.quantified {
            include(variable, &mut maximum);
        }
        expression_span(&declaration.value, &mut maximum);
    }
    maximum.map_or(0, |variable: TypeVariableId| variable.0.saturating_add(1))
}

fn include(variable: &TypeVariableId, maximum: &mut Option<TypeVariableId>) {
    if maximum.is_none_or(|previous| previous < *variable) {
        *maximum = Some(*variable);
    }
}

fn expression_span(expression: &Expr, maximum: &mut Option<TypeVariableId>) {
    match &expression.kind {
        ExprKind::Constructor { arguments, .. }
        | ExprKind::IntrinsicCall { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => {
            for argument in arguments {
                expression_span(argument, maximum);
            }
        }
        ExprKind::Record { fields } => {
            for (_, value) in fields {
                expression_span(value, maximum);
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            expression_span(record, maximum);
            for (_, value) in fields {
                expression_span(value, maximum);
            }
        }
        ExprKind::FieldAccess { record, .. }
        | ExprKind::RepresentationCast { value: record, .. } => expression_span(record, maximum),
        ExprKind::Application(left, right) => {
            expression_span(left, maximum);
            expression_span(right, maximum);
        }
        ExprKind::Lambda { body, .. } => expression_span(body, maximum),
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                for variable in &binding.quantified {
                    include(variable, maximum);
                }
                expression_span(&binding.value, maximum);
            }
            expression_span(body, maximum);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expression_span(condition, maximum);
            expression_span(then_branch, maximum);
            expression_span(else_branch, maximum);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            expression_span(scrutinee, maximum);
            for branch in branches {
                expression_span(&branch.value, maximum);
            }
        }
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
        ExprKind::Unit | ExprKind::StateToken | ExprKind::Trap => {}
    }
}
