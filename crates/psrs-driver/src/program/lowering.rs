use super::*;

pub(super) fn lower_program_inner(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
    runner: Option<(&str, &str)>,
) -> Result<
    (
        psrs_core::Module,
        Vec<ProgramWarning>,
        Option<psrs_core::effect::EffectCompilation>,
    ),
    Vec<ProgramDiagnostic>,
> {
    let resolved = resolve_program_sources(sources)?;
    let selected_runner = runner
        .map(|(owner, name)| {
            let candidates = resolved
                .iter()
                .take(if trusted_prefix == 0 {
                    resolved.len()
                } else {
                    trusted_prefix
                })
                .filter(|module| module.name == owner)
                .flat_map(|module| module.declarations.iter().filter(|decl| decl.name == name))
                .collect::<Vec<_>>();
            match candidates.as_slice() {
                [runner] => Ok(runner.symbol),
                _ => Err(vec![ProgramDiagnostic {
                    source: DiagnosticOrigin::Program,
                    diagnostic: diagnostic(
                        "P7 command runner",
                        psrs_span::TextRange::new(0, 0),
                        "configured command runner must resolve to exactly one source declaration",
                    ),
                }]),
            }
        })
        .transpose()?;
    let entry = effects::require_unambiguous_entry(effects::select_entry(&resolved))?;
    let trusted = if selected_runner.is_none() {
        let trusted = effects::trusted_effect(&resolved, trusted_prefix)?;
        effects::check_run_effect_scope(&resolved, entry, trusted.as_ref())?;
        trusted
    } else {
        None
    };
    let (typed, warnings) = typecheck_resolved_program_with_warnings(resolved)?;
    let command_entry = if selected_runner.is_none() {
        classify_command_entry(&typed, entry, trusted.as_ref())?
    } else {
        None
    };
    let mut modules = Vec::with_capacity(typed.len());
    for (index, module) in typed.into_iter().enumerate() {
        match psrs_core::lower_module_unverified(module) {
            Ok(core) => modules.push(core),
            Err(errors) => {
                return Err(errors
                    .into_iter()
                    .map(|error| ProgramDiagnostic {
                        source: DiagnosticOrigin::Source(index),
                        diagnostic: diagnostic("P6 Core lowering", error.span, error.message),
                    })
                    .collect());
            }
        }
    }
    let mut linked = psrs_core::link(modules);
    if let Some(entry) = entry {
        let symbol = if let Some(runner) = selected_runner {
            psrs_core::command::normalize_entry(&mut linked, entry.symbol, runner).map_err(
                |errors| {
                    errors
                        .into_iter()
                        .map(|error| ProgramDiagnostic {
                            source: DiagnosticOrigin::Source(error.module.0 as usize),
                            diagnostic: diagnostic("P7 command runner", error.span, error.message),
                        })
                        .collect::<Vec<_>>()
                },
            )?
        } else {
            entry.symbol
        };
        linked.entry = Some(symbol);
        psrs_core::prune_unreachable(&mut linked, symbol);
    }
    if let Err(errors) = linked.verify() {
        return Err(errors
            .into_iter()
            .map(|error| ProgramDiagnostic {
                source: DiagnosticOrigin::Source(error.module.0 as usize),
                diagnostic: diagnostic("P7 Core verification", error.span, error.message),
            })
            .collect());
    }
    let context = trusted.map(|trusted| psrs_core::effect::EffectCompilation {
        trusted,
        command_entry,
    });
    Ok((linked, warnings, context))
}

pub(crate) fn lower_program_to_core_and_effect_context(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<
    (
        psrs_core::Module,
        Vec<ProgramWarning>,
        Option<psrs_core::effect::EffectCompilation>,
    ),
    Vec<ProgramDiagnostic>,
> {
    lower_program_inner(sources, trusted_prefix, None)
}

#[cfg(test)]
pub(crate) fn lower_program_to_core_with_runner(
    sources: &[(&str, &str)],
    owner: &str,
    function: &str,
) -> Result<psrs_core::Module, Vec<ProgramDiagnostic>> {
    lower_program_inner(sources, 0, Some((owner, function))).map(|(core, _, _)| core)
}
