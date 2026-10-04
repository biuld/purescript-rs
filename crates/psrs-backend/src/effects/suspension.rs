use super::verify::{effect_error, source_effect_error};
use crate::BackendError;
use crate::bindings::ExternalBindings;
use psrs_core::{
    Binder, Declaration, Expr, ExprKind, Module as CoreModule, Type, TypeId, closure_parts,
};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeVariableId};
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SuspensionPlan {
    index: usize,
    original: SymbolId,
    source_module: ModuleId,
    name: String,
    span: TextRange,
    source: TypeId,
    source_body: TypeId,
    quantified: Vec<TypeVariableId>,
    parameters: Vec<TypeId>,
    application: TypeId,
    payload: TypeId,
}

#[derive(Clone, Debug)]
pub(super) struct AppliedSuspension {
    plan: SuspensionPlan,
    host: SymbolId,
    host_type: TypeId,
}

fn import_error(module: &CoreModule, plan: &SuspensionPlan, message: &str) -> BackendError {
    source_effect_error(module, plan.source_module, plan.span, message)
}

pub(super) fn plan(
    module: &CoreModule,
    bindings: &ExternalBindings,
    trusted: &psrs_core::effect::TrustedEffect,
) -> Result<Vec<SuspensionPlan>, Vec<BackendError>> {
    let mut plans = Vec::new();
    let operation_symbols = trusted
        .operations
        .iter()
        .map(|operation| operation.symbol)
        .collect::<std::collections::HashSet<_>>();
    for (index, binding) in bindings.imports.iter().enumerate() {
        if operation_symbols.contains(&binding.symbol) {
            continue;
        }
        let Some(source) = binding.type_id else {
            continue;
        };
        let shape = psrs_core::effect::classify_effect_import(module, source, trusted.effect_type)
            .map_err(|message| {
                vec![source_effect_error(
                    module,
                    binding.source_module,
                    binding.span,
                    message,
                )]
            })?;
        let Some(shape) = shape else {
            continue;
        };
        let Some(external) = module
            .externals
            .iter()
            .find(|external| external.symbol == binding.symbol)
        else {
            return Err(vec![source_effect_error(
                module,
                binding.source_module,
                binding.span,
                "effectful import has no external symbol",
            )]);
        };
        let source_body = strip_foralls(module, source).ok_or_else(|| {
            vec![source_effect_error(
                module,
                binding.source_module,
                binding.span,
                "effectful import has an invalid quantified type",
            )]
        })?;
        plans.push(SuspensionPlan {
            index,
            original: binding.symbol,
            source_module: binding.source_module,
            name: external.name.clone(),
            span: binding.span,
            source,
            source_body,
            quantified: shape.quantified,
            parameters: shape.parameters,
            application: shape.application,
            payload: shape.payload,
        });
    }
    Ok(plans)
}

pub(super) fn apply(
    module: &mut CoreModule,
    bindings: &mut ExternalBindings,
    plans: &[SuspensionPlan],
) -> Result<Vec<AppliedSuspension>, Vec<BackendError>> {
    let mut applied = Vec::with_capacity(plans.len());
    for plan in plans {
        let Some(binding) = bindings.imports.get(plan.index) else {
            return Err(vec![import_error(
                module,
                plan,
                "effect import plan no longer names an external binding",
            )]);
        };
        if binding.symbol != plan.original
            || binding.source_module != plan.source_module
            || binding.type_id != Some(plan.source)
        {
            return Err(vec![import_error(
                module,
                plan,
                "effect import plan no longer matches its source binding",
            )]);
        }
        let Some((parameters, payload)) = closure_parts(&module.types, plan.application) else {
            return Err(vec![import_error(
                module,
                plan,
                "planned Effect result was not lowered to a closure",
            )]);
        };
        if parameters.len() != 1 || payload != plan.payload {
            return Err(vec![import_error(
                module,
                plan,
                "planned Effect result has an invalid token closure shape",
            )]);
        }
        let token = parameters[0];
        let host = psrs_core::effect::fresh_foreign_symbol(module);
        let host_body = psrs_core::effect::function_type(module, &plan.parameters, plan.payload);
        let host_type = quantify(module, &plan.quantified, host_body);
        let Some(external) = module
            .externals
            .iter_mut()
            .find(|external| external.symbol == plan.original)
        else {
            return Err(vec![import_error(
                module,
                plan,
                "effectful import has no external symbol",
            )]);
        };
        external.symbol = host;
        let Some(external_type) = module
            .external_types
            .iter_mut()
            .find(|external| external.symbol == plan.original)
        else {
            return Err(vec![import_error(
                module,
                plan,
                "effectful import has no checked source signature",
            )]);
        };
        external_type.symbol = host;
        external_type.ty = host_type;
        bindings.imports[plan.index].symbol = host;
        bindings.imports[plan.index].type_id = Some(host_type);
        let wrapper = build_wrapper(module, plan, host, host_body, token)?;
        module.declarations.push(wrapper);
        applied.push(AppliedSuspension {
            plan: plan.clone(),
            host,
            host_type,
        });
    }
    Ok(applied)
}

