//! Effect lowering coordinator. Abstract identities and the import plan are
//! checked before the Core type table erases `Effect` applications.

mod entry;
mod suspension;
mod verify;

use super::boundary::{RepresentationPolicy, RepresentationRegistry};
use crate::BackendError;
use crate::bindings::ExternalBindings;
use psrs_core::{Module as CoreModule, TypeConstructor, effect::EffectCompilation};
use std::collections::HashSet;

/// Source Core captured before effect applications become closures, plus the
/// representation registry the Effect owner contributes to conversion.
pub(crate) struct EffectPreparation {
    pub(crate) source: CoreModule,
    pub(crate) registry: RepresentationRegistry,
}

pub(crate) fn lower_effects(
    module: &mut CoreModule,
    bindings: &mut ExternalBindings,
    context: &EffectCompilation,
) -> Result<EffectPreparation, Vec<BackendError>> {
    verify::trusted_contract(module, bindings, &context.trusted)?;
    validate_entry_context(module, context)?;
    let suspensions = suspension::plan(module, bindings, &context.trusted)?;
    let source = module.clone();
    let lowering = psrs_core::effect::lower_effects(module, &context.trusted)
        .map_err(|errors| verify::verification_errors(&errors))?;
    let registry = effect_registry(module, &context.trusted, &lowering)?;

    let applied = suspension::apply(module, bindings, &suspensions)?;
    if let Some(entry) = context.command_entry {
        entry::install(module, &context.trusted, entry)?;
    }

    lowering
        .verify(module)
        .map_err(|errors| verify::verification_errors(&errors))?;
    if let Err(errors) = module.verify_with_source(&source) {
        return Err(verify::verification_errors(&errors));
    }
    suspension::verify(module, bindings, &suspensions, &applied)?;
    let synthesized = lowering.synthesized.to_vec();
    bindings
        .imports
        .retain(|binding| !synthesized.contains(&binding.symbol));
    Ok(EffectPreparation { source, registry })
}

/// The Effect representation owner registers its constructor's stored calling
/// convention: the runtime token chosen by effect lowering. The payload
/// remains the application's result and is not a hidden parameter.
fn effect_registry(
    module: &CoreModule,
    trusted: &psrs_core::effect::TrustedEffect,
    lowering: &psrs_core::effect::EffectLowering,
) -> Result<RepresentationRegistry, Vec<BackendError>> {
    let mut tokens = HashSet::new();
    for closure in &lowering.closures {
        tokens.insert(closure.token);
    }
    if tokens.len() > 1 {
        return Err(vec![verify::effect_error(
            module,
            module.span,
            "Effect lowering produced more than one runtime token",
        )]);
    }
    let mut registry = RepresentationRegistry::new();
    if let Some(token) = tokens.into_iter().next() {
        registry.register(
            TypeConstructor::User(trusted.effect_type),
            RepresentationPolicy::Fixed(vec![token]),
        );
    }
    Ok(registry)
}

fn validate_entry_context(
    module: &CoreModule,
    context: &EffectCompilation,
) -> Result<(), Vec<BackendError>> {
    let selected = module.entry;
    match (selected, context.command_entry) {
        (Some(selected), Some(entry)) if selected != entry.symbol => {
            Err(vec![verify::effect_error(
                module,
                module.span,
                "Effect command entry does not match the selected source entry",
            )])
        }
        (Some(_), Some(entry)) => entry::validate_source(module, &context.trusted, entry),
        (Some(selected), None) => {
            let Some(declaration) = module
                .declarations
                .iter()
                .find(|declaration| declaration.symbol == selected)
            else {
                return Ok(());
            };
            if psrs_core::effect::effect_application(
                module,
                declaration.ty,
                context.trusted.effect_type,
            )
            .is_some()
            {
                Err(vec![verify::effect_error(
                    module,
                    declaration.name_span,
                    "missing Effect command entry metadata for the selected source entry",
                )])
            } else {
                Ok(())
            }
        }
        (None, Some(_)) => Err(vec![verify::effect_error(
            module,
            module.span,
            "Effect command entry metadata has no selected source entry",
        )]),
        (None, None) => Ok(()),
    }
}
