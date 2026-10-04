use super::verify::effect_error;
use crate::BackendError;
use psrs_core::{
    Binder, Binding, Declaration, Expr, ExprKind, Module as CoreModule, Type, TypeConstructor,
    TypeId,
};
use psrs_hir::SymbolId;
use psrs_span::TextRange;

pub(super) fn validate_source(
    module: &CoreModule,
    trusted: &psrs_core::effect::TrustedEffect,
    entry: psrs_core::effect::EffectCommandEntry,
) -> Result<(), Vec<BackendError>> {
    let Some(main) = module
        .declarations
        .iter()
        .find(|declaration| declaration.symbol == entry.symbol)
    else {
        return Err(vec![effect_error(
            module,
            module.span,
            "Effect command entry has no source declaration",
        )]);
    };
    if !main.quantified.is_empty()
        || psrs_core::effect::effect_application(module, main.ty, trusted.effect_type)
            .is_none_or(|(_, payload)| !is_unit(module, payload))
    {
        return Err(vec![effect_error(
            module,
            main.name_span,
            "Effect command entry must have type Effect Unit",
        )]);
    }
    Ok(())
}

pub(super) fn install(
    module: &mut CoreModule,
    trusted: &psrs_core::effect::TrustedEffect,
    entry: psrs_core::effect::EffectCommandEntry,
) -> Result<(), Vec<BackendError>> {
    let Some(main) = module
        .declarations
        .iter()
        .find(|declaration| declaration.symbol == entry.symbol)
        .cloned()
    else {
        return Err(vec![effect_error(
            module,
            module.span,
            "Effect command entry has no source declaration",
        )]);
    };
    let Some((token_parameters, unit)) = psrs_core::closure_parts(&module.types, main.ty) else {
        return Err(vec![effect_error(
            module,
            main.name_span,
            "lowered Effect command entry is not an ordinary token closure",
        )]);
    };
    if token_parameters.len() != 1 || !is_unit(module, unit) {
        return Err(vec![effect_error(
            module,
            main.name_span,
            "lowered Effect command entry must take one token and return Unit",
        )]);
    }
    let Some(run) = trusted
        .operations
        .iter()
        .find(|operation| operation.operation == psrs_core::effect::EffectOperation::Run)
        .map(|operation| operation.symbol)
    else {
        return Err(vec![effect_error(
            module,
            main.name_span,
            "trusted runEffect operation is missing from the Effect contract",
        )]);
    };
    if !module
        .declarations
        .iter()
        .any(|declaration| declaration.symbol == run)
    {
        return Err(vec![effect_error(
            module,
            main.name_span,
            "trusted runEffect operation was not lowered",
        )]);
    }
    let int = intern(module, Type::Constructor(TypeConstructor::Int));
    let run_type = psrs_core::effect::function_type(module, &[main.ty], unit);
    let result_local = psrs_core::effect::fresh_local(module);
    let binder = Binder {
        id: result_local,
        name: "effectResult".to_string(),
        ty: unit,
        span: main.name_span,
    };
    let run_action = expr(
        ExprKind::Application(
            Box::new(expr(ExprKind::Global(run), run_type, main.name_span)),
            Box::new(expr(
                ExprKind::Global(entry.symbol),
                main.ty,
                main.name_span,
            )),
        ),
        unit,
        main.name_span,
    );
    let value = expr(
        ExprKind::Let {
            bindings: vec![Binding {
                binder,
                quantified: Vec::new(),
                value: run_action,
                span: main.name_span,
            }],
            body: Box::new(expr(ExprKind::Integer(0), int, main.name_span)),
        },
        int,
        main.name_span,
    );
    let symbol = fresh_entry_symbol(module, main.symbol.module);
    module.declarations.push(Declaration {
        symbol,
        name: "psrs.command-entry".to_string(),
        name_span: main.name_span,
        quantified: Vec::new(),
        ty: int,
        value,
        span: main.span,
    });
    module.entry = Some(symbol);
    if !valid_wrapper(module, symbol, entry.symbol, run, unit, int) {
        return Err(vec![effect_error(
            module,
            main.name_span,
            "generated Effect command entry does not run the selected action exactly once",
        )]);
    }
    Ok(())
}

fn valid_wrapper(
    module: &CoreModule,
    wrapper_symbol: SymbolId,
    source: SymbolId,
    run: SymbolId,
    unit: TypeId,
    int: TypeId,
) -> bool {
    let Some(wrapper) = module
        .declarations
        .iter()
        .find(|declaration| declaration.symbol == wrapper_symbol)
    else {
        return false;
    };
    if wrapper.ty != int || !wrapper.quantified.is_empty() {
        return false;
    }
    let ExprKind::Let { bindings, body } = &wrapper.value.kind else {
        return false;
    };
    if bindings.len() != 1 || bindings[0].binder.ty != unit || bindings[0].value.ty != unit {
        return false;
    }
    let ExprKind::Application(function, action) = &bindings[0].value.kind else {
        return false;
    };
    if !matches!(function.kind, ExprKind::Global(symbol) if symbol == run)
        || !matches!(action.kind, ExprKind::Global(symbol) if symbol == source)
    {
        return false;
    }
    matches!(body.kind, ExprKind::Integer(0)) && body.ty == int
}

fn is_unit(module: &CoreModule, ty: TypeId) -> bool {
    matches!(
        module.types.get(ty.0 as usize),
        Some(Type::Constructor(TypeConstructor::Unit))
    )
}

fn intern(module: &mut CoreModule, ty: Type) -> TypeId {
    if let Some(index) = module.types.iter().position(|current| current == &ty) {
        TypeId(index as u32)
    } else {
        module.types.push(ty);
        TypeId((module.types.len() - 1) as u32)
    }
}

fn fresh_entry_symbol(module: &CoreModule, owner: psrs_hir::ModuleId) -> SymbolId {
    let next = module
        .declarations
        .iter()
        .filter(|declaration| declaration.symbol.module == owner)
        .map(|declaration| declaration.symbol.index)
        .chain(
            module
                .externals
                .iter()
                .filter(|external| external.symbol.module == owner)
                .map(|external| external.symbol.index),
        )
        .chain(
            module
                .constructors
                .iter()
                .filter(|constructor| constructor.symbol.module == owner)
                .map(|constructor| constructor.symbol.index),
        )
        .max()
        .map_or(0, |index| index.saturating_add(1));
    SymbolId::new(owner, next)
}

fn expr(kind: ExprKind, ty: TypeId, span: TextRange) -> Expr {
    Expr { kind, ty, span }
}