pub(super) fn verify(
    module: &CoreModule,
    bindings: &ExternalBindings,
    plans: &[SuspensionPlan],
    applied: &[AppliedSuspension],
) -> Result<(), Vec<BackendError>> {
    if plans.len() != applied.len() {
        return Err(vec![effect_error(
            module,
            module.span,
            "effect import plan and wrapper counts differ",
        )]);
    }
    for (plan, applied) in plans.iter().zip(applied) {
        if &applied.plan != plan {
            return Err(vec![import_error(
                module,
                plan,
                "effect import wrapper was built from different lowering evidence",
            )]);
        }
        let Some(binding) = bindings.imports.get(plan.index) else {
            return Err(vec![import_error(
                module,
                plan,
                "effect wrapper has no host binding",
            )]);
        };
        if binding.symbol != applied.host
            || binding.source_module != plan.source_module
            || binding.type_id != Some(applied.host_type)
        {
            return Err(vec![import_error(
                module,
                plan,
                "effect wrapper and host binding signatures differ",
            )]);
        }
        let Some(host_external) = module
            .externals
            .iter()
            .find(|external| external.symbol == applied.host)
        else {
            return Err(vec![import_error(
                module,
                plan,
                "effect wrapper host import is missing",
            )]);
        };
        if host_external.signature.is_none() {
            return Err(vec![import_error(
                module,
                plan,
                "effect wrapper host import has no source signature",
            )]);
        }
        let Some(wrapper) = module
            .declarations
            .iter()
            .find(|declaration| declaration.symbol == plan.original)
        else {
            return Err(vec![import_error(
                module,
                plan,
                "effect import wrapper declaration is missing",
            )]);
        };
        if wrapper.ty != plan.source
            || wrapper.quantified != plan.quantified
            || !wrapper_matches(module, wrapper, plan, applied.host)
        {
            return Err(vec![import_error(
                module,
                plan,
                "effect import wrapper does not preserve its planned signature and suspension",
            )]);
        }
        let Some((token_parameters, result)) = closure_parts(&module.types, plan.application)
        else {
            return Err(vec![import_error(
                module,
                plan,
                "planned Effect result is no longer a closure",
            )]);
        };
        if token_parameters.len() != 1 || result != plan.payload {
            return Err(vec![import_error(
                module,
                plan,
                "planned Effect result no longer matches its recorded payload",
            )]);
        }
    }
    Ok(())
}

fn build_wrapper(
    module: &CoreModule,
    plan: &SuspensionPlan,
    host: SymbolId,
    host_body: TypeId,
    token: TypeId,
) -> Result<Declaration, Vec<BackendError>> {
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
    let mut call = expr(ExprKind::Global(host), host_body, plan.span);
    let mut remaining = host_body;
    for (local, ty) in &binders {
        let Some((_, result)) = psrs_core::arrow_parts(&module.types, remaining) else {
            return Err(vec![import_error(
                module,
                plan,
                "planned host import type has fewer arguments than its source binding",
            )]);
        };
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
    if remaining != plan.payload {
        return Err(vec![import_error(
            module,
            plan,
            "planned host import result differs from its Effect payload",
        )]);
    }
    let mut arrow_types = Vec::new();
    let mut current = plan.source_body;
    for _ in &plan.parameters {
        arrow_types.push(current);
        let Some((_, result)) = psrs_core::arrow_parts(&module.types, current) else {
            return Err(vec![import_error(
                module,
                plan,
                "effectful import type is not a function of its planned payload",
            )]);
        };
        current = result;
    }
    if current != plan.application {
        return Err(vec![import_error(
            module,
            plan,
            "effect import source signature changed after planning",
        )]);
    }
    let token_local = fresh();
    let mut value = lambda(
        token_local,
        "token",
        token,
        call,
        plan.application,
        plan.span,
    );
    for (index, (local, ty)) in binders.iter().enumerate().rev() {
        value = lambda(*local, "value", *ty, value, arrow_types[index], plan.span);
    }
    Ok(Declaration {
        symbol: plan.original,
        name: plan.name.clone(),
        name_span: plan.span,
        quantified: plan.quantified.clone(),
        ty: plan.source,
        value,
        span: plan.span,
    })
}

fn wrapper_matches(
    module: &CoreModule,
    wrapper: &Declaration,
    plan: &SuspensionPlan,
    host: SymbolId,
) -> bool {
    let mut expression = &wrapper.value;
    let mut locals = Vec::with_capacity(plan.parameters.len() + 1);
    for parameter in &plan.parameters {
        let ExprKind::Lambda { binder, body } = &expression.kind else {
            return false;
        };
        if binder.ty != *parameter {
            return false;
        }
        locals.push(binder.id);
        expression = body;
    }
    let ExprKind::Lambda { binder, body } = &expression.kind else {
        return false;
    };
    let Some((tokens, result)) = closure_parts(&module.types, plan.application) else {
        return false;
    };
    if tokens != [binder.ty] || result != plan.payload || expression.ty != plan.application {
        return false;
    }
    locals.push(binder.id);
    let mut call = body.as_ref();
    for local in locals.iter().take(plan.parameters.len()).rev() {
        let ExprKind::Application(function, argument) = &call.kind else {
            return false;
        };
        if !matches!(argument.kind, ExprKind::Local(id) if id == *local) {
            return false;
        }
        call = function;
    }
    matches!(call.kind, ExprKind::Global(symbol) if symbol == host)
}

fn strip_foralls(module: &CoreModule, mut ty: TypeId) -> Option<TypeId> {
    let mut remaining = module.types.len();
    while let Some(Type::ForAll { body, .. }) = module.types.get(ty.0 as usize) {
        if remaining == 0 {
            return None;
        }
        remaining -= 1;
        ty = *body;
    }
    Some(ty)
}

fn quantify(module: &mut CoreModule, variables: &[TypeVariableId], body: TypeId) -> TypeId {
    if variables.is_empty() {
        return body;
    }
    let ty = Type::ForAll {
        variables: variables.to_vec(),
        body,
    };
    if let Some(index) = module.types.iter().position(|candidate| candidate == &ty) {
        TypeId(index as u32)
    } else {
        module.types.push(ty);
        TypeId((module.types.len() - 1) as u32)
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

fn expr(kind: ExprKind, ty: TypeId, span: TextRange) -> Expr {
    Expr { kind, ty, span }
}
