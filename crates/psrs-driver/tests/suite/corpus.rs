//! Corpus discovery and classification shared by the scoreboards.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The vendored corpus, or `$PURESCRIPT_REPO/tests/purs` when it is set.
pub fn corpus_root() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PURESCRIPT_REPO") {
        let base = PathBuf::from(path).join("tests/purs");
        if base.is_dir() {
            return Some(base);
        }
        return None;
    }
    let vendored = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream");
    vendored.is_dir().then_some(vendored)
}

/// Caps a scoreboard run for a quick iteration. `0` and an unset variable both
/// mean "no limit".
pub fn suite_limit() -> Option<usize> {
    std::env::var("PSRS_SUITE_LIMIT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|limit| *limit != 0)
}

/// Restricts a scoreboard run to the paths containing this substring.
pub fn suite_filter() -> Option<String> {
    std::env::var("PSRS_SUITE_FILTER")
        .ok()
        .filter(|value| !value.is_empty())
}

pub fn is_ffi_excluded(path: &Path, text: &str) -> bool {
    text.contains("foreign import") || path.with_extension("js").is_file()
}

/// Every `.purs` file under `dir`, sorted, so a report is deterministic.
pub fn collected_files(dir: &Path, limit: Option<usize>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_purs_files(dir, &mut files);
    files.sort();
    if let Some(limit) = limit {
        files.truncate(limit);
    }
    files
}

pub fn collect_purs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_purs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "purs") {
            out.push(path);
        }
    }
}

/// The main source plus the modules in a sibling directory named after the file
/// stem, which is how the corpus supplies a case's support modules.
pub fn support_sources(path: &Path, text: &str) -> Vec<(String, String)> {
    let mut sources = vec![(path.to_string_lossy().into_owned(), text.to_owned())];
    let support_dir = path.with_extension("");
    if support_dir.is_dir() {
        let mut files = Vec::new();
        collect_purs_files(&support_dir, &mut files);
        files.sort();
        for support in files {
            if let Ok(text) = std::fs::read_to_string(&support) {
                sources.push((support.to_string_lossy().into_owned(), text));
            }
        }
    }
    sources
}

/// The `errorCode`s a `failing` case declares in its `@shouldFailWith`
/// annotations, which are the corpus's own ground truth and need no `purs`.
pub fn annotation_codes(text: &str) -> Vec<String> {
    text.lines()
        .take_while(|line| line.trim_start().starts_with("--"))
        .filter_map(|line| line.split("@shouldFailWith").nth(1))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

pub fn diagnostic_codes(errors: &[psrs_driver::ProgramDiagnostic]) -> HashSet<&str> {
    errors
        .iter()
        .filter_map(|error| error.diagnostic.code)
        .collect()
}
