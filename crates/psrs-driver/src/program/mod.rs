//! Multi-module program pipelines: resolution, dependency-ordered type
//! checking against imported signatures, Core lowering, and linking.

use super::{
    Artifact, DiagnosticOrigin, ProgramDiagnostic, ProgramWarning, Warning, backend_warnings,
    coded_diagnostic, diagnostic, lower_source_to_ast,
};

pub use lenient::{
    check_program_kinds_lenient, check_program_lenient, check_program_types_lenient,
};
pub use library::{
    check_program_kinds_lenient_with_prelude, check_program_lenient_with_prelude,
    check_program_types_lenient_with_prelude, compile_program_sources_with_prelude,
};
use std::collections::{HashMap, HashSet};

mod effects;
mod graph;
mod lenient;
mod library;
mod reports;

pub(crate) use reports::typecheck_warnings;
pub use reports::{
    check_program, check_program_with_warnings, typecheck_program_sources,
    typecheck_program_sources_with_warnings,
};

fn desugar_diagnostic(error: psrs_desugar::DesugarError) -> super::Diagnostic {
    let kind = (error.message == "boolean literal pattern survived P4 desugaring")
        .then_some(psrs_backend::BackendErrorKind::UnsupportedSource);
    super::Diagnostic {
        stage: "P4 desugar",
        span: error.span,
        message: error.message.into(),
        code: None,
        kind,
    }
}

use graph::{imported_instance_declarations, module_dependencies, module_table, typecheck_order};

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
    let (core, source_warnings) =
        lower_program_to_core_with_trusted_prefix_and_warnings(sources, trusted_prefix)?;
    let output = psrs_backend::compile(core).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| ProgramDiagnostic {
                source: error.module.map_or(DiagnosticOrigin::Program, |module| {
                    DiagnosticOrigin::Source(module.0 as usize)
                }),
                diagnostic: diagnostic(error.pass, error.span, error.message),
            })
            .collect::<Vec<_>>()
    })?;
    let mut warnings = typecheck_warnings(source_warnings, trusted_prefix);
    warnings.extend(backend_warnings(output.warnings, trusted_prefix));
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

