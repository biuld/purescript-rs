use super::{FunctionFlow, function};
use crate::{Expr, ExprKind, Module, VerifyError};
use psrs_hir::ModuleId;

pub(super) fn expression<'a>(
    module: &'a Module,
    owner: ModuleId,
    value: &'a Expr,
    flows: &mut Vec<FunctionFlow<'a>>,
    errors: &mut Vec<VerifyError>,
) {
    psrs_span::with_sufficient_stack(|| {
        if let ExprKind::Lambda { binder, body } = &value.kind
            && crate::state::region(module, binder.ty).is_some()
            && crate::state::step(module, body.ty).is_some()
        {
            match function(module, owner, value) {
                Ok(flow) => flows.push(flow),
                Err(error) => errors.push(error),
            }
        }
        let mut visit = |child| expression(module, owner, child, flows, errors);
        match &value.kind {
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
                for (_, field) in fields {
                    visit(field);
                }
            }
            ExprKind::RecordUpdate { record, fields } => {
                visit(record);
                for (_, field) in fields {
                    visit(field);
                }
            }
            ExprKind::FieldAccess { record, .. }
            | ExprKind::RepresentationCast { value: record, .. } => visit(record),
            ExprKind::Application(function, argument) => {
                visit(function);
                visit(argument);
            }
            ExprKind::Lambda { body, .. } => visit(body),
            ExprKind::Let { bindings, body } => {
                for binding in bindings {
                    visit(&binding.value);
                }
                visit(body);
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
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                visit(scrutinee);
                for branch in branches {
                    visit(&branch.value);
                }
            }
            _ => {}
        }
    });
}
