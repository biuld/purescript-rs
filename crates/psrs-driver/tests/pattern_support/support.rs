use psrs_ast::{Expr, ExprKind, Guard, Pattern, PatternKind, Type, TypeKind};

pub(super) fn type_contains_wildcard(ty: &Type) -> bool {
    match &ty.kind {
        TypeKind::Wildcard => true,
        TypeKind::Application(function, argument) => {
            type_contains_wildcard(function) || type_contains_wildcard(argument)
        }
        TypeKind::Function { parameter, result } => {
            type_contains_wildcard(parameter) || type_contains_wildcard(result)
        }
        TypeKind::Forall { variables, body } => {
            variables
                .iter()
                .filter_map(|variable| variable.kind.as_ref())
                .any(type_contains_wildcard)
                || type_contains_wildcard(body)
        }
        TypeKind::Constrained { constraint, body } => {
            type_contains_wildcard(constraint) || type_contains_wildcard(body)
        }
        TypeKind::Row { fields, tail } | TypeKind::Record { fields, tail } => {
            fields.iter().any(|field| type_contains_wildcard(&field.ty))
                || tail.as_deref().is_some_and(type_contains_wildcard)
        }
        TypeKind::Name(_) | TypeKind::Integer(_) | TypeKind::String(_) => false,
        TypeKind::OperatorChain { operands, .. } => operands.iter().any(type_contains_wildcard),
    }
}

pub(super) fn visit_expr_patterns<'a>(expression: &'a Expr, output: &mut Vec<&'a Pattern>) {
    match &expression.kind {
        ExprKind::Lambda { body, .. }
        | ExprKind::FieldAccess {
            expression: body, ..
        }
        | ExprKind::Typed {
            expression: body, ..
        }
        | ExprKind::TypeApplication {
            expression: body, ..
        }
        | ExprKind::Negate {
            expression: body, ..
        }
        | ExprKind::OperatorSection { operand: body, .. } => visit_expr_patterns(body, output),
        ExprKind::Array(elements)
        | ExprKind::OperatorChain {
            operands: elements, ..
        } => {
            for element in elements {
                visit_expr_patterns(element, output);
            }
        }
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                visit_expr_patterns(value, output);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            visit_expr_patterns(expression, output);
            for (_, value) in fields {
                visit_expr_patterns(value, output);
            }
        }
        ExprKind::Application(function, argument)
        | ExprKind::Operator {
            left: function,
            right: argument,
            ..
        } => {
            visit_expr_patterns(function, output);
            visit_expr_patterns(argument, output);
        }
        ExprKind::Let { declarations, body } => {
            for declaration in declarations {
                visit_expr_patterns(&declaration.value, output);
            }
            visit_expr_patterns(body, output);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            visit_expr_patterns(condition, output);
            visit_expr_patterns(then_branch, output);
            visit_expr_patterns(else_branch, output);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            visit_expr_patterns(scrutinee, output);
            for branch in branches {
                visit_pattern(&branch.pattern, output);
                visit_expr_patterns(&branch.value, output);
            }
        }
        ExprKind::Guarded(clauses) => {
            for clause in clauses {
                for guard in &clause.guards {
                    if let Guard::Pattern { pattern, value } = guard {
                        visit_pattern(pattern, output);
                        visit_expr_patterns(value, output);
                    }
                }
                visit_expr_patterns(&clause.value, output);
            }
        }
        ExprKind::Name(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}

pub(super) fn visit_expr_guards<'a>(expression: &'a Expr, output: &mut Vec<&'a Guard>) {
    match &expression.kind {
        ExprKind::Lambda { body, .. }
        | ExprKind::FieldAccess {
            expression: body, ..
        }
        | ExprKind::Typed {
            expression: body, ..
        }
        | ExprKind::TypeApplication {
            expression: body, ..
        }
        | ExprKind::Negate {
            expression: body, ..
        }
        | ExprKind::OperatorSection { operand: body, .. } => visit_expr_guards(body, output),
        ExprKind::Array(elements)
        | ExprKind::OperatorChain {
            operands: elements, ..
        } => {
            for element in elements {
                visit_expr_guards(element, output);
            }
        }
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                visit_expr_guards(value, output);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            visit_expr_guards(expression, output);
            for (_, value) in fields {
                visit_expr_guards(value, output);
            }
        }
        ExprKind::Application(function, argument)
        | ExprKind::Operator {
            left: function,
            right: argument,
            ..
        } => {
            visit_expr_guards(function, output);
            visit_expr_guards(argument, output);
        }
        ExprKind::Let { declarations, body } => {
            for declaration in declarations {
                visit_expr_guards(&declaration.value, output);
            }
            visit_expr_guards(body, output);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            visit_expr_guards(condition, output);
            visit_expr_guards(then_branch, output);
            visit_expr_guards(else_branch, output);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            visit_expr_guards(scrutinee, output);
            for branch in branches {
                visit_expr_guards(&branch.value, output);
            }
        }
        ExprKind::Guarded(clauses) => {
            for clause in clauses {
                output.extend(clause.guards.iter());
                for guard in &clause.guards {
                    if let Guard::Pattern { value, .. } = guard {
                        visit_expr_guards(value, output);
                    }
                }
                visit_expr_guards(&clause.value, output);
            }
        }
        ExprKind::Name(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}

fn visit_pattern<'a>(pattern: &'a Pattern, output: &mut Vec<&'a Pattern>) {
    output.push(pattern);
    match &pattern.kind {
        PatternKind::Array { elements }
        | PatternKind::Constructor {
            arguments: elements,
            ..
        }
        | PatternKind::OperatorChain {
            operands: elements, ..
        } => {
            for element in elements {
                visit_pattern(element, output);
            }
        }
        PatternKind::Record { fields, .. } => {
            for (_, field) in fields {
                visit_pattern(field, output);
            }
        }
        PatternKind::Named { pattern, .. } | PatternKind::Typed { pattern, .. } => {
            visit_pattern(pattern, output);
        }
        _ => {}
    }
}
