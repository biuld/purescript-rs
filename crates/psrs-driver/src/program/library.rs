//! Prepends the on-disk standard library as the trusted source prefix.

use super::{
    check_program_kinds_lenient, check_program_lenient, check_program_types_lenient, diagnostic,
};
use crate::prelude;
use crate::{Artifact, CompilationReport, DiagnosticOrigin, ProgramDiagnostic};

/// Compiles user sources together with the on-disk standard library.
///
/// Library modules occupy the trusted prefix. A diagnostic reports the caller's
/// source as [`DiagnosticOrigin::Source`], [`DiagnosticOrigin::Library`] when it
/// belongs to a standard-library module, and [`DiagnosticOrigin::Program`] when
/// it is not attributable to a single source.
pub fn compile_program_sources_with_prelude(
    sources: &[(&str, &str)],
) -> Result<Artifact, Vec<ProgramDiagnostic>> {
    let report = compile_library(sources, false, false);
    match report.artifact {
        Some(artifact) => Ok(artifact),
        None => Err(report.diagnostics),
    }
}

/// Compiles with the package and retains the last successful IR stages.
pub fn compile_program_sources_with_prelude_report(sources: &[(&str, &str)]) -> CompilationReport {
    compile_library(sources, true, false)
}

/// Compiles with the package and records actual frontend/backend pass events.
pub fn compile_program_sources_with_prelude_diagnosis(
    sources: &[(&str, &str)],
    capture_dumps: bool,
) -> CompilationReport {
    compile_library(sources, capture_dumps, true)
}

fn compile_library(
    sources: &[(&str, &str)],
    capture_dumps: bool,
    trace: bool,
) -> CompilationReport {
    let (all_sources, trusted_prefix) = match prelude::prepend_for_command(sources) {
        Ok(sources) => sources,
        Err(message) => return package_failure(message),
    };
    let info = match prelude::standard_library_info() {
        Ok(info) => info,
        Err(message) => return package_failure(message),
    };
    let mut report = super::compilation::compile_attempt(
        &all_sources,
        trusted_prefix,
        capture_dumps,
        trace,
        info.command_runner.as_ref(),
    );
    report.diagnostics = shift(report.diagnostics, trusted_prefix);
    report
}

fn package_failure(message: String) -> CompilationReport {
    CompilationReport::failed(
        vec![ProgramDiagnostic {
            source: DiagnosticOrigin::Library,
            diagnostic: diagnostic("stdlib", psrs_span::TextRange::default(), message),
        }],
        Default::default(),
    )
}

/// Resolves user sources leniently together with the on-disk standard library.
///
/// A lenient check tolerates imports whose modules are not provided at all, so a
/// corpus case measured against a missing `Prelude` reports an `UnknownName` for
/// every name it imported from it. With the library on the module path, a case is
/// measured against the library this compiler actually ships, which is what the
/// M2 row counts. See [`compile_program_sources_with_prelude`] for the origin a
/// diagnostic reports.
pub fn check_program_lenient_with_prelude(
    sources: &[(&str, &str)],
) -> Result<(), Vec<ProgramDiagnostic>> {
    let (all_sources, trusted_prefix) = with_prelude(sources)?;
    check_program_lenient(&all_sources).map_err(|errors| shift(errors, trusted_prefix))
}

/// Kind-checks user sources leniently together with the on-disk standard
/// library. See [`check_program_lenient_with_prelude`].
pub fn check_program_kinds_lenient_with_prelude(
    sources: &[(&str, &str)],
) -> Result<(), Vec<ProgramDiagnostic>> {
    let (all_sources, trusted_prefix) = with_prelude(sources)?;
    check_program_kinds_lenient(&all_sources).map_err(|errors| shift(errors, trusted_prefix))
}

/// Type checks user sources leniently together with the on-disk standard
/// library. See [`check_program_lenient_with_prelude`].
pub fn check_program_types_lenient_with_prelude(
    sources: &[(&str, &str)],
) -> Result<(), Vec<ProgramDiagnostic>> {
    let (all_sources, trusted_prefix) = with_prelude(sources)?;
    check_program_types_lenient(&all_sources).map_err(|errors| shift(errors, trusted_prefix))
}

