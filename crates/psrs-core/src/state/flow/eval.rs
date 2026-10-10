use super::{Builder, Path, Value};
use crate::{Expr, ExprKind, Pattern, PatternKind, VerifyError};
use std::collections::HashMap;

impl<'a> Builder<'a> {
    pub(super) fn eval(
        &mut self,
        expression: &'a Expr,
        path: &mut Path,
    ) -> Result<Value, VerifyError> {
        psrs_span::with_sufficient_stack(|| self.eval_inner(expression, path))
    }

    fn eval_inner(&mut self, expression: &'a Expr, path: &mut Path) -> Result<Value, VerifyError> {
        let value = match &expression.kind {
            ExprKind::Local(id) => path.locals.get(id).cloned().unwrap_or(Value::Opaque),
            ExprKind::Record { fields } => {
                let mut values = HashMap::new();
                for (label, field) in fields {
                    let value = self.eval(field, path)?;
                    if matches!(value, Value::Terminated) {
                        return Ok(value);
                    }
                    values.insert(label.clone(), value);
                }
                Value::Record(values)
            }
            ExprKind::FieldAccess { record, field } => match self.eval(record, path)? {
                Value::Record(mut fields) => fields.remove(field).unwrap_or(Value::Opaque),
                Value::Terminated => Value::Terminated,
                _ => Value::Opaque,
            },
            ExprKind::RecordUpdate { record, fields } => {
                let value = self.eval(record, path)?;
                if matches!(value, Value::Terminated) {
                    return Ok(value);
                }
                let mut values = match value {
                    Value::Record(values) => values,
                    _ => HashMap::new(),
                };
                for (label, field) in fields {
                    let value = self.eval(field, path)?;
                    if matches!(value, Value::Terminated) {
                        return Ok(value);
                    }
                    values.insert(label.clone(), value);
                }
                Value::Record(values)
            }
            ExprKind::Application(function, argument) => {
                if matches!(self.eval(function, path)?, Value::Terminated) {
                    return Ok(Value::Terminated);
                }
                let argument_value = self.eval(argument, path)?;
                if matches!(argument_value, Value::Terminated) {
                    return Ok(argument_value);
                }
                if let Some(region) = crate::state::region(self.module, argument.ty) {
                    if !self.module.types_equivalent(region, self.region) {
                        return Err(self.error(argument.span, "state call mixes checked regions"));
                    }
                    // Use the instantiated final callable, not a declaration's
                    // uninstantiated variables or an action constructor name.
                    let signature = crate::state::signature(self.module, function.ty)
                        .map_err(|message| self.error(function.span, message))?;
                    if !signature.parameters.is_empty() {
                        return Err(self.error(
                            function.span,
                            "state call is not at its invocation boundary",
                        ));
                    }
                    let input = self.state_value(&argument_value, path, argument.span)?;
                    let output = if super::summary::passthrough(self.module, function) {
                        input
                    } else {
                        self.transition(expression, input, path)?
                    };
                    Value::Record(HashMap::from([
                        ("state".into(), Value::State(output)),
                        ("value".into(), Value::Opaque),
                    ]))
                } else {
                    Value::Opaque
                }
            }
            ExprKind::Let { bindings, body } => {
                let previous = path.locals.clone();
                for binding in bindings {
                    let value = self.eval(&binding.value, path)?;
                    if matches!(value, Value::Terminated) {
                        return Ok(value);
                    }
                    path.locals.insert(binding.binder.id, value);
                }
                let value = self.eval(body, path)?;
                path.locals = previous;
                value
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if matches!(self.eval(condition, path)?, Value::Terminated) {
                    return Ok(Value::Terminated);
                }
                return self.alternatives(&[then_branch, else_branch], path, expression.span);
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                let value = self.eval(scrutinee, path)?;
                if matches!(value, Value::Terminated) {
                    return Ok(value);
                }
                if branches.len() == 1 {
                    let previous = path.locals.clone();
                    self.bind(&branches[0].pattern, value, path)?;
                    let value = self.eval(&branches[0].value, path)?;
                    path.locals = previous;
                    value
                } else {
                    return self.case_alternatives(value, branches, path, expression.span);
                }
            }
            ExprKind::Constructor { arguments, .. } => {
                let mut values = Vec::new();
                for argument in arguments {
                    let value = self.eval(argument, path)?;
                    if matches!(value, Value::Terminated) {
                        return Ok(value);
                    }
                    values.push(value);
                }
                Value::Constructor(values)
            }
            ExprKind::RepresentationCast { value, .. } => {
                // Checked casts retain provenance only for an equivalent
                // State/record contract; arbitrary unsafe transport is not a
                // source of a root dependency.
                let evaluated = self.eval(value, path)?;
                if matches!(evaluated, Value::Terminated)
                    || self.module.types_equivalent(value.ty, expression.ty)
                {
                    evaluated
                } else {
                    Value::Opaque
                }
            }
            ExprKind::IntrinsicCall { arguments, .. }
            | ExprKind::Array {
                elements: arguments,
            } => {
                for argument in arguments {
                    if matches!(self.eval(argument, path)?, Value::Terminated) {
                        return Ok(Value::Terminated);
                    }
                }
                Value::Opaque
            }
            ExprKind::Trap => Value::Terminated,
            // A lambda is inert here. Its own execution scope is checked by
            // scan, independently of the enclosing invocation's parameter.
            _ => Value::Opaque,
        };
        if crate::state::region(self.module, expression.ty).is_some()
            && !matches!(value, Value::State(_) | Value::Terminated)
        {
            return Err(self.error(
                expression.span,
                "state value has no checked producer provenance",
            ));
        }
        Ok(value)
    }

    pub(super) fn bind(
        &self,
        pattern: &Pattern,
        value: Value,
        path: &mut Path,
    ) -> Result<(), VerifyError> {
        match &pattern.kind {
            PatternKind::Var { id, .. } => {
                path.locals.insert(*id, value);
            }
            PatternKind::Named { id, pattern } => {
                path.locals.insert(*id, value.clone());
                self.bind(pattern, value, path)?;
            }
            PatternKind::Record { fields } => {
                let Value::Record(values) = value else {
                    // Opaque ordinary records carry no state evidence. Any
                    // later State use must reject rather than assume a root.
                    for (_, field) in fields {
                        self.bind(field, Value::Opaque, path)?;
                    }
                    return Ok(());
                };
                for (label, field) in fields {
                    self.bind(
                        field,
                        values.get(label).cloned().unwrap_or(Value::Opaque),
                        path,
                    )?;
                }
            }
            PatternKind::Constructor { arguments, .. } => {
                let values = if let Value::Constructor(values) = value {
                    values
                } else {
                    Vec::new()
                };
                for (index, field) in arguments.iter().enumerate() {
                    self.bind(
                        field,
                        values.get(index).cloned().unwrap_or(Value::Opaque),
                        path,
                    )?;
                }
            }
            PatternKind::Array { elements } => {
                for field in elements {
                    self.bind(field, Value::Opaque, path)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
