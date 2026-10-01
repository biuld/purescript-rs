//! Prepends the on-disk standard library as the trusted source prefix.

use super::{
    Artifact, ProgramDiagnostic, check_program_kinds_lenient, check_program_lenient,
    compile_program_sources_with_trusted_prefix, diagnostic,
};
use crate::prelude;

/// Compiles user sources together with the on-disk standard library.
///
/// Library modules occupy the trusted prefix. Diagnostic source indices refer
/// to `sources`, not that prefix, so callers can render errors against the
/// files they passed.
pub fn compile_program_sources_with_prelude(
    sources: &[(&str, &str)],
) -> Result<Artifact, Vec<ProgramDiagnostic>> {
    let (all_sources, trusted_prefix) = with_prelude(sources)?;
    compile_program_sources_with_trusted_prefix(&all_sources, trusted_prefix)
        .map_err(|errors| shift(errors, trusted_prefix))
}

/// Resolves user sources leniently together with the on-disk standard library.
///
/// A lenient check tolerates imports whose modules are not provided at all, so a
/// corpus case measured against a missing `Prelude` reports an `UnknownName` for
/// every name it imported from it. With the library on the module path, a case is
/// measured against the library this compiler actually ships, which is what the
/// M2 row counts.
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

fn with_prelude<'a>(
    sources: &[(&'a str, &'a str)],
) -> Result<crate::prelude::PrefixedSources<'a>, Vec<ProgramDiagnostic>> {
    prelude::prepend(sources).map_err(|message| {
        vec![ProgramDiagnostic {
            source: 0,
            diagnostic: diagnostic("stdlib", psrs_span::TextRange::default(), message),
        }]
    })
}

fn shift(errors: Vec<ProgramDiagnostic>, trusted_prefix: usize) -> Vec<ProgramDiagnostic> {
    errors
        .into_iter()
        .map(|mut error| {
            error.source = error.source.saturating_sub(trusted_prefix);
            error
        })
        .collect()
}