#[cfg(test)]
pub(crate) fn lower_program_to_core_with_trusted_prefix(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<psrs_core::Module, Vec<ProgramDiagnostic>> {
    lower_program_to_core_with_trusted_prefix_and_warnings(sources, trusted_prefix)
        .map(|(core, _warnings)| core)
}

pub(crate) fn lower_program_to_core_with_trusted_prefix_and_warnings(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<(psrs_core::Module, Vec<ProgramWarning>), Vec<ProgramDiagnostic>> {
    let (typed, warnings) =
        typecheck_program_sources_with_trusted_prefix_and_warnings(sources, trusted_prefix)?;
    let entry = select_entry(&typed)?;
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
        linked.entry = Some(entry);
        psrs_core::prune_unreachable(&mut linked, entry);
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
    Ok((linked, warnings))
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
                source: DiagnosticOrigin::Source(*source),
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
            source: DiagnosticOrigin::Source(source),
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
                        source: DiagnosticOrigin::Source(index),
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
                source: DiagnosticOrigin::Source(error.module),
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

pub(crate) fn check_program_with_trusted_prefix_and_warnings(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<Vec<ProgramWarning>, Vec<ProgramDiagnostic>> {
    typecheck_program_sources_with_trusted_prefix_and_warnings(sources, trusted_prefix)
        .map(|(_modules, warnings)| warnings)
}

#[cfg(test)]
pub(crate) fn typecheck_program_sources_with_trusted_prefix(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<Vec<psrs_thir::Module>, Vec<ProgramDiagnostic>> {
    let modules = resolve_program_sources(sources)?;
    typecheck_program_with_warnings(modules, trusted_prefix).map(|(modules, _warnings)| modules)
}

fn typecheck_program_sources_with_trusted_prefix_and_warnings(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<(Vec<psrs_thir::Module>, Vec<ProgramWarning>), Vec<ProgramDiagnostic>> {
    let modules = resolve_program_sources(sources)?;
    typecheck_program_with_warnings(modules, trusted_prefix)
}

pub(super) fn typecheck_program_with_warnings(
    modules: Vec<psrs_hir::Module>,
    trusted_prefix: usize,
) -> Result<(Vec<psrs_thir::Module>, Vec<ProgramWarning>), Vec<ProgramDiagnostic>> {
    let true_symbols = psrs_desugar::true_symbols(&modules);
    let mut desugared = Vec::with_capacity(modules.len());
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for (source, module) in modules.into_iter().enumerate() {
        match psrs_desugar::desugar_module_with_true_symbols(module, &true_symbols) {
            Ok(module) => desugared.push(module),
            Err(module_errors) => {
                for error in module_errors {
                    errors.push(ProgramDiagnostic {
                        source: DiagnosticOrigin::Source(source),
                        diagnostic: desugar_diagnostic(error),
                    });
                }
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let modules = desugared;
    effects::check_run_effect_scope(&modules, trusted_prefix)?;
    // Kind checking runs once for the whole program and produces the one
    // environment every module's type check consumes. Its diagnostics are
    // reported here rather than dropped: a conflict in module A is reported
    // against A's declaration even though B's use exposed it.
    let (checked_kinds, kind_diagnostics) = psrs_kind::check_program(&modules);
    let failed_kind_modules = kind_failure_modules(&kind_diagnostics);
    for error in kind_diagnostics {
        errors.push(ProgramDiagnostic {
            // A diagnostic about a compiler-provided declaration names no source
            // module, so it belongs to the program rather than to an index in
            // the caller's source list.
            source: if error.origin == psrs_hir::ModuleId::INTRINSICS {
                DiagnosticOrigin::Program
            } else {
                DiagnosticOrigin::Source(error.origin.0 as usize)
            },
            diagnostic: coded_diagnostic(
                "P5 kind check",
                error.span,
                Some(error.code),
                error.message,
            ),
        });
    }
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
    let mut known_types = modules
        .iter()
        .flat_map(|module| module.types.iter().cloned())
        .collect::<Vec<_>>();
    known_types.extend(
        psrs_hir::primitive_type_declarations()
            .into_iter()
            .map(|(_, declaration)| declaration),
    );
    // Instance declarations are threaded per module, like values: a module can
    // only select an instance declared in a module it imports, directly or
    // transitively.
    let instance_sets = modules
        .iter()
        .map(|module| module.instances.clone())
        .collect::<Vec<_>>();
    let exported_instances = modules
        .iter()
        .map(|module| {
            module.exports.as_ref().map(|exports| {
                exports
                    .instances
                    .iter()
                    .map(|instance| instance.symbol)
                    .collect()
            })
        })
        .collect::<Vec<_>>();
    let dependencies = module_dependencies(&modules);
    // A re-exported symbol is declared in the module that owns it, so the
    // signature table is global: a module that imports an exported symbol finds
    // its declared type even when it imported it through an umbrella module.
    // Foreign imports contribute the same way; their type lives on the
    // external rather than on a value declaration.
    let signatures = declared_signatures(&modules);
    let order = typecheck_order(&dependencies);
    let module_names = modules
        .iter()
        .map(|module| (module.id, module.name.clone()))
        .collect::<HashMap<_, _>>();
    let mut slots = modules.into_iter().map(Some).collect::<Vec<_>>();
    let mut typed = (0..slots.len()).map(|_| None).collect::<Vec<_>>();
    for index in order {
        let Some(module) = slots[index].take() else {
            continue;
        };
        if failed_kind_modules.contains(&module.id) {
            continue;
        }
        let imported = imported_signatures(&module, &signatures);
        let imported_instances = imported_instance_declarations(
            &dependencies,
            index,
            &instance_sets,
            &exported_instances,
        );
        let trusted_effect_representation = index < trusted_prefix
            && matches!(
                module.name.as_str(),
                "Prelude"
                    | "WASI.Resource"
                    | "WASI.IO"
                    | "WASI.Clock"
                    | "WASI.Random"
                    | "WASI.Console"
                    | "WASI.Process"
                    | "WASI.FileSystem"
                    | "WASI.Network"
                    | "WASI"
            );
        let check =
            psrs_typecheck::typecheck_module_with_checked_kinds_and_module_names_and_warnings(
                module,
                &imported,
                effect_type,
                trusted_effect_representation,
                psrs_typecheck::TypecheckContext {
                    known_types: &known_types,
                    imported_instances: &imported_instances,
                    module_names: &module_names,
                    checked_kinds: &checked_kinds,
                },
            );
        match check {
            Ok(output) => {
                typed[index] = Some(output.module);
                warnings.extend(output.warnings.into_iter().map(|warning| ProgramWarning {
                    source: DiagnosticOrigin::Source(index),
                    diagnostic: coded_diagnostic(
                        "P5 typecheck",
                        warning.span,
                        Some(warning.error_code()),
                        warning.message,
                    ),
                }));
            }
            Err(module_errors) => {
                for error in module_errors {
                    errors.push(ProgramDiagnostic {
                        source: DiagnosticOrigin::Source(index),
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
    }
    if errors.is_empty() {
        Ok((typed.into_iter().flatten().collect(), warnings))
    } else {
        Err(errors)
    }
}

/// The modules a program-level kind check reported a diagnostic against.
///
/// A module whose kind failed is not type checked: term inference consumes the
/// checked kind environment, so an errored module would otherwise be elaborated
/// against a kind that no module ever checked. The set is keyed by the module
/// that *declares* the offending type, which is how a cross-module conflict
/// stops the declaration rather than the use that exposed it.
fn kind_failure_modules(diagnostics: &[psrs_kind::KindDiagnostic]) -> HashSet<psrs_hir::ModuleId> {
    diagnostics.iter().map(|error| error.origin).collect()
}

/// The declared type of every value and external in the program, keyed by the
/// symbol that declares it. A re-exported symbol keeps the signature of the
/// declaration that owns it, so the table is global rather than per module.
fn declared_signatures(
    modules: &[psrs_hir::Module],
) -> HashMap<psrs_hir::SymbolId, psrs_hir::Type> {
    modules
        .iter()
        .flat_map(|module| {
            let declarations = module.declarations.iter().filter_map(|declaration| {
                declaration
                    .signature
                    .clone()
                    .map(|signature| (declaration.symbol, signature))
            });
            let externals = module.externals.iter().filter_map(|external| {
                external
                    .signature
                    .clone()
                    .map(|signature| (external.symbol, signature))
            });
            declarations.chain(externals)
        })
        .collect()
}

/// Resolves a module's imported symbols to their declared type from the global
/// declaration table. A symbol re-exported by an umbrella module keeps the
/// signature of the declaration that owns it.
fn imported_signatures(
    module: &psrs_hir::Module,
    signatures: &HashMap<psrs_hir::SymbolId, psrs_hir::Type>,
) -> HashMap<psrs_hir::SymbolId, psrs_hir::Type> {
    let mut imported = HashMap::new();
    for import in &module.imports {
        for symbol in &import.symbols {
            if let Some(ty) = signatures.get(&symbol.symbol) {
                imported.insert(symbol.symbol, ty.clone());
            }
        }
    }
    imported
}
