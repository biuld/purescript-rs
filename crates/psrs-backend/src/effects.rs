//! Suspends effectful foreign imports after [`psrs_core::effect::lower_effects`].
//!
//! A host call whose source type ends in an effect runs inside the closure
//! that lowering produced. `pure`, `bind`, and `runEffect` are already
//! ordinary declarations by the time this split runs.

use crate::BackendError;
use crate::bindings::ExternalBindings;
use psrs_core::{Binder, Declaration, Expr, ExprKind, Module as CoreModule, TypeId};
use psrs_hir::{LocalId, SymbolId};
use psrs_span::TextRange;

pub(crate) fn lower_effects(
    module: &mut CoreModule,
    bindings: &mut ExternalBindings,
) -> Result<(), Vec<BackendError>> {
    let lowering = psrs_core::effect::lower_effects(module);
    let synthesized = lowering.synthesized.to_vec();
    bindings
        .imports
        .retain(|binding| !synthesized.contains(&binding.symbol));
    suspend_imports(module, bindings)
}

struct Suspension {
    index: usize,
    original: SymbolId,
    name: String,
    span: TextRange,
    parameters: Vec<TypeId>,
    closure: TypeId,
    payload: TypeId,
    source: TypeId,
}

fn suspend_imports(
    module: &mut CoreModule,
    bindings: &mut ExternalBindings,
) -> Result<(), Vec<BackendError>> {
    let mut plans = Vec::new();
    for (index, binding) in bindings.imports.iter().enumerate() {
        let Some(source) = binding.type_id else {
            continue;
        };
        let Some((parameters, closure, payload)) =
            psrs_core::effect::suspended_import(module, source)
        else {
            continue;
        };
        let Some(external) = module
            .externals
            .iter()
            .find(|external| external.symbol == binding.symbol)
        else {
            return Err(vec![effect_error(
                module,
                binding.span,
                "effectful import has no external symbol",
            )]);
        };
        plans.push(Suspension {
            index,
            original: binding.symbol,
            name: external.name.clone(),
            span: binding.span,
            parameters,
            closure,
            payload,
            source,
        });
    }
    for plan in plans {
        let host = psrs_core::effect::fresh_foreign_symbol(module);
        let host_type = psrs_core::effect::function_type(module, &plan.parameters, plan.payload);
        let Some(external) = module
            .externals
            .iter_mut()
            .find(|external| external.symbol == plan.original)
        else {
            return Err(vec![effect_error(
                module,
                plan.span,
                "effectful import has no external symbol",
            )]);
        };
        external.symbol = host;
        bindings.imports[plan.index].symbol = host;
        bindings.imports[plan.index].type_id = Some(host_type);
        module
            .declarations
            .push(wrapper(module, &plan, host, host_type)?);
    }
    Ok(())
}

fn wrapper(
    module: &CoreModule,
    plan: &Suspension,
    host: SymbolId,
    host_type: TypeId,
) -> Result<Declaration, Vec<BackendError>> {
    let token_ty = psrs_core::closure_parts(&module.types, plan.closure)
        .and_then(|(parameters, _)| parameters.first().copied())
        .ok_or_else(|| {
            vec![effect_error(
                module,
                plan.span,
                "effect closure is missing its token parameter",
            )]
        })?;
    let mut next_local = psrs_core::effect::fresh_local(module).0;
    let mut fresh = || {
        let id = LocalId(next_local);
        next_local += 1;
        id
    };
    let binders = plan
        .parameters
        .iter()
        .copied()
        .map(|ty| (fresh(), ty))
        .collect::<Vec<_>>();
    let mut call = expr(ExprKind::Global(host), host_type, plan.span);
    let mut remaining = host_type;
    for (local, ty) in &binders {
        let result = psrs_core::arrow_parts(&module.types, remaining)
            .map(|(_, result)| result)
            .unwrap_or(plan.payload);
        call = expr(
            ExprKind::Application(
                Box::new(call),
                Box::new(expr(ExprKind::Local(*local), *ty, plan.span)),
            ),
            result,
            plan.span,
        );
        remaining = result;
    }
    let mut arrow_types = Vec::new();
    let mut current = plan.source;
    for _ in &plan.parameters {
        arrow_types.push(current);
        let Some((_, result)) = psrs_core::arrow_parts(&module.types, current) else {
            return Err(vec![effect_error(
                module,
                plan.span,
                "effectful import type is not a function of its payload",
            )]);
        };
        current = result;
    }
    let token = fresh();
    let mut value = lambda(token, "token", token_ty, call, plan.closure, plan.span);
    for (index, (local, ty)) in binders.iter().enumerate().rev() {
        value = lambda(*local, "value", *ty, value, arrow_types[index], plan.span);
    }
    Ok(Declaration {
        symbol: plan.original,
        name: plan.name.clone(),
        name_span: plan.span,
        quantified: Vec::new(),
        ty: plan.source,
        value,
        span: plan.span,
    })
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

fn expr(kind: ExprKind, ty: TypeId, span: TextRange) -> Expr {
    Expr { kind, ty, span }
}

fn effect_error(module: &CoreModule, span: TextRange, message: &str) -> BackendError {
    let error = BackendError::invalid_ir("P8 effect lowering", span, message.to_string());
    match module.entry {
        Some(entry) => error.with_module(entry.module),
        None => error,
    }
}
