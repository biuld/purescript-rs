//! Runtime units follow upstream TestUtils.getTestFiles: a category-root file
//! and its same-stem support directory form one program. Frontend scoreboards
//! deliberately keep their separate per-file discovery contract.

use super::{Case, collected_files, is_ffi_excluded};
use std::path::{Path, PathBuf};

/// Select program roots before applying a diagnostic filter and limit. Never
/// infer test membership from successful parsing or the presence of `main`:
/// malformed roots and missing entries must still fail the runtime board.
pub fn collected_programs(
    directory: &Path,
    filter: Option<&str>,
    limit: Option<usize>,
) -> Vec<PathBuf> {
    let mut roots = collected_files(directory, None)
        .into_iter()
        .filter(|path| path.parent() == Some(directory))
        .filter(|path| filter.is_none_or(|filter| path.to_string_lossy().contains(filter)))
        .collect::<Vec<_>>();
    if let Some(limit) = limit {
        roots.truncate(limit);
    }
    roots
}

/// A source FFI dependency excludes the whole program, including when the
/// declaration lives in a support module rather than the entry source.
pub fn program_is_ffi_excluded(case: &Case) -> bool {
    case.own
        .iter()
        .chain(&case.loaded)
        .any(|(path, text)| is_ffi_excluded(Path::new(path), text))
}
