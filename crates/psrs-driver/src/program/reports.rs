use super::{
    DiagnosticOrigin, ProgramDiagnostic, ProgramWarning, Warning, resolve_program_sources,
    typecheck_program_with_warnings,
};

/// Resolves and type checks a program and reports every diagnostic. Modules are
/// type checked in dependency order against the declared types of their values.
pub fn check_program(sources: &[(&str, &str)]) -> Result<(), Vec<ProgramDiagnostic>> {
    typecheck_program_sources(sources).map(|_| ())
}

/// Resolves and type checks a program, returning warnings from successful
/// module checks separately from errors.
pub fn check_program_with_warnings(
    sources: &[(&str, &str)],
) -> Result<Vec<ProgramWarning>, Vec<ProgramDiagnostic>> {
    typecheck_program_sources_with_warnings(sources).map(|(_modules, warnings)| warnings)
}

/// Resolves a program and returns its checked modules in input order.
pub fn typecheck_program_sources(
    sources: &[(&str, &str)],
) -> Result<Vec<psrs_thir::Module>, Vec<ProgramDiagnostic>> {
    typecheck_program_sources_with_warnings(sources).map(|(modules, _warnings)| modules)
}

/// Resolves and type checks a program, returning checked modules and warnings
/// attributed to their source positions.
pub fn typecheck_program_sources_with_warnings(
    sources: &[(&str, &str)],
) -> Result<(Vec<psrs_thir::Module>, Vec<ProgramWarning>), Vec<ProgramDiagnostic>> {
    let modules = resolve_program_sources(sources)?;
    typecheck_program_with_warnings(modules, 0)
}

/// Drops diagnostics for prepended trusted modules and rebases warnings onto
/// the caller's source list, matching backend warning attribution.
pub(crate) fn typecheck_warnings(
    warnings: Vec<ProgramWarning>,
    hidden_sources: usize,
) -> Vec<Warning> {
    warnings
        .into_iter()
        .filter_map(|warning| {
            let DiagnosticOrigin::Source(source) = warning.source else {
                return None;
            };
            (source >= hidden_sources).then(|| Warning {
                source: source - hidden_sources,
                diagnostic: warning.diagnostic,
            })
        })
        .collect()
}
