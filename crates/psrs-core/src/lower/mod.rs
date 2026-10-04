use crate::{Binder, Binding, Expr, ExprKind, LowerError, Module, TypeId};
use psrs_hir::{ExternalKind, SymbolId};
use psrs_thir::{Expr as TypedExpr, ExprKind as TypedExprKind};
use std::collections::HashMap;

mod dictionary;
mod module;

/// Lowers a module and verifies the result. A module with unresolved
/// cross-module global references cannot be verified on its own; use
/// [`lower_module_unverified`] and verify the linked module instead.
pub(super) fn lower_module(module: psrs_thir::Module) -> Result<Module, Vec<LowerError>> {
    let lowered = module::lower_module_inner(module)?;
    if lowered.verify().is_err() {
        return Err(vec![LowerError {
            span: lowered.span,
            message: "lowered Core failed verification",
        }]);
    }
    Ok(lowered)
}

/// Lowers a module without verifying the result, for linking.
pub(super) fn lower_module_unverified(
    module: psrs_thir::Module,
) -> Result<Module, Vec<LowerError>> {
    module::lower_module_inner(module)
}

fn lower_expr(
    expression: TypedExpr,
    externals: &HashMap<SymbolId, ExternalKind>,
    constructors: &HashMap<SymbolId, psrs_thir::ConstructorInfo>,
    source_types: &[psrs_thir::Type],
    context: &mut module::LowerContext,
) -> Result<Expr, LowerError> {
    let span = expression.span;
    let ty = TypeId(expression.ty.0);
    if let Some((symbol, arguments)) = constructor_application(&expression, constructors) {
        let arguments = arguments
            .into_iter()
            .map(|argument| {
                lower_expr(
                    argument.clone(),
                    externals,
                    constructors,
                    source_types,
                    context,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Expr {
            kind: ExprKind::Constructor { symbol, arguments },
            ty,
            span,
        });
    }
    let kind = match expression.kind {
        TypedExprKind::Local(id) => ExprKind::Local(id),
        TypedExprKind::Global(id) => {
            // `Prim.undefined` type checks as `forall a. a` but has no runtime
            // representation, and none of the primitive lowering shapes
            // describes a partial value. Reporting it here keeps Core honest
            // rather than emitting a reference to a global that does not
            // exist.
            if matches!(
                externals.get(&id),
                Some(ExternalKind::Intrinsic(psrs_hir::Intrinsic::Undefined))
            ) {
                return Err(LowerError {
                    span,
                    message: "`Prim.undefined` has no runtime representation",
                });
            }
            // `unit` is the one `Unit` value. `Unit` is a builtin type here, so
            // there is no constructor application for it and the primitive is
            // the value itself; Core verification checks the use still carries
            // the `Unit` type the checker gave it.
            if matches!(
                externals.get(&id),
                Some(ExternalKind::Intrinsic(psrs_hir::Intrinsic::Unit))
            ) {
                return Ok(Expr {
                    kind: ExprKind::Unit,
                    ty,
                    span,
                });
            }
            if let Some(constructor) = constructors.get(&id) {
                if constructor.field_count != 0 {
                    return lower_partial_constructor(
                        id,
                        expression.ty,
                        constructor.field_count,
                        span,
                        source_types,
                        context,
                    );
                }
                ExprKind::Constructor {
                    symbol: id,
                    arguments: Vec::new(),
                }
            } else {
                ExprKind::Global(id)
            }
        }
        TypedExprKind::Integer(value) => ExprKind::Integer(value),
        TypedExprKind::Number(value) => ExprKind::Number(value),
        TypedExprKind::Boolean(value) => ExprKind::Boolean(value),
        TypedExprKind::String(value) => ExprKind::String(value),
        TypedExprKind::Char(value) => ExprKind::Char(value),
        TypedExprKind::Array(elements) => ExprKind::Array {
            elements: elements
                .into_iter()
                .map(|element| lower_expr(element, externals, constructors, source_types, context))
                .collect::<Result<Vec<_>, _>>()?,
        },
        TypedExprKind::Record(fields) => ExprKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| {
                    Ok((
                        label,
                        lower_expr(value, externals, constructors, source_types, context)?,
                    ))
                })
                .collect::<Result<Vec<_>, LowerError>>()?,
        },
        TypedExprKind::RecordUpdate { expression, fields } => ExprKind::RecordUpdate {
            record: Box::new(lower_expr(
                *expression,
                externals,
                constructors,
                source_types,
                context,
            )?),
            fields: fields
                .into_iter()
                .map(|(label, value)| {
                    Ok((
                        label,
                        lower_expr(value, externals, constructors, source_types, context)?,
                    ))
                })
                .collect::<Result<Vec<_>, LowerError>>()?,
        },
        TypedExprKind::FieldAccess { expression, field } => ExprKind::FieldAccess {
            record: Box::new(lower_expr(
                *expression,
                externals,
                constructors,
                source_types,
                context,
            )?),
            field,
        },
        TypedExprKind::Evidence(evidence) => {
            return dictionary::lower_evidence(&evidence, source_types);
        }
        TypedExprKind::Coerce {
            value,
            evidence,
            source_type,
            target_type,
        } => {
            if evidence.class_id != psrs_hir::TypeId::COERCIBLE
                || source_type != value.ty
                || target_type.0 != ty.0
            {
                return Err(LowerError {
                    span,
                    message: "coercion evidence does not match its typed boundary",
                });
            }
            ExprKind::RepresentationCast {
                value: Box::new(lower_expr(
                    *value,
                    externals,
                    constructors,
                    source_types,
                    context,
                )?),
                source_type: TypeId(source_type.0),
                target_type: TypeId(target_type.0),
            }
        }
        TypedExprKind::Application(function, argument) => {
            let function = lower_expr(*function, externals, constructors, source_types, context)?;
            let argument = lower_expr(*argument, externals, constructors, source_types, context)?;
            // A saturated intrinsic application becomes one IntrinsicCall. The
            // registry's arity decides saturation, and the per-intrinsic
            // handling lives in the intrinsic module rather than here.
            if let Some((symbol, args)) = flatten_intrinsic(&function, argument.clone(), externals)
                && let Some(ExternalKind::Intrinsic(intrinsic)) = externals.get(&symbol)
                && args.len() == intrinsic.descriptor().arity as usize
            {
                return Ok(Expr {
                    kind: ExprKind::IntrinsicCall {
                        intrinsic: *intrinsic,
                        arguments: args,
                    },
                    ty,
                    span,
                });
            }
            ExprKind::Application(Box::new(function), Box::new(argument))
        }
        TypedExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder: Binder {
                id: binder.id,
                name: binder.name,
                ty: TypeId(binder.ty.0),
                span: binder.span,
            },
            body: Box::new(lower_expr(
                *body,
                externals,
                constructors,
                source_types,
                context,
            )?),
        },
        TypedExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|binding| {
                    Ok(Binding {
                        binder: Binder {
                            id: binding.binder.id,
                            name: binding.binder.name,
                            ty: TypeId(binding.binder.ty.0),
                            span: binding.binder.span,
                        },
                        quantified: binding.quantified,
                        value: lower_expr(
                            binding.value,
                            externals,
                            constructors,
                            source_types,
                            context,
                        )?,
                        span: binding.span,
                    })
                })
                .collect::<Result<Vec<_>, LowerError>>()?,
            body: Box::new(lower_expr(
                *body,
                externals,
                constructors,
                source_types,
                context,
            )?),
        },
        TypedExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(lower_expr(
                *condition,
                externals,
                constructors,
                source_types,
                context,
            )?),
            then_branch: Box::new(lower_expr(
                *then_branch,
                externals,
                constructors,
                source_types,
                context,
            )?),
            else_branch: Box::new(lower_expr(
                *else_branch,
                externals,
                constructors,
                source_types,
                context,
            )?),
        },
        TypedExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(lower_expr(
                *scrutinee,
                externals,
                constructors,
                source_types,
                context,
            )?),
            branches: branches
                .into_iter()
                .map(|branch| {
                    Ok(crate::CaseBranch {
                        pattern: lower_pattern(branch.pattern)?,
                        value: lower_expr(
                            branch.value,
                            externals,
                            constructors,
                            source_types,
                            context,
                        )?,
                        span: branch.span,
                        coverage: branch.coverage,
                    })
                })
                .collect::<Result<Vec<_>, LowerError>>()?,
        },
    };
    Ok(Expr { kind, ty, span })
}

