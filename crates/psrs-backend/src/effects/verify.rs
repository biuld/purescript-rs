use crate::BackendError;
use crate::bindings::ExternalBindings;
use psrs_core::{Module as CoreModule, Type, TypeConstructor};
use psrs_hir::{ExternalKind, TypeId as HirTypeId, TypeVariableId};
use psrs_span::TextRange;
use std::collections::HashSet;

const EFFECT_INTERFACE: &str = "psrs:effect";

pub(super) fn trusted_contract(
    module: &CoreModule,
    bindings: &ExternalBindings,
    trusted: &psrs_core::effect::TrustedEffect,
) -> Result<(), Vec<BackendError>> {
    if !module.opaque_ids.contains(&trusted.effect_type)
        || !module.types.iter().any(|ty| {
            matches!(ty, Type::Constructor(TypeConstructor::User(id)) if *id == trusted.effect_type)
        })
    {
        return Err(vec![effect_error(
            module,
            module.span,
            "trusted Effect identity is missing or is not an opaque type",
        )]);
    }
    let mut symbols = HashSet::new();
    let mut operations = HashSet::new();
    for operation in &trusted.operations {
        if !symbols.insert(operation.symbol) || !operations.insert(operation.operation) {
            return Err(vec![effect_error(
                module,
                module.span,
                "trusted Effect operation bindings are duplicated",
            )]);
        }
        let Some(external) = module
            .externals
            .iter()
            .find(|external| external.symbol == operation.symbol)
        else {
            return Err(vec![effect_error(
                module,
                module.span,
                "trusted Effect operation binding has no external declaration",
            )]);
        };
        let expected = operation.operation.wit_function();
        if !matches!(
            &external.kind,
            ExternalKind::Wit { interface, function }
                if interface == EFFECT_INTERFACE && function == expected
        ) {
            return Err(vec![source_effect_error(
                module,
                external_source_module(module, operation.symbol),
                external
                    .signature
                    .as_ref()
                    .map_or(module.span, |ty| ty.span),
                "trusted Effect operation identity does not match its WIT binding",
            )]);
        }
        let Some(binding) = bindings
            .imports
            .iter()
            .find(|binding| binding.symbol == operation.symbol)
        else {
            return Err(vec![source_effect_error(
                module,
                external_source_module(module, operation.symbol),
                external
                    .signature
                    .as_ref()
                    .map_or(module.span, |ty| ty.span),
                "trusted Effect operation has no external binding metadata",
            )]);
        };
        let Some(signature) = binding.type_id else {
            return Err(vec![source_effect_error(
                module,
                binding.source_module,
                binding.span,
                "trusted Effect operation has no checked source signature",
            )]);
        };
        if !valid_core_operation_signature(
            module,
            signature,
            trusted.effect_type,
            operation.operation,
        ) {
            return Err(vec![source_effect_error(
                module,
                binding.source_module,
                binding.span,
                &format!(
                    "trusted Effect operation `{expected}` has an invalid checked source signature"
                ),
            )]);
        }
    }
    if [
        psrs_core::effect::EffectOperation::Pure,
        psrs_core::effect::EffectOperation::Bind,
        psrs_core::effect::EffectOperation::Run,
        psrs_core::effect::EffectOperation::Trap,
    ]
    .into_iter()
    .any(|required| !operations.contains(&required))
    {
        return Err(vec![effect_error(
            module,
            module.span,
            "trusted Effect contract is missing a required operation binding",
        )]);
    }
    for external in &module.externals {
        if matches!(
            &external.kind,
            ExternalKind::Wit { interface, .. } if interface == EFFECT_INTERFACE
        ) && !symbols.contains(&external.symbol)
        {
            return Err(vec![source_effect_error(
                module,
                external_source_module(module, external.symbol),
                external
                    .signature
                    .as_ref()
                    .map_or(module.span, |ty| ty.span),
                "Effect WIT import is missing from the trusted operation bindings",
            )]);
        }
    }
    Ok(())
}

