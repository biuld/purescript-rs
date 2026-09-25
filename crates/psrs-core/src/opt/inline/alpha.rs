use super::super::util::FreshLocals;
use crate::{Binding, CaseBranch, Expr, ExprKind, Pattern, PatternKind};
use psrs_hir::LocalId;
use std::collections::HashMap;

pub(super) fn clone_with_fresh_locals(expression: &Expr, fresh: &mut FreshLocals) -> Option<Expr> {
    clone_expr(expression, fresh, &mut HashMap::new())
}

fn clone_expr(
    expression: &Expr,
    fresh: &mut FreshLocals,
    locals: &mut HashMap<LocalId, LocalId>,
) -> Option<Expr> {
    let kind = match &expression.kind {
        ExprKind::Local(id) => ExprKind::Local(locals.get(id).copied().unwrap_or(*id)),
        ExprKind::Global(symbol) => ExprKind::Global(*symbol),
        ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
            symbol: *symbol,
            arguments: arguments
                .iter()
                .map(|argument| clone_expr(argument, fresh, locals))
                .collect::<Option<Vec<_>>>()?,
        },
        ExprKind::Integer(value) => ExprKind::Integer(*value),
        ExprKind::Number(value) => ExprKind::Number(value.clone()),
        ExprKind::Boolean(value) => ExprKind::Boolean(*value),
        ExprKind::String(value) => ExprKind::String(value.clone()),
        ExprKind::Char(value) => ExprKind::Char(*value),
        ExprKind::Array { elements } => ExprKind::Array {
            elements: elements
                .iter()
                .map(|element| clone_expr(element, fresh, locals))
                .collect::<Option<Vec<_>>>()?,
        },
        ExprKind::Record { fields } => ExprKind::Record {
            fields: fields
                .iter()
                .map(|(label, value)| Some((label.clone(), clone_expr(value, fresh, locals)?)))
                .collect::<Option<Vec<_>>>()?,
        },
        ExprKind::RecordUpdate { record, fields } => ExprKind::RecordUpdate {
            record: Box::new(clone_expr(record, fresh, locals)?),
            fields: fields
                .iter()
                .map(|(label, value)| Some((label.clone(), clone_expr(value, fresh, locals)?)))
                .collect::<Option<Vec<_>>>()?,
        },
        ExprKind::FieldAccess { record, field } => ExprKind::FieldAccess {
            record: Box::new(clone_expr(record, fresh, locals)?),
            field: field.clone(),
        },
        ExprKind::ArrayLength(array) => {
            ExprKind::ArrayLength(Box::new(clone_expr(array, fresh, locals)?))
        }
        ExprKind::ArrayIndex { array, index } => ExprKind::ArrayIndex {
            array: Box::new(clone_expr(array, fresh, locals)?),
            index: Box::new(clone_expr(index, fresh, locals)?),
        },
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => ExprKind::ArrayUpdate {
            array: Box::new(clone_expr(array, fresh, locals)?),
            index: Box::new(clone_expr(index, fresh, locals)?),
            value: Box::new(clone_expr(value, fresh, locals)?),
        },
        ExprKind::Primitive { op, left, right } => ExprKind::Primitive {
            op: *op,
            left: Box::new(clone_expr(left, fresh, locals)?),
            right: Box::new(clone_expr(right, fresh, locals)?),
        },
        ExprKind::UnaryPrimitive { op, value } => ExprKind::UnaryPrimitive {
            op: *op,
            value: Box::new(clone_expr(value, fresh, locals)?),
        },
        ExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(clone_expr(function, fresh, locals)?),
            Box::new(clone_expr(argument, fresh, locals)?),
        ),
        ExprKind::Lambda { binder, body } => {
            let renamed = fresh.fresh()?;
            let previous = locals.insert(binder.id, renamed);
            let body = clone_expr(body, fresh, locals)?;
            restore_local(locals, binder.id, previous);
            ExprKind::Lambda {
                binder: crate::Binder {
                    id: renamed,
                    name: binder.name.clone(),
                    ty: binder.ty,
                    span: binder.span,
                },
                body: Box::new(body),
            }
        }
        ExprKind::Let { bindings, body } => {
            let mut renamed = Vec::with_capacity(bindings.len());
            let mut previous = Vec::with_capacity(bindings.len());
            for binding in bindings {
                let id = fresh.fresh()?;
                previous.push((binding.binder.id, locals.insert(binding.binder.id, id)));
                renamed.push(id);
            }
            let bindings = bindings
                .iter()
                .zip(renamed)
                .map(|(binding, id)| {
                    Some(Binding {
                        binder: crate::Binder {
                            id,
                            name: binding.binder.name.clone(),
                            ty: binding.binder.ty,
                            span: binding.binder.span,
                        },
                        quantified: binding.quantified.clone(),
                        value: clone_expr(&binding.value, fresh, locals)?,
                        span: binding.span,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            let body = clone_expr(body, fresh, locals)?;
            restore_locals(locals, previous);
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
            condition: Box::new(clone_expr(condition, fresh, locals)?),
            then_branch: Box::new(clone_expr(then_branch, fresh, locals)?),
            else_branch: Box::new(clone_expr(else_branch, fresh, locals)?),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(clone_expr(scrutinee, fresh, locals)?),
            branches: branches
                .iter()
                .map(|branch| {
                    let mut previous = Vec::new();
                    let pattern = clone_pattern(&branch.pattern, fresh, locals, &mut previous)?;
                    let value = clone_expr(&branch.value, fresh, locals)?;
                    restore_locals(locals, previous);
                    Some(CaseBranch {
                        pattern,
                        value,
                        span: branch.span,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        },
    };
    Some(Expr {
        kind,
        ty: expression.ty,
        span: expression.span,
    })
}

fn clone_pattern(
    pattern: &Pattern,
    fresh: &mut FreshLocals,
    locals: &mut HashMap<LocalId, LocalId>,
    previous: &mut Vec<(LocalId, Option<LocalId>)>,
) -> Option<Pattern> {
    let kind = match &pattern.kind {
        PatternKind::Wildcard => PatternKind::Wildcard,
        PatternKind::Var { id, ty } => {
            let renamed = fresh.fresh()?;
            previous.push((*id, locals.insert(*id, renamed)));
            PatternKind::Var {
                id: renamed,
                ty: *ty,
            }
        }
        PatternKind::Constructor { symbol, arguments } => PatternKind::Constructor {
            symbol: *symbol,
            arguments: arguments
                .iter()
                .map(|argument| clone_pattern(argument, fresh, locals, previous))
                .collect::<Option<Vec<_>>>()?,
        },
        PatternKind::Record { fields } => PatternKind::Record {
            fields: fields
                .iter()
                .map(|(label, pattern)| {
                    Some((
                        label.clone(),
                        clone_pattern(pattern, fresh, locals, previous)?,
                    ))
                })
                .collect::<Option<Vec<_>>>()?,
        },
    };
    Some(Pattern {
        kind,
        ty: pattern.ty,
        span: pattern.span,
    })
}

fn restore_local(locals: &mut HashMap<LocalId, LocalId>, id: LocalId, previous: Option<LocalId>) {
    if let Some(previous) = previous {
        locals.insert(id, previous);
    } else {
        locals.remove(&id);
    }
}

fn restore_locals(
    locals: &mut HashMap<LocalId, LocalId>,
    previous: Vec<(LocalId, Option<LocalId>)>,
) {
    for (id, old) in previous.into_iter().rev() {
        restore_local(locals, id, old);
    }
}