fn lower_partial_constructor(
    symbol: SymbolId,
    function_type: psrs_thir::TypeId,
    field_count: usize,
    span: psrs_span::TextRange,
    source_types: &[psrs_thir::Type],
    context: &mut module::LowerContext,
) -> Result<Expr, LowerError> {
    let mut current_type = function_type;
    let mut binders = Vec::with_capacity(field_count);
    for index in 0..field_count {
        let Some((parameter, result)) = arrow_parts_after_foralls(source_types, current_type)
        else {
            return Err(LowerError {
                span,
                message: "constructor type has fewer arguments than its declaration",
            });
        };
        let Some(id) = context.fresh_local() else {
            return Err(LowerError {
                span,
                message: "cannot allocate a local for a constructor function",
            });
        };
        binders.push((
            Binder {
                id,
                name: format!("__partial_constructor_{index}"),
                ty: TypeId(parameter.0),
                span,
            },
            TypeId(current_type.0),
        ));
        current_type = result;
    }

    let arguments = binders
        .iter()
        .map(|(binder, _)| Expr {
            kind: ExprKind::Local(binder.id),
            ty: binder.ty,
            span,
        })
        .collect();
    let mut body = Expr {
        kind: ExprKind::Constructor { symbol, arguments },
        ty: TypeId(current_type.0),
        span,
    };
    for (binder, function_type) in binders.into_iter().rev() {
        body = Expr {
            kind: ExprKind::Lambda {
                binder,
                body: Box::new(body),
            },
            ty: function_type,
            span,
        };
    }
    Ok(body)
}