fn valid_core_operation_signature(
    module: &CoreModule,
    ty: psrs_core::TypeId,
    effect: HirTypeId,
    operation: psrs_core::effect::EffectOperation,
) -> bool {
    let Some((variables, parameters, result)) = core_function_signature(module, ty) else {
        return false;
    };
    match operation {
        psrs_core::effect::EffectOperation::Pure => {
            variables.len() == 1
                && parameters.len() == 1
                && core_variable(module, parameters[0]) == Some(variables[0])
                && core_effect_payload(module, result, effect)
                    .is_some_and(|payload| core_variable(module, payload) == Some(variables[0]))
        }
        psrs_core::effect::EffectOperation::Bind => {
            if variables.len() != 2 || parameters.len() != 2 {
                return false;
            }
            let first = core_effect_payload(module, parameters[0], effect);
            let continuation = psrs_core::arrow_parts(&module.types, parameters[1]);
            let result = core_effect_payload(module, result, effect);
            matches!((first, continuation, result),
                (Some(first), Some((input, output)), Some(output_result))
                    if core_variable(module, first) == Some(variables[0])
                        && core_variable(module, input) == Some(variables[0])
                        && core_effect_payload(module, output, effect)
                            .is_some_and(|payload| core_variable(module, payload) == Some(variables[1]))
                        && core_variable(module, output_result) == Some(variables[1]))
        }
        psrs_core::effect::EffectOperation::Run => {
            variables.len() == 1
                && parameters.len() == 1
                && core_effect_payload(module, parameters[0], effect)
                    .is_some_and(|payload| core_variable(module, payload) == Some(variables[0]))
                && core_variable(module, result) == Some(variables[0])
        }
        psrs_core::effect::EffectOperation::Trap => {
            parameters.is_empty()
                && variables.is_empty()
                && core_effect_payload(module, result, effect).is_some_and(|payload| {
                    matches!(
                        module.types.get(payload.0 as usize),
                        Some(Type::Constructor(TypeConstructor::Unit))
                    )
                })
        }
    }
}

fn core_function_signature(
    module: &CoreModule,
    ty: psrs_core::TypeId,
) -> Option<(
    Vec<TypeVariableId>,
    Vec<psrs_core::TypeId>,
    psrs_core::TypeId,
)> {
    let mut quantified = Vec::new();
    let mut body = ty;
    let mut seen = HashSet::new();
    while seen.insert(body) {
        let Some((variables, inner)) = psrs_core::forall_parts(&module.types, body) else {
            break;
        };
        quantified.extend_from_slice(variables);
        body = inner;
    }
    if !seen.insert(body) && psrs_core::forall_parts(&module.types, body).is_some() {
        return None;
    }
    let mut parameters = Vec::new();
    let mut arrow_seen = HashSet::new();
    loop {
        if !arrow_seen.insert(body) {
            return None;
        }
        let Some((parameter, result)) = psrs_core::arrow_parts(&module.types, body) else {
            break;
        };
        parameters.push(parameter);
        body = result;
    }
    Some((quantified, parameters, body))
}

fn core_variable(module: &CoreModule, ty: psrs_core::TypeId) -> Option<TypeVariableId> {
    match module.types.get(ty.0 as usize) {
        Some(Type::Variable(variable)) => Some(*variable),
        _ => None,
    }
}

fn core_effect_payload(
    module: &CoreModule,
    ty: psrs_core::TypeId,
    effect: HirTypeId,
) -> Option<psrs_core::TypeId> {
    psrs_core::effect::effect_application(module, ty, effect).map(|(_, payload)| payload)
}

pub(super) fn verification_errors(
    module: &CoreModule,
    errors: &[psrs_core::VerifyError],
) -> Vec<BackendError> {
    errors
        .iter()
        .map(|error| effect_error(module, error.span, error.message))
        .collect()
}

pub(super) fn effect_error(module: &CoreModule, span: TextRange, message: &str) -> BackendError {
    let error = BackendError::invalid_ir("P8 effect lowering", span, message.to_string());
    match module.entry {
        Some(entry) => error.with_module(entry.module),
        None => error,
    }
}

pub(super) fn source_effect_error(
    _module: &CoreModule,
    source_module: psrs_hir::ModuleId,
    span: TextRange,
    message: &str,
) -> BackendError {
    BackendError::invalid_ir("P8 effect lowering", span, message.to_string())
        .with_module(source_module)
}

pub(super) fn external_source_module(
    module: &CoreModule,
    symbol: psrs_hir::SymbolId,
) -> psrs_hir::ModuleId {
    module
        .external_types
        .iter()
        .find(|external| external.symbol == symbol)
        .map_or(module.id, |external| external.source_module)
}
