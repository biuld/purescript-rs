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
