//! Instantiate checked primitive callable values before runtime ABI erasure.
//!
//! This is mandatory elaboration, not budgeted optimization. Primitive globals
//! must use the checked type at each occurrence; adapting a polymorphic array
//! wrapper by copying its inputs would change mutation and reference identity.
use crate::locals::FreshLocals;
use crate::{Binder, Expr, ExprKind, Module, Type, TypeId, VerifyError, arrow_parts, scheme_parts};
use psrs_hir::{Intrinsic, SymbolId};
use psrs_span::TextRange;
use std::collections::HashMap;

/// The value operation of a checked explicit primitive. Unsafe coercion is a
/// representation boundary, matching source coercion lowering, not a target
/// arithmetic instruction. The caller validates its signature and operands.
pub fn primitive_value(
    intrinsic: Intrinsic,
    arguments: Vec<Expr>,
    result: TypeId,
    span: TextRange,
) -> Expr {
    let kind = if intrinsic == Intrinsic::UnsafeCoerce && arguments.len() == 1 {
        let value = arguments.into_iter().next().expect("one checked operand");
        ExprKind::RepresentationCast {
            source_type: value.ty,
            target_type: result,
            value: Box::new(value),
        }
    } else {
        ExprKind::IntrinsicCall {
            intrinsic,
            arguments,
        }
    };
    Expr {
        kind,
        ty: result,
        span,
    }
}

/// The caller supplies verified Core and validated operation identities. Work
/// on a candidate and verify the complete result before publishing it; failures
/// may leave this candidate partially rewritten.
pub fn expand_primitive_globals(
    module: &mut Module,
    primitives: &HashMap<SymbolId, Intrinsic>,
) -> Result<(), Vec<VerifyError>> {
    for declaration in &mut module.declarations {
        let mut fresh = FreshLocals::for_declaration(&declaration.value);
        rewrite(
            &mut declaration.value,
            &module.types,
            primitives,
            &mut fresh,
        )
        .map_err(|message| {
            vec![VerifyError {
                module: declaration.symbol.module,
                span: declaration.span,
                message,
            }]
        })?;
    }
    Ok(())
}

fn rewrite(
    expression: &mut Expr,
    types: &[Type],
    primitives: &HashMap<SymbolId, Intrinsic>,
    fresh: &mut FreshLocals,
) -> Result<(), &'static str> {
    match &mut expression.kind {
        ExprKind::Global(symbol) => {
            if let Some(intrinsic) = primitives.get(symbol) {
                let (_, mut cursor) = scheme_parts(types, expression.ty)
                    .ok_or("primitive value has an invalid checked scheme")?;
                let mut parameters = Vec::new();
                for index in 0..intrinsic.descriptor().arity {
                    let (parameter, result) = arrow_parts(types, cursor)
                        .ok_or("primitive value has an invalid checked function type")?;
                    let binder = Binder {
                        id: fresh
                            .fresh()
                            .ok_or("cannot allocate a primitive parameter")?,
                        name: format!("primitive_argument_{index}"),
                        ty: parameter,
                        span: expression.span,
                    };
                    parameters.push((binder, cursor));
                    cursor = result;
                }
                let mut body = primitive_value(
                    *intrinsic,
                    parameters
                        .iter()
                        .map(|(binder, _)| Expr {
                            kind: ExprKind::Local(binder.id),
                            ty: binder.ty,
                            span: expression.span,
                        })
                        .collect(),
                    cursor,
                    expression.span,
                );
                for (binder, ty) in parameters.into_iter().rev() {
                    body = Expr {
                        kind: ExprKind::Lambda {
                            binder,
                            body: Box::new(body),
                        },
                        ty,
                        span: expression.span,
                    };
                }
                // Keep the occurrence's outer forall and its checked identities.
                body.ty = expression.ty;
                *expression = body;
            }
        }
        ExprKind::Constructor { arguments, .. }
        | ExprKind::IntrinsicCall { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => {
            for argument in arguments {
                rewrite(argument, types, primitives, fresh)?;
            }
        }
        ExprKind::Record { fields } => {
            for (_, value) in fields {
                rewrite(value, types, primitives, fresh)?;
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            rewrite(record, types, primitives, fresh)?;
            for (_, value) in fields {
                rewrite(value, types, primitives, fresh)?;
            }
        }
        ExprKind::FieldAccess { record, .. }
        | ExprKind::RepresentationCast { value: record, .. }
        | ExprKind::Lambda { body: record, .. } => rewrite(record, types, primitives, fresh)?,
        ExprKind::Application(function, argument) => {
            rewrite(function, types, primitives, fresh)?;
            rewrite(argument, types, primitives, fresh)?;
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                rewrite(&mut binding.value, types, primitives, fresh)?;
            }
            rewrite(body, types, primitives, fresh)?;
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            rewrite(condition, types, primitives, fresh)?;
            rewrite(then_branch, types, primitives, fresh)?;
            rewrite(else_branch, types, primitives, fresh)?;
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            rewrite(scrutinee, types, primitives, fresh)?;
            for branch in branches {
                rewrite(&mut branch.value, types, primitives, fresh)?;
            }
        }
        ExprKind::Local(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Unit
        | ExprKind::StateToken
        | ExprKind::Trap => {}
    }
    Ok(())
}
