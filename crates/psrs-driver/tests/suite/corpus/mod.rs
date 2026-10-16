//! Corpus discovery and classification shared by the scoreboards.

use std::collections::{BTreeMap, HashSet};
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
    psrs_driver::collect_purs_files(dir, out);
}

/// One corpus case, assembled the way the compiler loads a program.
pub struct Case {
    /// The case's own modules: the main source plus a sibling directory named
    /// after the file stem, which is how the corpus ships support modules.
    pub own: Vec<(String, String)>,
    /// Modules the compiler's loader discovered on disk to satisfy the case's
    /// imports, such as `passing/Import/M1.purs` for `passing/Import/M2.purs`.
    pub loaded: Vec<(String, String)>,
    /// The directory the case lives in, which holds its dependencies only when
    /// the case has a directory of its own.
    pub directory: PathBuf,
    /// Whether the case has a directory of its own. A case in a category root
    /// shares its directory with hundreds of independent cases, so nothing there
    /// is treated as a dependency.
    pub own_directory: bool,
}

impl Case {
    pub fn inputs(&self) -> Vec<(&str, &str)> {
        self.own
            .iter()
            .chain(&self.loaded)
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect()
    }

    /// Whether a diagnostic belongs to the case itself rather than to a module the
    /// loader added, to the standard library, or to the program as a whole. A
    /// sibling module is not part of the case, so its diagnostics must not decide
    /// whether the case agrees.
    ///
    /// A library diagnostic names a source the case never passed, so it can never
    /// be the case's own. A program-wide diagnostic is not the case's own source
    /// either, but it is still the case's program failing, so [`blocker`] counts
    /// it as a stage rather than as a harness assembly failure.
    pub fn is_own(&self, source: psrs_driver::DiagnosticOrigin) -> bool {
        source
            .source_index()
            .is_some_and(|source| source < self.own.len())
    }

    pub fn own_diagnostics<'a>(
        &self,
        errors: &'a [psrs_driver::ProgramDiagnostic],
    ) -> Vec<&'a psrs_driver::ProgramDiagnostic> {
        errors
            .iter()
            .filter(|error| self.is_own(error.source))
            .collect()
    }
}

/// Assembles a case: its own modules plus the transitive imports the driver's
/// loader finds on disk, such as `passing/Import/M1.purs` for
/// `passing/Import/M2.purs`. The loader indexes each search directory by
/// declared module name and follows the import graph, so an unrelated file in a
/// case directory is never pulled in.
///
/// A case in a category root is never given siblings. `tests/upstream/failing`
/// alone contains three files that declare `module M1` — `ExportExplicit.purs`,
/// `DuplicateModule.purs`, and the support module `ExportExplicit1/M1.purs` — so
/// resolving a name against the root would silently substitute another case's
/// module for the one under test. A case that has its own subdirectory has
/// dependencies there, and that subdirectory is searched first, so its support
/// modules win over anything found beside the case.
pub fn load_case(path: &Path, category_dir: &Path, text: &str) -> Case {
    let own_directory = path.parent() != Some(category_dir);
    let sources = psrs_driver::load_program_case_sources(path, category_dir, text)
        .expect("a previously read suite case and its support files remain readable");
    Case {
        own: sources.own,
        loaded: sources.loaded,
        directory: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
        own_directory,
    }
}

/// Why a case is blocked, split so the phase that recovers it is visible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocker {
    /// An import no provided module satisfies. Usually a library module the
    /// compiler does not provide; see [`unusable_siblings`] for the case where
    /// the module is on disk but this compiler cannot lower it.
    MissingLibrary,
    /// The case's own modules were accepted and a module the loader added was
    /// not, so the harness could not assemble the case as a program.
    HarnessLoading,
    /// A case-owned source was rejected at this stage.
    Stage(String),
}

