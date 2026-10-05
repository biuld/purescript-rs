//! Ordinary declarations for the abstract `psrs:effect` operations.
//!
//! `lower_effects` replaces the library's opaque imports with the values the
//! design specifies: `pure` returns a closure over the token, `bind` runs the
//! first effect before the continuation, `run` applies the closure to the
//! runtime state token, and `trap` is the effect that escapes instead of
//! returning.

use super::supplies::{LocalSupply, VariableSupply};
use super::{EFFECT_INTERFACE, TrustedEffect, intern};
use crate::{Binder, Declaration, Expr, ExprKind, Module, Type, TypeConstructor, TypeId};
use psrs_hir::{ExternalKind, LocalId, SymbolId, TypeVariableId};
use psrs_span::TextRange;

/// Synthesizes `pure`, `bind`, `run`, and `trap`, dropping the abstract imports
/// they replace. Returns the symbols that became ordinary declarations.
pub(super) fn synthesize_operations(
    module: &mut Module,
    token: TypeId,
    trusted: &TrustedEffect,
) -> Result<Vec<SymbolId>, Vec<crate::VerifyError>> {
    let mut operations = Vec::new();
    let mut seen_symbols = std::collections::HashSet::new();
    let mut seen_operations = std::collections::HashSet::new();
    for binding in &trusted.operations {
        if !seen_symbols.insert(binding.symbol) || !seen_operations.insert(binding.operation) {
            return Err(vec![verification_error(
                module,
                "trusted Effect operation bindings are duplicated",
            )]);
        }
        let Some((index, external)) = module
            .externals
            .iter()
            .enumerate()
            .find(|(_, external)| external.symbol == binding.symbol)
        else {
            return Err(vec![verification_error(
                module,
                "trusted Effect operation binding has no external declaration",
            )]);
        };
        let ExternalKind::Wit {
            interface,
            function,
        } = &external.kind
        else {
            return Err(vec![verification_error(
                module,
                "trusted Effect operation is not a WIT import",
            )]);
        };
        if interface != EFFECT_INTERFACE || function != binding.operation.wit_function() {
            return Err(vec![verification_error(
                module,
                "trusted Effect operation identity does not match its WIT binding",
            )]);
        }
        operations.push((
            index,
            binding.symbol,
            external.name.clone(),
            function.clone(),
            external_span(external),
        ));
    }
    if module.externals.iter().any(|external| {
        matches!(
            &external.kind,
            ExternalKind::Wit { interface, .. } if interface == EFFECT_INTERFACE
        ) && !seen_symbols.contains(&external.symbol)
    }) {
        return Err(vec![verification_error(
            module,
            "Effect WIT import is missing from the trusted operation bindings",
        )]);
    }
    let mut locals = LocalSupply::new(module);
    let mut variables = VariableSupply::new(module);
    let mut synthesized = Vec::new();
    let mut remove = Vec::new();
    for (index, symbol, name, function, span) in operations {
        let declaration = match function.as_str() {
            "pure" => Some(pure_declaration(
                module,
                symbol,
                &name,
                span,
                token,
                &mut locals,
                &mut variables,
            )),
            "bind" => Some(bind_declaration(
                module,
                symbol,
                &name,
                span,
                token,
                &mut locals,
                &mut variables,
            )),
            "run" => Some(run_declaration(
                module,
                symbol,
                &name,
                span,
                token,
                &mut locals,
                &mut variables,
            )),
            "trap" => Some(trap_declaration(
                module,
                symbol,
                &name,
                span,
                token,
                &mut locals,
            )),
            _ => None,
        };
        let Some(declaration) = declaration else {
            return Err(vec![verification_error(
                module,
                "trusted Effect operation has no lowering rule",
            )]);
        };
        module.declarations.push(declaration);
        synthesized.push(symbol);
        remove.push(index);
    }
    remove.sort_unstable();
    for index in remove.into_iter().rev() {
        module.externals.remove(index);
    }
    module
        .external_types
        .retain(|external| !seen_symbols.contains(&external.symbol));
    Ok(synthesized)
}

fn verification_error(module: &Module, message: &'static str) -> crate::VerifyError {
    crate::VerifyError {
        module: module.id,
        span: module.span,
        message,
    }
}

fn pure_declaration(
    module: &mut Module,
    symbol: SymbolId,
    name: &str,
    span: TextRange,
    token: TypeId,
    locals: &mut LocalSupply,
    variables: &mut VariableSupply,
) -> Declaration {
    let parameter = variables.fresh();
    let parameter_ty = intern(module, Type::Variable(parameter));
    let result = closure_type(module, token, parameter_ty);
    let ty = arrow(module, parameter_ty, result);
    let value_local = locals.fresh();
    let token_local = locals.fresh();
    let body = expr(ExprKind::Local(value_local), parameter_ty, span);
    let closure = lambda(token_local, "token", token, body, result, span);
    let value = lambda(value_local, "value", parameter_ty, closure, ty, span);
    declaration(symbol, name, vec![parameter], ty, value, span)
}