fn arrow_parts_after_foralls(
    types: &[psrs_thir::Type],
    mut function_type: psrs_thir::TypeId,
) -> Option<(psrs_thir::TypeId, psrs_thir::TypeId)> {
    while let Some((_, body)) = psrs_thir::forall_parts(types, function_type) {
        function_type = body;
    }
    psrs_thir::arrow_parts(types, function_type)
}

fn lower_pattern(pattern: psrs_thir::Pattern) -> Result<crate::Pattern, LowerError> {
    let span = pattern.span;
    let kind = match pattern.kind {
        psrs_thir::PatternKind::Wildcard => crate::PatternKind::Wildcard,
        psrs_thir::PatternKind::Literal { literal } => crate::PatternKind::Literal {
            value: match literal {
                psrs_thir::PatternLiteral::Integer(value) => crate::Literal::Integer(value),
                psrs_thir::PatternLiteral::Number(value) => crate::Literal::Number(value),
                psrs_thir::PatternLiteral::String(value) => crate::Literal::String(value),
                psrs_thir::PatternLiteral::Char(value) => crate::Literal::Char(value),
                psrs_thir::PatternLiteral::Boolean(value) => crate::Literal::Boolean(value),
            },
        },
        psrs_thir::PatternKind::Array { elements } => crate::PatternKind::Array {
            elements: elements
                .into_iter()
                .map(lower_pattern)
                .collect::<Result<Vec<_>, _>>()?,
        },
        psrs_thir::PatternKind::Named { id, pattern } => crate::PatternKind::Named {
            id,
            pattern: Box::new(lower_pattern(*pattern)?),
        },
        psrs_thir::PatternKind::Var { id, ty } => crate::PatternKind::Var {
            id,
            ty: TypeId(ty.0),
        },
        psrs_thir::PatternKind::Constructor { symbol, arguments } => {
            crate::PatternKind::Constructor {
                symbol,
                arguments: arguments
                    .into_iter()
                    .map(lower_pattern)
                    .collect::<Result<Vec<_>, _>>()?,
            }
        }
        psrs_thir::PatternKind::Record { fields } => crate::PatternKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, pattern)| Ok((label, lower_pattern(pattern)?)))
                .collect::<Result<Vec<_>, LowerError>>()?,
        },
    };
    Ok(crate::Pattern {
        kind,
        ty: TypeId(pattern.ty.0),
        span,
    })
}

fn constructor_application<'a>(
    expression: &'a TypedExpr,
    constructors: &HashMap<SymbolId, psrs_thir::ConstructorInfo>,
) -> Option<(SymbolId, Vec<&'a TypedExpr>)> {
    let mut arguments = Vec::new();
    let mut head = expression;
    while let TypedExprKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function;
    }
    let TypedExprKind::Global(symbol) = head.kind else {
        return None;
    };
    let constructor = constructors.get(&symbol)?;
    (constructor.field_count == arguments.len()).then(|| {
        arguments.reverse();
        (symbol, arguments)
    })
}

fn flatten_intrinsic(
    function: &Expr,
    final_argument: Expr,
    externals: &HashMap<SymbolId, ExternalKind>,
) -> Option<(SymbolId, Vec<Expr>)> {
    let mut arguments = vec![final_argument];
    let mut head = function;
    while let ExprKind::Application(next, argument) = &head.kind {
        arguments.push((**argument).clone());
        head = next;
    }
    let ExprKind::Global(symbol) = head.kind else {
        return None;
    };
    if !matches!(externals.get(&symbol), Some(ExternalKind::Intrinsic(_))) {
        return None;
    }
    arguments.reverse();
    Some((symbol, arguments))
}
