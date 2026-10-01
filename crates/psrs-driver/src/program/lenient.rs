use super::*;

/// Resolves a program while tolerating imports whose modules are not provided,
/// and reports every resolution diagnostic. This is used to measure module,
/// import, export, and name resolution against the corpus, where support
/// libraries such as `Prelude` are not part of the input.
pub fn check_program_lenient(sources: &[(&str, &str)]) -> Result<(), Vec<ProgramDiagnostic>> {
    resolve_program_lenient(sources).map(|_| ())
}

/// Resolves a lenient program and then kind-checks every module that resolved.
/// Missing support libraries still produce resolution diagnostics, but a module
/// that resolves without them is kind-checked, so the M3 layer is measurable
/// against the corpus.
pub fn check_program_kinds_lenient(sources: &[(&str, &str)]) -> Result<(), Vec<ProgramDiagnostic>> {
    let mut modules = lower_program_to_ast(sources)?;
    let options = psrs_resolve::ResolveOptions {
        tolerate_missing_modules: true,
    };
    let (resolved, program_errors) =
        psrs_resolve::resolve_program_partial(std::mem::take(&mut modules), options);
    let mut errors = Vec::new();
    for error in program_errors {
        errors.push(ProgramDiagnostic {
            source: error.module,
            diagnostic: coded_diagnostic(
                "P3 resolve",
                error.error.span,
                error.error.error_code(),
                error.error.message(),
            ),
        });
    }
    let resolved_modules = resolved
        .iter()
        .filter_map(Option::as_ref)
        .cloned()
        .collect::<Vec<_>>();
    let (_, role_errors) = psrs_kind::check_roles(&resolved_modules);
    for (module, error) in role_errors {
        errors.push(ProgramDiagnostic {
            source: module.0 as usize,
            diagnostic: coded_diagnostic(
                "P5 kind check",
                error.span,
                Some(error.code),
                error.message,
            ),
        });
    }
    for (source, module) in resolved.iter().enumerate() {
        let Some(module) = module else {
            continue;
        };
        for error in psrs_kind::check_module(module) {
            errors.push(ProgramDiagnostic {
                source,
                diagnostic: coded_diagnostic(
                    "P5 kind check",
                    error.span,
                    Some(error.code),
                    error.message,
                ),
            });
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn resolve_program_lenient(
    sources: &[(&str, &str)],
) -> Result<Vec<psrs_hir::Module>, Vec<ProgramDiagnostic>> {
    let modules = lower_program_to_ast(sources)?;
    let options = psrs_resolve::ResolveOptions {
        tolerate_missing_modules: true,
    };
    match psrs_resolve::resolve_program_with_options(modules, options) {
        Ok(resolved) => Ok(resolved),
        Err(program_errors) => Err(program_errors
            .into_iter()
            .map(|error| ProgramDiagnostic {
                source: error.module,
                diagnostic: coded_diagnostic(
                    "P3 resolve",
                    error.error.span,
                    error.error.error_code(),
                    error.error.message(),
                ),
            })
            .collect()),
    }
}

fn lower_program_to_ast(
    sources: &[(&str, &str)],
) -> Result<Vec<psrs_ast::Module>, Vec<ProgramDiagnostic>> {
    let mut modules = Vec::with_capacity(sources.len());
    let mut errors = Vec::new();
    for (index, (name, text)) in sources.iter().enumerate() {
        match lower_source_to_ast(name, text) {
            Ok(module) => modules.push(module),
            Err(diagnostics) => {
                for diagnostic in diagnostics {
                    errors.push(ProgramDiagnostic {
                        source: index,
                        diagnostic,
                    });
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(modules)
    } else {
        Err(errors)
    }
}
