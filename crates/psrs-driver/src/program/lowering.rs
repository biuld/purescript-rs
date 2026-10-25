use super::*;

pub(super) fn lower_program_inner(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
    runner: Option<(&str, &str)>,
    support: SupportEnvironment,
) -> Result<(psrs_core::Module, Vec<ProgramWarning>), Vec<ProgramDiagnostic>> {
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
    let (typed, warnings) = typecheck_resolved_program_with_warnings(resolved, support)?;
    let mut modules = Vec::with_capacity(typed.len());
    for (index, module) in typed.into_iter().enumerate() {
        match psrs_core::lower_module_unverified(module) {
            Ok(mut core) => {
                let source_id = psrs_span::SourceId(core.id.0);
                let (_, text) = sources[core.id.0 as usize];
                core.sources.insert(
                    source_id,
                    psrs_span::SourceFile::new(core.name.clone(), text),
                );
                modules.push(core);
            }
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
    if let Err(errors) = linked.compact_types() {
        return Err(errors
            .into_iter()
            .map(|error| ProgramDiagnostic {
                source: DiagnosticOrigin::Source(error.module.0 as usize),
                diagnostic: diagnostic("P7 Core verification", error.span, error.message),
            })
            .collect());
    }
    Ok((linked, warnings))
}

pub(crate) fn lower_program_to_core_and_warnings(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<(psrs_core::Module, Vec<ProgramWarning>), Vec<ProgramDiagnostic>> {
    lower_program_inner(sources, trusted_prefix, None, SupportEnvironment::Imports)
}
