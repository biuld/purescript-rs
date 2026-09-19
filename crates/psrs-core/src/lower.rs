use crate::{
    Binder, Binding, Declaration, Expr, ExprKind, LowerError, Module, Primitive, Type, TypeId,
};
use psrs_hir::{ExternalKind, SymbolId};
use psrs_thir::{Expr as TypedExpr, ExprKind as TypedExprKind};
use std::collections::HashMap;

pub(super) fn lower_module(module: psrs_thir::Module) -> Result<Module, Vec<LowerError>> {
    if let Err(errors) = module.verify() {
        return Err(errors
            .into_iter()
            .map(|error| LowerError {
                span: error.span,
                message: "invalid THIR input",
            })
            .collect());
    }
    let externals = module
        .externals
        .iter()
        .map(|external| (external.symbol, external.kind))
        .collect::<HashMap<_, _>>();
    let lowered = Module {
        id: module.id,
        name: module.name,
        externals: module.externals,
        types: module
            .types
            .into_iter()
            .map(|ty| match ty {
                psrs_thir::Type::Variable(variable) => Type::Variable(variable),
                psrs_thir::Type::I32 => Type::I32,
                psrs_thir::Type::Boolean => Type::Boolean,
                psrs_thir::Type::String => Type::String,
                psrs_thir::Type::Unit => Type::Unit,
                psrs_thir::Type::Constructor(constructor) => Type::Constructor(match constructor {
                    psrs_thir::TypeConstructor::Array => crate::TypeConstructor::Array,
                    psrs_thir::TypeConstructor::User(id) => crate::TypeConstructor::User(id),
                }),
                psrs_thir::Type::Application(function, argument) => {
                    Type::Application(TypeId(function.0), TypeId(argument.0))
                }
                psrs_thir::Type::Function { parameter, result } => Type::Function {
                    parameter: TypeId(parameter.0),
                    result: TypeId(result.0),
                },
            })
            .collect(),
        declarations: module
            .declarations
            .into_iter()
            .map(|declaration| Declaration {
                symbol: declaration.symbol,
                name: declaration.name,
                name_span: declaration.name_span,
                quantified: declaration.quantified,
                ty: TypeId(declaration.ty.0),
                value: lower_expr(declaration.value, &externals),
                span: declaration.span,
            })
            .collect(),
        span: module.span,
    };
    if lowered.verify().is_err() {
        return Err(vec![LowerError {
            span: lowered.span,
            message: "lowered Core failed verification",
        }]);
    }
    Ok(lowered)
}

fn lower_expr(expression: TypedExpr, externals: &HashMap<SymbolId, ExternalKind>) -> Expr {
    let span = expression.span;
    let ty = TypeId(expression.ty.0);
    let kind = match expression.kind {
        TypedExprKind::Local(id) => ExprKind::Local(id),
        TypedExprKind::Global(id) => ExprKind::Global(id),
        TypedExprKind::Integer(value) => ExprKind::Integer(value),
        TypedExprKind::Boolean(value) => ExprKind::Boolean(value),
        TypedExprKind::String(value) => ExprKind::String(value),
        TypedExprKind::Application(function, argument) => {
            let function = lower_expr(*function, externals);
            let argument = lower_expr(*argument, externals);
            if let Some((symbol, args)) = flatten_intrinsic(&function, argument.clone(), externals)
                && args.len() == 2
                && let Some(op) = externals.get(&symbol).copied().and_then(|kind| match kind {
                    ExternalKind::Intrinsic(intrinsic) => Primitive::from_intrinsic(intrinsic),
                    ExternalKind::Runtime(_) => None,
                })
            {
                return Expr {
                    kind: ExprKind::Primitive {
                        op,
                        left: Box::new(args[0].clone()),
                        right: Box::new(args[1].clone()),
                    },
                    ty,
                    span,
                };
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
            body: Box::new(lower_expr(*body, externals)),
        },
        TypedExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|binding| Binding {
                    binder: Binder {
                        id: binding.binder.id,
                        name: binding.binder.name,
                        ty: TypeId(binding.binder.ty.0),
                        span: binding.binder.span,
                    },
                    quantified: binding.quantified,
                    value: lower_expr(binding.value, externals),
                    span: binding.span,
                })
                .collect(),
            body: Box::new(lower_expr(*body, externals)),
        },
        TypedExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(lower_expr(*condition, externals)),
            then_branch: Box::new(lower_expr(*then_branch, externals)),
            else_branch: Box::new(lower_expr(*else_branch, externals)),
        },
    };
    Expr { kind, ty, span }
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

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_hir::Intrinsic;

    #[test]
    fn primitive_mapping_is_limited_to_integer_operations() {
        assert_eq!(
            Primitive::from_intrinsic(Intrinsic::I32Add),
            Some(Primitive::Add)
        );
        assert_eq!(Primitive::from_intrinsic(Intrinsic::BoolTrue), None);
    }
}
