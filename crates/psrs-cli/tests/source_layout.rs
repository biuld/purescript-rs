use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn rust_sources_follow_repository_layout_rules() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repository_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("psrs-cli manifest should be under the repository crates directory");
    let crates_dir = repository_root.join("crates");
    let mut violations = Vec::new();

    scan_directory(&crates_dir, &mut violations);

    assert!(
        violations.is_empty(),
        "Rust source layout violations:\n{}",
        violations.join("\n")
    );
}

fn scan_directory(directory: &Path, violations: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(directory) else {
        violations.push(format!("{}: could not read directory", directory.display()));
        return;
    };

    let mut rust_files = Vec::new();
    let mut module_directories = BTreeSet::new();
    let mut child_directories = Vec::new();

    for entry in entries {
        let Ok(entry) = entry else {
            violations.push(format!(
                "{}: could not read directory entry",
                directory.display()
            ));
            continue;
        };
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            violations.push(format!("{}: could not inspect entry", path.display()));
            continue;
        };

        if file_type.is_dir() {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                module_directories.insert(name.to_owned());
            }
            child_directories.push(path);
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            rust_files.push(path);
        }
    }
    rust_files.sort();

    let mut module_roots = module_directories.clone();
    for path in &rust_files {
        if path.file_name().is_some_and(|name| name != "mod.rs")
            && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
        {
            module_roots.insert(stem.to_owned());
        }
    }

    for path in &rust_files {
        let display = path.display().to_string();
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) => {
                violations.push(format!("{display}: could not read source: {error}"));
                continue;
            }
        };
        let line_count = contents.lines().count();
        if line_count > 500 {
            violations.push(format!(
                "{display}: {line_count} lines (limit 500); split into a module directory with mod.rs and focused submodules"
            ));
        }

        if path.file_name().is_some_and(|name| name != "mod.rs")
            && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
        {
            if module_directories.contains(stem) {
                violations.push(format!(
                    "{display}: file and `{}` directory share the same module name; move this module root to `{}/mod.rs`",
                    path.with_extension("").display(),
                    path.with_extension("").display()
                ));
            }

            // Only enforce grouping when a sibling file or directory gives an exact
            // module root. This avoids guessing that an arbitrary filename prefix is
            // a module name when the corresponding module does not exist.
            let matching_roots = module_roots
                .iter()
                .filter(|root| stem.starts_with(&format!("{root}_")))
                .collect::<Vec<_>>();
            if let Some(root) = matching_roots.iter().max_by_key(|root| root.len()) {
                let root_file = directory.join(format!("{root}.rs"));
                let root_directory = directory.join(root);
                let root_migration = if root_file.is_file() && !root_directory.is_dir() {
                    format!("; move `{root}.rs` to `{root}/mod.rs` to create the module directory")
                } else {
                    String::new()
                };
                violations.push(format!(
                    "{display}: flat `{root}_*.rs` sibling belongs under `{root}/`; move it to `{root}/{stem_after_root}.rs`{root_migration}",
                    stem_after_root = &stem[root.len() + 1..]
                ));
            }
        }
    }

    child_directories.sort();
    for child in child_directories {
        scan_directory(&child, violations);
    }
}
