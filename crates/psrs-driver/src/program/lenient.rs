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
    let mut errors = Vec::new();
    let resolved = desugar_resolved(resolve_partial(sources, &mut errors), &mut errors);
    let (_, role_errors) = psrs_kind::check_roles(&resolved);
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

/// Resolves a program leniently, then kind-checks and type checks every module
/// that resolved, and reports every diagnostic from every stage.
///
/// This is the M4/M5 measurement entry point. It is lenient in the same way as
/// [`check_program_kinds_lenient`]: a module whose import cannot be satisfied is
/// reported and skipped rather than aborting the program, so a corpus case that
/// needs a library module we do not provide still reaches the type checker for
/// the modules it does provide. A module is type checked only when it
/// desugars and kind checks, so a type error is never attributed to a module
/// that failed an earlier stage.
pub fn check_program_types_lenient(sources: &[(&str, &str)]) -> Result<(), Vec<ProgramDiagnostic>> {
    let mut errors = Vec::new();
    let resolved = desugar_resolved(resolve_partial(sources, &mut errors), &mut errors);
    let (checked_kinds, role_errors) = psrs_kind::check_roles(&resolved);
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
    let signatures = declared_signatures(&resolved);
    let mut known_types = resolved
        .iter()
        .flat_map(|module| module.types.iter().cloned())
        .collect::<Vec<_>>();
    known_types.extend(
        psrs_hir::primitive_type_declarations()
            .into_iter()
            .map(|(_, declaration)| declaration),
    );
    let instance_sets = resolved
        .iter()
        .map(|module| module.instances.clone())
        .collect::<Vec<_>>();
    let dependencies = module_dependencies(&resolved);
    let module_names = resolved
        .iter()
        .map(|module| (module.id, module.name.clone()))
        .collect::<HashMap<_, _>>();

    for (source, module) in resolved.into_iter().enumerate() {
        if !psrs_kind::check_module(&module).is_empty() {
            continue;
        }
        let imported = imported_signatures(&module, &signatures);
        let imported_instances =
            imported_instance_declarations(&dependencies, source, &instance_sets);
        let check = psrs_typecheck::typecheck_module_with_checked_kinds_and_module_names(
            module,
            &imported,
            None,
            false,
            psrs_typecheck::TypecheckContext {
                known_types: &known_types,
                imported_instances: &imported_instances,
                module_names: &module_names,
                checked_kinds: &checked_kinds,
            },
        );
        if let Err(module_errors) = check {
            for error in module_errors {
                errors.push(ProgramDiagnostic {
                    source,
                    diagnostic: coded_diagnostic(
                        "P5 typecheck",
                        error.span,
                        error.error_code(),
                        error.message(),
                    ),
                });
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn desugar_resolved(
    resolved: Vec<psrs_hir::Module>,
    errors: &mut Vec<ProgramDiagnostic>,
) -> Vec<psrs_hir::Module> {
    let mut desugared = Vec::with_capacity(resolved.len());
    for module in resolved {
        let source = module.id.0 as usize;
        match psrs_desugar::desugar_module(module) {
            Ok(module) => desugared.push(module),
            Err(module_errors) => {
                for error in module_errors {
                    errors.push(ProgramDiagnostic {
                        source,
                        diagnostic: diagnostic("P4 desugar", error.span, error.message),
                    });
                }
            }
        }
    }
    desugared
}

/// Resolves leniently, pushing every resolution and parse diagnostic into
/// `errors` and returning the modules that resolved, in source order. A source
/// that does not parse or cannot resolve has no entry, which is what keeps later
/// stages from attributing an error to it.
fn resolve_partial(
    sources: &[(&str, &str)],
    errors: &mut Vec<ProgramDiagnostic>,
) -> Vec<psrs_hir::Module> {
    let mut modules = match lower_program_to_ast(sources) {
        Ok(modules) => modules,
        Err(parse_errors) => {
            errors.extend(parse_errors);
            return Vec::new();
        }
    };
    let options = psrs_resolve::ResolveOptions {
        tolerate_missing_modules: true,
    };
    let (resolved, program_errors) =
        psrs_resolve::resolve_program_partial(std::mem::take(&mut modules), options);
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
    resolved.into_iter().flatten().collect()
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
