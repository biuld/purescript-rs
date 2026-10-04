//! Effect lowering coordinator. Abstract identities and the import plan are
//! checked before the Core type table erases `Effect` applications.

mod entry;
mod suspension;
mod verify;

use crate::BackendError;
use crate::bindings::ExternalBindings;
use psrs_core::{Module as CoreModule, effect::EffectCompilation};

pub(crate) fn lower_effects(
    module: &mut CoreModule,
    bindings: &mut ExternalBindings,
    context: &EffectCompilation,
) -> Result<(), Vec<BackendError>> {
    verify::trusted_contract(module, bindings, &context.trusted)?;
    validate_entry_context(module, context)?;
    let suspensions = suspension::plan(module, bindings, &context.trusted)?;
    let lowering = psrs_core::effect::lower_effects(module, &context.trusted)
        .map_err(|errors| verify::verification_errors(&errors))?;

    let applied = suspension::apply(module, bindings, &suspensions)?;
    if let Some(entry) = context.command_entry {
        entry::install(module, &context.trusted, entry)?;
    }

    lowering
        .verify(module)
        .map_err(|errors| verify::verification_errors(&errors))?;
    if let Err(errors) = module.verify() {
        return Err(verify::verification_errors(&errors));
    }
    suspension::verify(module, bindings, &suspensions, &applied)?;
    let synthesized = lowering.synthesized.to_vec();
    bindings
        .imports
        .retain(|binding| !synthesized.contains(&binding.symbol));
    Ok(())
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
