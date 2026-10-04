//! Filesystem discovery for the transitive source graph.
//!
//! The standard library is read from `stdlib/lib`; user modules are discovered
//! from the filesystem. Given the entry files, this loader scans their
//! directories for `.purs` files, indexes them by declared module name, and
//! follows the `import` graph until it closes. Modules supplied by the
//! standard library are never searched. Resolution, duplicate-module checks,
//! and cycle checks remain in P3.

use crate::lower_source_to_ast;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// Source files that make up a corpus case: the main file, its optional
/// same-stem support directory, and the transitive imports the normal loader
/// discovers for those entry files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramCaseSources {
    pub own: Vec<(String, String)>,
    pub loaded: Vec<(String, String)>,
}

impl ProgramCaseSources {
    pub fn inputs(&self) -> Vec<(&str, &str)> {
        self.own
            .iter()
            .chain(&self.loaded)
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect()
    }
}

/// Assembles one suite case using the compiler's regular import loader.
/// Files beside a category root are independent; a same-stem subdirectory is
/// searched only when the case itself lives below that root.
pub fn load_program_case_sources(
    path: &Path,
    category_dir: &Path,
    main_text: &str,
) -> Result<ProgramCaseSources, String> {
    let mut own = vec![(path.to_string_lossy().into_owned(), main_text.to_owned())];
    let support_dir = path.with_extension("");
    if support_dir.is_dir() {
        let mut support = Vec::new();
        collect_purs_files(&support_dir, &mut support);
        support.sort();
        for support_path in support {
            let text = std::fs::read_to_string(&support_path)
                .map_err(|error| format!("{}: {error}", support_path.display()))?;
            own.push((support_path.to_string_lossy().into_owned(), text));
        }
    }

    let own_directory = path.parent() != Some(category_dir);
    let loaded = if own_directory {
        let entry_paths = own
            .iter()
            .skip(1)
            .map(|(path, _)| path.clone())
            .chain(std::iter::once(path.to_string_lossy().into_owned()))
            .collect::<Vec<_>>();
        load_program_files(&entry_paths)?
            .into_iter()
            .filter(|(loaded, _)| !own.iter().any(|(own, _)| own == loaded))
            .collect()
    } else {
        Vec::new()
    };
    Ok(ProgramCaseSources { own, loaded })
}

pub fn collect_purs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_purs_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "purs")
        {
            out.push(path);
        }
    }
}

/// Loads the entry files plus every transitively imported user module found on
/// disk. Returns `(path, text)` pairs; the order is discovery order, and P3
/// orders modules by dependency. An unreadable entry file is an error.
pub fn load_program_files(entry_paths: &[String]) -> Result<Vec<(String, String)>, String> {
    let directories = search_directories(entry_paths);
    let index = index_modules(&directories);
    let prelude_names = crate::prelude::module_names()?;

    let mut loaded = Vec::new();
    let mut loaded_names = HashSet::new();
    let mut seen_paths = HashSet::new();
    let mut queue = VecDeque::from_iter(entry_paths.iter().cloned());

    while let Some(path) = queue.pop_front() {
        if !seen_paths.insert(path.clone()) {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))?;
        // A parse failure is reported by the normal P0-P2 pipeline; the loader
        // only uses the AST to follow imports.
        let module = lower_source_to_ast(&path, &text).ok();
        if let Some(module) = &module {
            loaded_names.insert(module.name.text.clone());
        }
        loaded.push((path, text));
        let Some(module) = module else {
            continue;
        };
        for import in &module.imports {
            let name = &import.module.text;
            if prelude_names.contains(name) || loaded_names.contains(name) {
                continue;
            }
            if let Some(found) = index.get(name)
                && !seen_paths.contains(found)
            {
                queue.push_back(found.clone());
            }
        }
    }
    Ok(loaded)
}

/// The directories searched for imported modules: the entry files' parent
/// directories, in first-seen order.
fn search_directories(entry_paths: &[String]) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    for path in entry_paths {
        let parent = Path::new(path).parent().unwrap_or(Path::new(""));
        let directory = if parent.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            parent.to_path_buf()
        };
        if !directories.contains(&directory) {
            directories.push(directory);
        }
    }
    directories
}

/// Maps each parseable `.purs` file in the search directories to its declared
/// module name. The first file wins when two files declare the same module; P3
/// still rejects a duplicate only when both are actually loaded.
fn index_modules(directories: &[PathBuf]) -> HashMap<String, String> {
    let mut index = HashMap::new();
    for directory in directories {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("purs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let name = path.to_string_lossy().into_owned();
            if let Ok(module) = lower_source_to_ast(&name, &text) {
                index.entry(module.name.text).or_insert(name);
            }
        }
    }
    index
}