fn bind_declaration(
    module: &mut Module,
    symbol: SymbolId,
    name: &str,
    span: TextRange,
    token: TypeId,
    locals: &mut LocalSupply,
    variables: &mut VariableSupply,
) -> Declaration {
    let first_var = variables.fresh();
    let next_var = variables.fresh();
    let first_ty = intern(module, Type::Variable(first_var));
    let next_ty = intern(module, Type::Variable(next_var));
    let first_effect = closure_type(module, token, first_ty);
    let next_effect = closure_type(module, token, next_ty);
    let continuation = arrow(module, first_ty, next_effect);
    let suspended = arrow(module, continuation, next_effect);
    let ty = arrow(module, first_effect, suspended);
    let first_local = locals.fresh();
    let next_local = locals.fresh();
    let token_local = locals.fresh();
    let ran_first = application(
        expr(ExprKind::Local(first_local), first_effect, span),
        expr(ExprKind::Local(token_local), token, span),
        first_ty,
        span,
    );
    let ran_next = application(
        expr(ExprKind::Local(next_local), continuation, span),
        ran_first,
        next_effect,
        span,
    );
    let body = application(
        ran_next,
        expr(ExprKind::Local(token_local), token, span),
        next_ty,
        span,
    );
    let closure = lambda(token_local, "token", token, body, next_effect, span);
    let with_next = lambda(next_local, "next", continuation, closure, suspended, span);
    let value = lambda(first_local, "first", first_effect, with_next, ty, span);
    declaration(symbol, name, vec![first_var, next_var], ty, value, span)
}

fn run_declaration(
    module: &mut Module,
    symbol: SymbolId,
    name: &str,
    span: TextRange,
    token: TypeId,
    locals: &mut LocalSupply,
    variables: &mut VariableSupply,
) -> Declaration {
    let result_var = variables.fresh();
    let result_ty = intern(module, Type::Variable(result_var));
    let action_ty = closure_type(module, token, result_ty);
    let ty = arrow(module, action_ty, result_ty);
    let action = locals.fresh();
    let token_value = expr(ExprKind::StateToken, token, span);
    let body = application(
        expr(ExprKind::Local(action), action_ty, span),
        token_value,
        result_ty,
        span,
    );
    let value = lambda(action, "action", action_ty, body, ty, span);
    declaration(symbol, name, vec![result_var], ty, value, span)
}

/// The `trap :: Effect Unit` operation: an effect over the runtime token whose
/// body never returns, so running it escapes instead of producing `Unit`.
///
/// It is a closure rather than a bare trap so that referencing the value stays
/// inert, exactly as `pure` is: an `Effect` only does something when the entry
/// applies it to a token. `Effect` is abstract, so this is the only place that
/// can state that an uncaught failure is a target trap.
fn trap_declaration(
    module: &mut Module,
    symbol: SymbolId,
    name: &str,
    span: TextRange,
    token: TypeId,
    locals: &mut LocalSupply,
) -> Declaration {
    let unit = intern(module, Type::Constructor(TypeConstructor::Unit));
    let action = closure_type(module, token, unit);
    let token_local = locals.fresh();
    let body = expr(ExprKind::Trap, unit, span);
    let value = lambda(token_local, "token", token, body, action, span);
    declaration(symbol, name, Vec::new(), action, value, span)
}

fn declaration(
    symbol: SymbolId,
    name: &str,
    quantified: Vec<TypeVariableId>,
    ty: TypeId,
    value: Expr,
    span: TextRange,
) -> Declaration {
    Declaration {
        symbol,
        name: name.to_string(),
        name_span: span,
        quantified,
        ty,
        value,
        span,
    }
}

fn lambda(
    id: LocalId,
    name: &str,
    ty: TypeId,
    body: Expr,
    function_ty: TypeId,
    span: TextRange,
) -> Expr {
    expr(
        ExprKind::Lambda {
            binder: Binder {
                id,
                name: name.to_string(),
                ty,
                span,
            },
            body: Box::new(body),
        },
        function_ty,
        span,
    )
}

fn application(function: Expr, argument: Expr, ty: TypeId, span: TextRange) -> Expr {
    expr(
        ExprKind::Application(Box::new(function), Box::new(argument)),
        ty,
        span,
    )
}

fn expr(kind: ExprKind, ty: TypeId, span: TextRange) -> Expr {
    Expr { kind, ty, span }
}

fn external_span(external: &psrs_hir::ExternalSymbol) -> TextRange {
    external
        .signature
        .as_ref()
        .map(|signature| signature.span)
        .unwrap_or_default()
}

/// Builds the token closure type `[token] -> result` in the module type table.
pub(super) fn closure_type(module: &mut Module, token: TypeId, result: TypeId) -> TypeId {
    intern(
        module,
        Type::Closure {
            parameters: vec![token],
            result,
        },
    )
}

/// Builds `parameter -> result` in the module type table.
pub(super) fn arrow(module: &mut Module, parameter: TypeId, result: TypeId) -> TypeId {
    let function = intern(module, Type::Constructor(TypeConstructor::Function));
    let partial = intern(module, Type::Application(function, parameter));
    intern(module, Type::Application(partial, result))
}
