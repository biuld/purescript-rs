//! The on-disk PureScript standard library.
//!
//! Sources are read from `stdlib/lib` at runtime (not embedded). `lib/trusted`
//! names the modules that form the trusted prefix, in order. The directory is
//! resolved from the crate location first, so `cargo test` and the CLI do not
//! depend on the process current directory.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub(crate) type PrefixedSources<'a> = (Vec<(&'a str, &'a str)>, usize);

pub(crate) struct ModuleSource {
    /// Filesystem path, used as the source name in the compile pipeline.
    pub path: String,
    pub module_name: String,
    pub text: String,
}

struct Library {
    modules: Vec<ModuleSource>,
}

pub(crate) fn sources() -> Result<&'static [ModuleSource], String> {
    Ok(&load()?.modules)
}

pub(crate) fn module_names() -> Result<HashSet<String>, String> {
    Ok(sources()?
        .iter()
        .map(|module| module.module_name.clone())
        .collect())
}

/// Prepends the trusted standard-library modules to `sources`.
pub(crate) fn prepend<'a>(
    user_sources: &[(&'a str, &'a str)],
) -> Result<PrefixedSources<'a>, String> {
    let library = sources()?;
    let trusted_prefix = library.len();
    let mut all = Vec::with_capacity(trusted_prefix + user_sources.len());
    for module in library {
        all.push((module.path.as_str(), module.text.as_str()));
    }
    all.extend_from_slice(user_sources);
    Ok((all, trusted_prefix))
}

fn load() -> Result<&'static Library, String> {
    static LIBRARY: OnceLock<Library> = OnceLock::new();
    if let Some(library) = LIBRARY.get() {
        return Ok(library);
    }
    let loaded = read_library()?;
    Ok(LIBRARY.get_or_init(|| loaded))
}

fn read_library() -> Result<Library, String> {
    let root = find_stdlib_root()?;
    let lib = root.join("lib");
    let names = read_trusted_names(&lib.join("trusted"))?;
    let mut modules = Vec::with_capacity(names.len());
    for name in names {
        let path = module_file(&lib, &name);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let display = path.to_string_lossy().into_owned();
        let parsed = crate::lower_source_to_ast(&display, &text).map_err(|errors| {
            let message = errors
                .first()
                .map(|error| error.message.as_str())
                .unwrap_or("could not parse the standard-library module");
            format!("{display}: {message}")
        })?;
        if parsed.name.text != name {
            return Err(format!(
                "{display}: declares module `{}` but the trusted list names `{name}`",
                parsed.name.text
            ));
        }
        modules.push(ModuleSource {
            path: display,
            module_name: name,
            text,
        });
    }
    Ok(Library { modules })
}

fn read_trusted_names(path: &Path) -> Result<Vec<String>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut names = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.split_whitespace().nth(1).is_some() {
            return Err(format!(
                "{}:{}: expected one module name per line",
                path.display(),
                number + 1
            ));
        }
        names.push(line.to_owned());
    }
    if names.is_empty() {
        return Err(format!(
            "{}: the trusted standard-library list is empty",
            path.display()
        ));
    }
    Ok(names)
}

fn module_file(lib: &Path, module_name: &str) -> PathBuf {
    let mut path = lib.to_path_buf();
    let mut parts = module_name.split('.');
    let Some(file_stem) = parts.next_back() else {
        return path;
    };
    for part in parts {
        path.push(part);
    }
    path.push(format!("{file_stem}.purs"));
    path
}

fn find_stdlib_root() -> Result<PathBuf, String> {
    let mut tried = Vec::new();
    for candidate in stdlib_candidates() {
        if is_stdlib_root(&candidate) {
            return candidate
                .canonicalize()
                .map_err(|error| format!("{}: {error}", candidate.display()));
        }
        if !tried.iter().any(|existing| existing == &candidate) {
            tried.push(candidate);
        }
    }
    Err(format!(
        "could not find the standard library (expected stdlib/lib/trusted and stdlib/lib/Prelude.purs); looked in {}",
        tried
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn is_stdlib_root(path: &Path) -> bool {
    path.join("lib/trusted").is_file() && path.join("lib/Prelude.purs").is_file()
}

fn stdlib_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Anchored at this crate: `crates/psrs-driver` -> repository `stdlib/`.
    candidates.push(crate_dir.join("../../stdlib"));
    push_ancestor_stdlibs(&crate_dir, &mut candidates);
    if let Ok(current) = std::env::current_dir() {
        push_ancestor_stdlibs(&current, &mut candidates);
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        push_ancestor_stdlibs(parent, &mut candidates);
    }
    candidates
}

fn push_ancestor_stdlibs(start: &Path, candidates: &mut Vec<PathBuf>) {
    let mut directory = start.to_path_buf();
    loop {
        candidates.push(directory.join("stdlib"));
        if !directory.pop() {
            break;
        }
    }
}
