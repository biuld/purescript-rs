use crate::{
    Binder, Binding, Declaration, Expr, ExprKind, LowerError, Module, Primitive, Type, TypeId,
};
use psrs_hir::{ExternalKind, SymbolId};
use psrs_thir::{Expr as TypedExpr, ExprKind as TypedExprKind};
use std::collections::HashMap;

fn lower_module_inner(module: psrs_thir::Module) -> Result<Module, Vec<LowerError>> {
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
        .map(|external| (external.symbol, external.kind.clone()))
        .collect::<HashMap<_, _>>();
    let constructors = module
        .constructors
        .iter()
        .map(|constructor| (constructor.symbol, constructor.clone()))
        .collect::<HashMap<_, _>>();
    let types = module
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
        .collect();
    let mut declarations = Vec::with_capacity(module.declarations.len());
    for declaration in module.declarations {
        let value = lower_expr(declaration.value, &externals, &constructors)
            .map_err(|error| vec![error])?;
        declarations.push(Declaration {
            symbol: declaration.symbol,
            name: declaration.name,
            name_span: declaration.name_span,
            quantified: declaration.quantified,
            ty: TypeId(declaration.ty.0),
            value,
            span: declaration.span,
        });
    }
    let lowered = Module {
        id: module.id,
        name: module.name,
        externals: module.externals,
        types,
        constructors: module
            .constructors
            .iter()
            .map(|constructor| crate::ConstructorInfo {
                symbol: constructor.symbol,
                type_id: constructor.type_id,
                tag: constructor.tag,
                field_count: constructor.field_count,
                field_types: constructor
                    .field_types
                    .iter()
                    .map(|field| TypeId(field.0))
                    .collect(),
            })
            .collect(),
        declarations,
        entry: None,
        span: module.span,
    };
    Ok(lowered)
}

/// Lowers a module and verifies the result. A module with unresolved
/// cross-module global references cannot be verified on its own; use
/// [`lower_module_unverified`] and verify the linked module instead.
pub(super) fn lower_module(module: psrs_thir::Module) -> Result<Module, Vec<LowerError>> {
    let lowered = lower_module_inner(module)?;
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
    lower_module_inner(module)
}

fn lower_expr(
    expression: TypedExpr,
    externals: &HashMap<SymbolId, ExternalKind>,
    constructors: &HashMap<SymbolId, psrs_thir::ConstructorInfo>,
) -> Result<Expr, LowerError> {
    let span = expression.span;
    let ty = TypeId(expression.ty.0);
    if let Some((symbol, arguments)) = constructor_application(&expression, constructors) {
        let arguments = arguments
            .into_iter()
            .map(|argument| lower_expr(argument.clone(), externals, constructors))
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
            if let Some(constructor) = constructors.get(&id) {
                if constructor.field_count != 0 {
                    return Err(LowerError {
                        span,
                        message: "partially applied field constructors require closure conversion",
                    });
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
        TypedExprKind::Boolean(value) => ExprKind::Boolean(value),
        TypedExprKind::String(value) => ExprKind::String(value),
        TypedExprKind::Application(function, argument) => {
            let function = lower_expr(*function, externals, constructors)?;
            let argument = lower_expr(*argument, externals, constructors)?;
            if let Some((symbol, args)) = flatten_intrinsic(&function, argument.clone(), externals)
                && args.len() == 2
                && let Some(op) = externals.get(&symbol).cloned().and_then(|kind| match kind {
                    ExternalKind::Intrinsic(intrinsic) => Primitive::from_intrinsic(intrinsic),
                    ExternalKind::Wit { .. } => None,
                })
            {
                return Ok(Expr {
                    kind: ExprKind::Primitive {
                        op,
                        left: Box::new(args[0].clone()),
                        right: Box::new(args[1].clone()),
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
            body: Box::new(lower_expr(*body, externals, constructors)?),
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
                        value: lower_expr(binding.value, externals, constructors)?,
                        span: binding.span,
                    })
                })
                .collect::<Result<Vec<_>, LowerError>>()?,
            body: Box::new(lower_expr(*body, externals, constructors)?),
        },
        TypedExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(lower_expr(*condition, externals, constructors)?),
            then_branch: Box::new(lower_expr(*then_branch, externals, constructors)?),
            else_branch: Box::new(lower_expr(*else_branch, externals, constructors)?),
        },
        TypedExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(lower_expr(*scrutinee, externals, constructors)?),
            branches: branches
                .into_iter()
                .map(|branch| {
                    Ok(crate::CaseBranch {
                        pattern: lower_pattern(branch.pattern)?,
                        value: lower_expr(branch.value, externals, constructors)?,
                        span: branch.span,
                    })
                })
                .collect::<Result<Vec<_>, LowerError>>()?,
        },
    };
    Ok(Expr { kind, ty, span })
}

fn lower_pattern(pattern: psrs_thir::Pattern) -> Result<crate::Pattern, LowerError> {
    let span = pattern.span;
    let kind = match pattern.kind {
        psrs_thir::PatternKind::Wildcard => crate::PatternKind::Wildcard,
        psrs_thir::PatternKind::Var { id, .. } => crate::PatternKind::Var(id),
        psrs_thir::PatternKind::Constructor { symbol, arguments } => {
            crate::PatternKind::Constructor {
                symbol,
                arguments: arguments
                    .into_iter()
                    .map(lower_pattern)
                    .collect::<Result<Vec<_>, _>>()?,
            }
        }
    };
    Ok(crate::Pattern { kind, span })
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
