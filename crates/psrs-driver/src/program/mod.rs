//! Multi-module program pipelines: resolution, dependency-ordered type
//! checking against imported signatures, Core lowering, and linking.

use super::{
    Artifact, ProgramDiagnostic, backend_warnings, coded_diagnostic, diagnostic,
    lower_source_to_ast,
};

pub use library::compile_program_sources_with_prelude;
use std::collections::HashMap;

mod effects;
mod library;

/// Compiles a whole program to a single Wasm component. Every module is type
/// checked in dependency order and lowered to Core; the modules are then linked
/// into one before the backend runs.
pub fn compile_program_sources(
    sources: &[(&str, &str)],
) -> Result<Artifact, Vec<ProgramDiagnostic>> {
    compile_program_sources_with_trusted_prefix(sources, 0)
}

fn compile_program_sources_with_trusted_prefix(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<Artifact, Vec<ProgramDiagnostic>> {
    let core = lower_program_to_core_with_trusted_prefix(sources, trusted_prefix)?;
    let output = psrs_backend::compile(core).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| ProgramDiagnostic {
                source: error.module.map_or(0, |module| module.0 as usize),
                diagnostic: diagnostic(error.pass, error.span, error.message),
            })
            .collect::<Vec<_>>()
    })?;
    let warnings = backend_warnings(output.warnings, trusted_prefix);
    Ok(Artifact {
        wasm: output.wasm,
        wat: output.wat,
        warnings,
    })
}

#[cfg(test)]
pub(crate) fn lower_program_to_core(
    sources: &[(&str, &str)],
) -> Result<psrs_core::Module, Vec<ProgramDiagnostic>> {
    lower_program_to_core_with_trusted_prefix(sources, 0)
}

