//! Lexical callback execution shares the caller's dependency path, not a root.
use super::{Builder, Path, Value};
use crate::{Expr, ExprKind, VerifyError};
use psrs_hir::LocalId;
use psrs_span::TextRange;
use std::collections::HashSet;

impl<'a> Builder<'a> {
    pub(super) fn closure(&mut self, expression: &'a Expr, path: &Path) -> Value {
        let index = self.closures.len();
        self.closures.push(expression);
        let mut free = HashSet::new();
        free_locals(expression, &HashSet::new(), &mut free);
        Value::Closure {
            index,
            captures: path
                .locals
                .iter()
                .filter(|(id, _)| free.contains(id))
                .map(|(id, value)| (*id, value.clone()))
                .collect(),
        }
    }

    pub(super) fn invoke_closure(
        &mut self,
        function: Value,
        argument: Value,
        path: &mut Path,
        span: TextRange,
    ) -> Result<Value, VerifyError> {
        let Value::Closure {
            index,
            mut captures,
        } = function
        else {
            if argument.carries_state() {
                return Err(self.error(
                    span,
                    "state-carrying argument requires a checked invocation scope",
                ));
            }
            return Ok(Value::Opaque);
        };
        if self.active_closures.contains(&index) {
            return Err(self.error(
                span,
                "recursive captured callback requires checked loop transport",
            ));
        }
        let ExprKind::Lambda { binder, body } = &self.closures[index].kind else {
            unreachable!("closure registration requires a lambda");
        };
        for captured in captures.values_mut() {
            captured.resolve(path);
        }
        captures.insert(binder.id, argument);
        let caller = std::mem::replace(&mut path.locals, captures);
        self.active_closures.push(index);
        let result = self.eval(body, path);
        self.active_closures.pop();
        path.locals = caller;
        result
    }
}

fn free_locals(expr: &Expr, bound: &HashSet<LocalId>, free: &mut HashSet<LocalId>) {
    psrs_span::with_sufficient_stack(|| {
        let mut visit = |value| free_locals(value, bound, free);
        match &expr.kind {
            ExprKind::Local(id) if !bound.contains(id) => {
                free.insert(*id);
            }
            ExprKind::Lambda { binder, body } => {
                let mut scope = bound.clone();
                scope.insert(binder.id);
                free_locals(body, &scope, free);
            }
            ExprKind::Let { bindings, body } => {
                let mut scope = bound.clone();
                scope.extend(bindings.iter().map(|binding| binding.binder.id));
                for binding in bindings {
                    free_locals(&binding.value, &scope, free);
                }
                free_locals(body, &scope, free);
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                visit(scrutinee);
                for branch in branches {
                    let mut scope = bound.clone();
                    bind_pattern(&branch.pattern, &mut scope);
                    free_locals(&branch.value, &scope, free);
                }
            }
            ExprKind::Constructor { arguments, .. }
            | ExprKind::IntrinsicCall { arguments, .. }
            | ExprKind::Array {
                elements: arguments,
            } => {
                for argument in arguments {
                    visit(argument);
                }
            }
            ExprKind::Record { fields } => {
                for (_, value) in fields {
                    visit(value);
                }
            }
            ExprKind::RecordUpdate { record, fields } => {
                visit(record);
                for (_, value) in fields {
                    visit(value);
                }
            }
            ExprKind::FieldAccess { record, .. }
            | ExprKind::RepresentationCast { value: record, .. } => visit(record),
            ExprKind::Application(function, argument) => {
                visit(function);
                visit(argument);
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                visit(condition);
                visit(then_branch);
                visit(else_branch);
            }
            _ => {}
        }
    });
}

fn bind_pattern(pattern: &crate::Pattern, bound: &mut HashSet<LocalId>) {
    use crate::PatternKind;
    match &pattern.kind {
        PatternKind::Var { id, .. } => {
            bound.insert(*id);
        }
        PatternKind::Named { id, pattern } => {
            bound.insert(*id);
            bind_pattern(pattern, bound);
        }
        PatternKind::Constructor { arguments, .. } => {
            for field in arguments {
                bind_pattern(field, bound);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                bind_pattern(field, bound);
            }
        }
        PatternKind::Array { elements } => {
            for element in elements {
                bind_pattern(element, bound);
            }
        }
        _ => {}
    }
}