/// Classifies a failed case. `errors` is the full diagnostic list for the
/// assembled program; only the case's own sources decide the category, because a
/// sibling module is not part of the case being measured. A diagnostic the
/// backend attributes to no module is still this case's program failing, so it
/// decides the stage rather than the harness.
pub fn blocker(case: &Case, errors: &[psrs_driver::ProgramDiagnostic]) -> Blocker {
    let own = case.own_diagnostics(errors);
    let first = own.first().copied().or_else(|| {
        errors
            .iter()
            .find(|error| error.source == psrs_driver::DiagnosticOrigin::Program)
    });
    let Some(first) = first else {
        return Blocker::HarnessLoading;
    };
    if own
        .iter()
        .any(|error| error.diagnostic.code == Some("ModuleNotFound"))
    {
        return Blocker::MissingLibrary;
    }
    Blocker::Stage(first.diagnostic.stage.to_owned())
}

/// Siblings beside a case that could not be loaded, for a case that is blocked on
/// a missing module anyway.
///
/// A sibling the loader did not take is normally just a sibling the case does not
/// import, so this only reports the ones that matter: the case asks for a module
/// the loader could not supply, and a file that could have supplied it sits right
/// there. The loader indexes a directory by parsing each file, so a file that
/// parses but does not lower is never a candidate —
/// `passing/RedefinedFixity/M1.purs` declares an `infixr ... as $` alias, so the
/// two cases importing it report `ModuleNotFound` for a module that exists. Such
/// a blocker is the frontend's, not the library's, so it is called out separately
/// from the `missing library module` count.
pub fn unusable_siblings(case: &Case, errors: &[psrs_driver::ProgramDiagnostic]) -> Vec<String> {
    let missing_module = errors
        .iter()
        .any(|error| error.diagnostic.code == Some("ModuleNotFound") && case.is_own(error.source));
    if !missing_module || !case.own_directory {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&case.directory) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "purs")
        })
        .collect();
    paths.sort();
    let used = case
        .own
        .iter()
        .chain(&case.loaded)
        .map(|(path, _)| std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path)))
        .collect::<HashSet<PathBuf>>();
    paths
        .iter()
        .filter(|path| {
            !used.contains(&std::fs::canonicalize(path).unwrap_or_else(|_| (*path).clone()))
        })
        .map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

/// Prints a `category: count` histogram in descending count order.
pub fn print_histogram(title: &str, counts: &BTreeMap<String, usize>) {
    println!("{title}:");
    let mut ranked: Vec<_> = counts.iter().collect();
    ranked.sort_by_key(|(label, count)| std::cmp::Reverse((**count, (*label).clone())));
    for (label, count) in ranked {
        println!("  {count} x {label}");
    }
}

/// Blocked `passing` cases, counted by the phase that recovers them: a missing
/// library module is Phase 3, a harness-loading failure is the harness's, and
/// every other stage is Phase 2 or the backend's.
#[derive(Default)]
pub struct Blockers {
    pub total: usize,
    pub missing_library: usize,
    pub harness_loading: usize,
    pub stages: BTreeMap<String, usize>,
}

impl Blockers {
    pub fn record(&mut self, case: &Case, errors: &[psrs_driver::ProgramDiagnostic]) {
        self.total += 1;
        match blocker(case, errors) {
            Blocker::MissingLibrary => self.missing_library += 1,
            Blocker::HarnessLoading => self.harness_loading += 1,
            Blocker::Stage(stage) => *self.stages.entry(stage).or_default() += 1,
        }
    }

    pub fn print(&self) {
        println!("blocked cases: {}", self.total);
        println!("  missing library module: {}", self.missing_library);
        println!("  harness loading: {}", self.harness_loading);
        print_histogram("  other blockers, by stage", &self.stages);
    }
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

pub fn diagnostic_codes<'a>(
    errors: impl IntoIterator<Item = &'a psrs_driver::ProgramDiagnostic>,
) -> HashSet<&'a str> {
    errors
        .into_iter()
        .filter_map(|error| error.diagnostic.code)
        .collect()
}

#[cfg(test)]
mod tests;