fn with_prelude<'a>(
    sources: &[(&'a str, &'a str)],
) -> Result<crate::prelude::PrefixedSources<'a>, Vec<ProgramDiagnostic>> {
    prelude::prepend(sources).map_err(|message| {
        vec![ProgramDiagnostic {
            source: DiagnosticOrigin::Library,
            diagnostic: diagnostic("stdlib", psrs_span::TextRange::default(), message),
        }]
    })
}

/// Rebases a diagnostic's source index from the prefixed program back onto the
/// caller's own source list.
///
/// A source below `trusted_prefix` is a standard-library module, which is not in
/// the caller's list, so it becomes [`DiagnosticOrigin::Library`]. Subtracting
/// with saturation instead would map every library diagnostic onto the caller's
/// first source, and a caller that attributes diagnostics by source would read
/// it as its own. A diagnostic that was never attributed to a source keeps its
/// own origin rather than acquiring an index it never had.
fn shift(errors: Vec<ProgramDiagnostic>, trusted_prefix: usize) -> Vec<ProgramDiagnostic> {
    errors
        .into_iter()
        .map(|error| ProgramDiagnostic {
            source: match error.source {
                DiagnosticOrigin::Source(source) if source < trusted_prefix => {
                    DiagnosticOrigin::Library
                }
                DiagnosticOrigin::Source(source) => {
                    DiagnosticOrigin::Source(source - trusted_prefix)
                }
                origin => origin,
            },
            diagnostic: error.diagnostic,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREFIX: usize = 12;

    fn program_diagnostic(source: DiagnosticOrigin) -> ProgramDiagnostic {
        ProgramDiagnostic {
            source,
            diagnostic: diagnostic("P3 resolve", psrs_span::TextRange::default(), "a message"),
        }
    }

    #[test]
    fn a_trusted_source_is_not_reported_as_the_callers_first_source() {
        // The trusted prefix is the standard library, so any source below it
        // belongs to a library module. Saturating subtraction would report it
        // against source zero, which is the first file the caller passed.
        let shifted = shift(
            vec![program_diagnostic(DiagnosticOrigin::Source(0))],
            PREFIX,
        );
        assert_eq!(shifted[0].source, DiagnosticOrigin::Library);
    }

    #[test]
    fn every_trusted_source_is_reported_as_the_library() {
        for source in 0..PREFIX {
            let shifted = shift(
                vec![program_diagnostic(DiagnosticOrigin::Source(source))],
                PREFIX,
            );
            assert_eq!(
                shifted[0].source,
                DiagnosticOrigin::Library,
                "library source {source} must not name a caller source"
            );
        }
    }

    #[test]
    fn a_caller_source_keeps_its_offset_from_the_trusted_prefix() {
        let shifted = shift(
            vec![
                program_diagnostic(DiagnosticOrigin::Source(PREFIX)),
                program_diagnostic(DiagnosticOrigin::Source(PREFIX + 2)),
            ],
            PREFIX,
        );
        assert_eq!(shifted[0].source, DiagnosticOrigin::Source(0));
        assert_eq!(shifted[1].source, DiagnosticOrigin::Source(2));
    }

    #[test]
    fn a_program_wide_diagnostic_keeps_its_origin() {
        // The backend reports a component with no entry point against the
        // program, with no module to name. That is the caller's program failing,
        // not the library, so it must not be filed as a library diagnostic and
        // must not acquire the caller's first source.
        let shifted = shift(
            vec![
                program_diagnostic(DiagnosticOrigin::Program),
                program_diagnostic(DiagnosticOrigin::Library),
            ],
            PREFIX,
        );
        assert_eq!(shifted[0].source, DiagnosticOrigin::Program);
        assert_eq!(shifted[1].source, DiagnosticOrigin::Library);
    }
}
