//! The on-disk PureScript standard library.
//!
//! Sources come from the locked external `psrs-stdlib` package at runtime.
//! `PSRS_STDLIB_ROOT` explicitly selects an unlocked development package.

mod package;
pub use package::{CommandRunner, StandardLibraryInfo};

use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub(crate) type PrefixedSources<'a> = (Vec<(&'a str, &'a str)>, usize);

pub(crate) struct ModuleSource {
    /// Filesystem path, used as the source name in the compile pipeline.
    pub path: String,
    pub module_name: String,
    pub text: String,
    imports: Vec<String>,
}

struct Library {
    info: StandardLibraryInfo,
    modules: Vec<ModuleSource>,
}

pub fn standard_library_info() -> Result<StandardLibraryInfo, String> {
    Ok(load()?.info.clone())
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

/// Prepends the trusted standard-library modules reachable from `sources`.
pub(crate) fn prepend<'a>(
    user_sources: &[(&'a str, &'a str)],
) -> Result<PrefixedSources<'a>, String> {
    prepend_selected(user_sources, false)
}

pub(crate) fn prepend_for_command<'a>(
    user_sources: &[(&'a str, &'a str)],
) -> Result<PrefixedSources<'a>, String> {
    prepend_selected(user_sources, true)
}

fn prepend_selected<'a>(
    user_sources: &[(&'a str, &'a str)],
    command: bool,
) -> Result<PrefixedSources<'a>, String> {
    let library = sources()?;
    let by_name = library
        .iter()
        .enumerate()
        .map(|(index, module)| (module.module_name.as_str(), index))
        .collect::<std::collections::HashMap<_, _>>();
    let mut needed = HashSet::new();
    let mut pending = VecDeque::new();
    if command && let Some(runner) = &load()?.info.command_runner {
        if !by_name.contains_key(runner.module.as_str()) {
            return Err(format!(
                "configured command runner module `{}` is absent from the trusted package inventory",
                runner.module
            ));
        }
        pending.push_back(runner.module.clone());
    }
    for (name, text) in user_sources {
        if let Ok(parsed) = crate::lower_source_to_ast(name, text) {
            pending.extend(parsed.imports.into_iter().map(|import| import.module.text));
        }
    }
    while let Some(name) = pending.pop_front() {
        let Some(&index) = by_name.get(name.as_str()) else {
            continue;
        };
        if needed.insert(index) {
            pending.extend(library[index].imports.iter().cloned());
        }
    }

    let selected = library
        .iter()
        .enumerate()
        .filter(|(index, _)| needed.contains(index))
        .map(|(_, module)| module)
        .collect::<Vec<_>>();
    let trusted_prefix = selected.len();
    let mut all = Vec::with_capacity(trusted_prefix + user_sources.len());
    for module in selected {
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
    let info = package::select()?;
    let lib = info.root.join("lib");
    let names = read_trusted_names(&lib.join("trusted"))?;
    let mut modules = Vec::with_capacity(names.len());
    for name in names {
        // A compiler-provided module (for example `Safe.Coerce`) resolves
        // through its primitive interface. Loading the vendored file as well
        // would shadow that interface with the official `unsafeCoerce` body,
        // which the project cannot compile.
        if psrs_resolve::compiler_provided_module(&name) {
            continue;
        }
        let path = module_file(&lib, &name);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let display = path.to_string_lossy().into_owned();
        let parsed = crate::lower_source_to_ast(&display, &text).map_err(|errors| {
            let message = errors
                .first()
                .map(|error| error.message.as_str())
                .unwrap_or("could not parse the standard-library module");
            format!(
                "{display}:{}: {message}",
                errors.first().map(|error| error.span.start).unwrap_or(0)
            )
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
            imports: parsed
                .imports
                .into_iter()
                .map(|import| import.module.text)
                .collect(),
        });
    }
    if package::fingerprint(&info.root)? != info.source_fingerprint {
        return Err("standard-library package changed during loading".into());
    }
    Ok(Library { modules, info })
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