pub(crate) fn lower_program_to_core_with_trusted_prefix(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<psrs_core::Module, Vec<ProgramDiagnostic>> {
    let typed = typecheck_program_sources_with_trusted_prefix(sources, trusted_prefix)?;
    let entry = select_entry(&typed)?;
    let mut modules = Vec::with_capacity(typed.len());
    for (index, module) in typed.into_iter().enumerate() {
        match psrs_core::lower_module_unverified(module) {
            Ok(core) => modules.push(core),
            Err(errors) => {
                return Err(errors
                    .into_iter()
                    .map(|error| ProgramDiagnostic {
                        source: index,
                        diagnostic: diagnostic("P6 Core lowering", error.span, error.message),
                    })
                    .collect());
            }
        }
    }
    let mut linked = psrs_core::link(modules);
    if let Some(entry) = entry {
        linked.entry = Some(entry);
        psrs_core::prune_unreachable(&mut linked, entry);
    }
    if let Err(errors) = linked.verify() {
        return Err(errors
            .into_iter()
            .map(|error| ProgramDiagnostic {
                source: error.module.0 as usize,
                diagnostic: diagnostic("P7 Core verification", error.span, error.message),
            })
            .collect());
    }
    Ok(linked)
}

/// Selects one deterministic program entry before linking. A command program
/// uses `Main.main` when that module is present; otherwise a source list with a
/// single `main` declaration is accepted. Ambiguous entries are frontend-facing
/// diagnostics instead of being resolved by input order; a missing entry is
/// left for the backend to diagnose after Core lowering so earlier backend
/// limitation diagnostics remain useful.
fn select_entry(
    modules: &[psrs_thir::Module],
) -> Result<Option<psrs_hir::SymbolId>, Vec<ProgramDiagnostic>> {
    let candidates = modules
        .iter()
        .enumerate()
        .flat_map(|(source, module)| {
            module
                .declarations
                .iter()
                .filter(|declaration| declaration.name == "main")
                .map(move |declaration| (source, module, declaration))
        })
        .collect::<Vec<_>>();
    let main_module = candidates
        .iter()
        .filter(|(_, module, _)| module.name == "Main")
        .collect::<Vec<_>>();
    let selected = if main_module.len() == 1 {
        Some(main_module[0].2.symbol)
    } else if main_module.len() > 1 {
        return Err(main_module
            .into_iter()
            .map(|(source, _, declaration)| ProgramDiagnostic {
                source: *source,
                diagnostic: diagnostic(
                    "P7 entry selection",
                    declaration.name_span,
                    "multiple `main` declarations exist in modules named `Main`",
                ),
            })
            .collect());
    } else if candidates.len() == 1 {
        Some(candidates[0].2.symbol)
    } else {
        None
    };
    if let Some(symbol) = selected {
        return Ok(Some(symbol));
    }
    if candidates.is_empty() {
        return Ok(None);
    }
    Err(candidates
        .into_iter()
        .map(|(source, _, declaration)| ProgramDiagnostic {
            source,
            diagnostic: diagnostic(
                "P7 entry selection",
                declaration.name_span,
                "program has multiple `main` declarations; define one `main` in `Main`",
            ),
        })
        .collect())
}

/// Resolves a program from a list of `(source_name, source_text)` pairs. Module
/// IDs equal each source's index in the list. This is the module-graph entry
/// point used to measure module, import, export, and name resolution.
pub fn resolve_program_sources(
    sources: &[(&str, &str)],
) -> Result<Vec<psrs_hir::Module>, Vec<ProgramDiagnostic>> {
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
    if !errors.is_empty() {
        return Err(errors);
    }
    match psrs_resolve::resolve_program(modules) {
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

/// Resolves and type checks a program and reports every diagnostic. Modules are
/// type checked in dependency order against the declared types of the values
/// they import, so a module can use a value another module defines.
pub fn check_program(sources: &[(&str, &str)]) -> Result<(), Vec<ProgramDiagnostic>> {
    typecheck_program_sources(sources).map(|_| ())
}

pub(crate) fn check_program_with_trusted_prefix(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<(), Vec<ProgramDiagnostic>> {
    typecheck_program_sources_with_trusted_prefix(sources, trusted_prefix).map(|_| ())
}

/// Resolves a program and type checks every module in dependency order,
/// returning the typed modules in input order.
pub fn typecheck_program_sources(
    sources: &[(&str, &str)],
) -> Result<Vec<psrs_thir::Module>, Vec<ProgramDiagnostic>> {
    typecheck_program_sources_with_trusted_prefix(sources, 0)
}

pub(crate) fn typecheck_program_sources_with_trusted_prefix(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<Vec<psrs_thir::Module>, Vec<ProgramDiagnostic>> {
    let modules = resolve_program_sources(sources)?;
    typecheck_program(modules, trusted_prefix)
}

fn typecheck_program(
    modules: Vec<psrs_hir::Module>,
    trusted_prefix: usize,
) -> Result<Vec<psrs_thir::Module>, Vec<ProgramDiagnostic>> {
    effects::check_run_effect_scope(&modules, trusted_prefix)?;
    let effect_type = modules
        .iter()
        .take(trusted_prefix)
        .find(|module| module.name == "Prelude")
        .and_then(|module| {
            module
                .types
                .iter()
                .find(|declaration| declaration.name == "Effect")
                .map(|declaration| declaration.id)
        });
    let known_types = modules
        .iter()
        .flat_map(|module| module.types.iter().cloned())
        .collect::<Vec<_>>();
    let exported = modules.iter().map(exported_signatures).collect::<Vec<_>>();
    let order = typecheck_order(&modules);
    let mut slots = modules.into_iter().map(Some).collect::<Vec<_>>();
    let mut typed = (0..slots.len()).map(|_| None).collect::<Vec<_>>();
    let mut errors = Vec::new();
    for index in order {
        let Some(module) = slots[index].take() else {
            continue;
        };
        let module = match psrs_desugar::desugar_module(module) {
            Ok(module) => module,
            Err(module_errors) => {
                for error in module_errors {
                    errors.push(ProgramDiagnostic {
                        source: index,
                        diagnostic: diagnostic("P4 desugar", error.span, error.message),
                    });
                }
                continue;
            }
        };
        let kind_errors = psrs_kind::check_module(&module);
        if !kind_errors.is_empty() {
            for error in kind_errors {
                errors.push(ProgramDiagnostic {
                    source: index,
                    diagnostic: coded_diagnostic(
                        "P5 kind check",
                        error.span,
                        Some(error.code),
                        error.message,
                    ),
                });
            }
            continue;
        }
        let imported = imported_signatures(&module, &exported);
        let trusted_effect_representation = index < trusted_prefix
            && matches!(
                module.name.as_str(),
                "Prelude"
                    | "WASI.Console"
                    | "WASI.Clock"
                    | "WASI.Random"
                    | "WASI.Exit"
                    | "WASI.Environment"
            );
        let check = psrs_typecheck::typecheck_module_with_imports_and_effect_context(
            module,
            &imported,
            effect_type,
            trusted_effect_representation,
            &known_types,
        );
        match check {
            Ok(module) => typed[index] = Some(module),
            Err(module_errors) => {
                for error in module_errors {
                    errors.push(ProgramDiagnostic {
                        source: index,
                        diagnostic: diagnostic("P5 typecheck", error.span, error.message()),
                    });
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(typed.into_iter().flatten().collect())
    } else {
        Err(errors)
    }
}

/// The declared value types a module exposes to its importers, taken from each
/// declaration's annotation. A declaration without an annotation is not
/// exported for cross-module use yet.
fn exported_signatures(module: &psrs_hir::Module) -> HashMap<psrs_hir::SymbolId, psrs_hir::Type> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| {
            declaration
                .signature
                .clone()
                .map(|signature| (declaration.symbol, signature))
        })
        .collect()
}

/// Resolves a module's imported symbols to their exporting declaration's
/// declared type.
fn imported_signatures(
    module: &psrs_hir::Module,
    exported: &[HashMap<psrs_hir::SymbolId, psrs_hir::Type>],
) -> HashMap<psrs_hir::SymbolId, psrs_hir::Type> {
    let mut imported = HashMap::new();
    for import in &module.imports {
        let Some(table) = exported.get(import.module.0 as usize) else {
            continue;
        };
        for symbol in &import.symbols {
            if let Some(ty) = table.get(&symbol.symbol) {
                imported.insert(symbol.symbol, ty.clone());
            }
        }
    }
    imported
}

/// Orders modules so every module follows the modules it imports.
fn typecheck_order(modules: &[psrs_hir::Module]) -> Vec<usize> {
    let dependencies = modules
        .iter()
        .map(|module| {
            module
                .imports
                .iter()
                .map(|import| import.module.0 as usize)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut order = Vec::with_capacity(modules.len());
    let mut visited = vec![false; modules.len()];
    for index in 0..modules.len() {
        visit(index, &dependencies, &mut visited, &mut order);
    }
    order
}

fn visit(index: usize, dependencies: &[Vec<usize>], visited: &mut [bool], order: &mut Vec<usize>) {
    if visited.get(index).copied().unwrap_or(true) {
        return;
    }
    visited[index] = true;
    for &dependency in dependencies.get(index).into_iter().flatten() {
        visit(dependency, dependencies, visited, order);
    }
    order.push(index);
}

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
