//! Prepends the on-disk standard library as the trusted source prefix.

use super::{Artifact, ProgramDiagnostic, compile_program_sources_with_trusted_prefix, diagnostic};
use crate::prelude;

/// Compiles user sources together with the on-disk standard library.
///
/// Library modules occupy the trusted prefix. Diagnostic source indices refer
/// to `sources`, not that prefix, so callers can render errors against the
/// files they passed.
pub fn compile_program_sources_with_prelude(
    sources: &[(&str, &str)],
) -> Result<Artifact, Vec<ProgramDiagnostic>> {
    let (all_sources, trusted_prefix) = prelude::prepend(sources).map_err(|message| {
        vec![ProgramDiagnostic {
            source: 0,
            diagnostic: diagnostic("stdlib", psrs_span::TextRange::default(), message),
        }]
    })?;
    compile_program_sources_with_trusted_prefix(&all_sources, trusted_prefix).map_err(|errors| {
        errors
            .into_iter()
            .map(|mut error| {
                error.source = error.source.saturating_sub(trusted_prefix);
                error
            })
            .collect()
    })
}
