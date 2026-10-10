//! Package selection and portable content identity for the external stdlib.
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct StandardLibraryInfo {
    pub root: PathBuf,
    pub source_fingerprint: String,
    /// Absent for an explicit development override.
    pub locked_revision: Option<String>,
    pub command_runner: Option<CommandRunner>,
}

/// An ordinary package-owned declaration used to adapt command entries.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRunner {
    pub module: String,
    pub function: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Lock {
    schema_version: u32,
    path: PathBuf,
    revision: String,
    source_fingerprint: String,
}

pub(super) fn select() -> Result<StandardLibraryInfo, String> {
    if let Some(root) = std::env::var_os("PSRS_STDLIB_ROOT") {
        return inspect(Path::new(&root), None);
    }
    let lock_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../stdlib.lock.json");
    select_locked(&lock_path)
}

fn select_locked(lock_path: &Path) -> Result<StandardLibraryInfo, String> {
    let lock: Lock = serde_json::from_slice(&read(lock_path)?)
        .map_err(|error| format!("{}: {error}", lock_path.display()))?;
    if lock.schema_version != 1
        || lock.revision.len() != 40
        || !lock.revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "{}: unsupported or invalid stdlib lock",
            lock_path.display()
        ));
    }
    let info = inspect(
        &lock_path.parent().unwrap().join(lock.path),
        Some(lock.revision),
    )?;
    if info.source_fingerprint != lock.source_fingerprint {
        return Err(format!(
            "{}: standard-library content differs from the lock; use PSRS_STDLIB_ROOT for an explicit development override",
            info.root.display()
        ));
    }
    Ok(info)
}

fn inspect(root: &Path, revision: Option<String>) -> Result<StandardLibraryInfo, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("{}: {error}", root.display()))?;
    let manifest: serde_json::Value = serde_json::from_slice(&read(&root.join("manifest.json"))?)
        .map_err(|error| format!("{}: {error}", root.display()))?;
    // Protocol versions are compiler contracts, not assumptions inferred from source names.
    for (pointer, expected) in [
        ("/schema_version", serde_json::json!(1)),
        ("/name", serde_json::json!("psrs-stdlib")),
        ("/source_root", serde_json::json!("lib")),
        ("/trusted_modules", serde_json::json!("lib/trusted")),
        ("/upstream_lock", serde_json::json!("upstream-lock.json")),
        (
            "/compiler_contract/primitive_binding_protocol",
            serde_json::json!(1),
        ),
        (
            "/compiler_contract/string_semantics",
            serde_json::json!("unicode-scalar-utf8"),
        ),
        (
            "/compiler_contract/int_semantics",
            serde_json::json!("signed-i32"),
        ),
    ] {
        if manifest.pointer(pointer) != Some(&expected) {
            return Err(format!(
                "{}: unsupported stdlib manifest field {pointer}",
                root.display()
            ));
        }
    }
    let command_runner = manifest
        .pointer("/compiler_contract/command_runner")
        .map(|value| {
            serde_json::from_value::<CommandRunner>(value.clone())
                .map_err(|error| format!("{}: invalid command_runner: {error}", root.display()))
        })
        .transpose()?;
    if command_runner.as_ref().is_some_and(|runner| {
        runner.module.is_empty()
            || runner.function.is_empty()
            || runner.module.trim() != runner.module
            || runner.function.trim() != runner.function
    }) {
        return Err(format!(
            "{}: command_runner requires nonempty module and function names",
            root.display()
        ));
    }
    // Required metadata must exist even in development mode.
    read(&root.join("lib/trusted"))?;
    read(&root.join("lib/Prelude.purs"))?;
    read(&root.join("upstream-lock.json"))?;
    let source_fingerprint = fingerprint(&root)?;
    Ok(StandardLibraryInfo {
        root,
        source_fingerprint,
        locked_revision: revision,
        command_runner,
    })
}

/// FNV-1a content identity, versioned and independent of checkout paths.
/// Covers all files under lib/ and conformance/, plus both package manifests.
/// This is a reproducibility identifier, not a cryptographic integrity claim.
pub(super) fn fingerprint(root: &Path) -> Result<String, String> {
    let mut paths = vec![
        PathBuf::from("manifest.json"),
        PathBuf::from("upstream-lock.json"),
    ];
    collect(root, Path::new("lib"), &mut paths)?;
    collect(root, Path::new("conformance"), &mut paths)?;
    paths.sort();
    let mut hash = 0xcbf29ce484222325_u64;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    feed(b"psrs-stdlib-content-v1\0");
    for path in paths {
        let bytes = read(&root.join(&path))?;
        let name = path
            .components()
            .map(|part| {
                part.as_os_str()
                    .to_str()
                    .ok_or("stdlib contains a non-UTF-8 path")
            })
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        feed(name.as_bytes());
        feed(&[0]);
        feed(&(bytes.len() as u64).to_le_bytes());
        feed(&bytes);
    }
    Ok(format!("fnv1a64-v1:{hash:016x}"))
}

fn collect(root: &Path, relative: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let path = root.join(relative);
    if std::fs::symlink_metadata(&path)
        .map_err(|error| error.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err(format!(
            "{}: stdlib package symlinks are unsupported",
            path.display()
        ));
    }
    for entry in std::fs::read_dir(&path).map_err(|error| format!("{}: {error}", path.display()))? {
        let entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        let relative = relative.join(entry.file_name());
        if kind.is_symlink() {
            return Err(format!(
                "{}: stdlib package symlinks are unsupported",
                entry.path().display()
            ));
        } else if kind.is_dir() {
            collect(root, &relative, paths)?;
        } else if kind.is_file() {
            paths.push(relative);
        } else {
            return Err(format!(
                "{}: expected a regular package file",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    if !std::fs::symlink_metadata(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .file_type()
        .is_file()
    {
        return Err(format!(
            "{}: expected a regular package file",
            path.display()
        ));
    }
    std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "psrs-package-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("lib")).unwrap();
            std::fs::create_dir_all(root.join("conformance")).unwrap();
            let manifest = serde_json::json!({
                "schema_version": 1, "name": "psrs-stdlib", "source_root": "lib",
                "trusted_modules": "lib/trusted", "upstream_lock": "upstream-lock.json",
                "compiler_contract": {"primitive_binding_protocol": 1,
                    "string_semantics": "unicode-scalar-utf8", "int_semantics": "signed-i32"}
            });
            std::fs::write(
                root.join("manifest.json"),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
            std::fs::write(root.join("upstream-lock.json"), "{}").unwrap();
            std::fs::write(root.join("lib/trusted"), "Prelude\n").unwrap();
            std::fs::write(root.join("lib/Prelude.purs"), "module Prelude where\n").unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn package_identity_is_portable_and_covers_cases_and_extra_modules() {
        let a = Fixture::new();
        let b = Fixture::new();
        let original = fingerprint(&a.0).unwrap();
        assert_eq!(original, fingerprint(&b.0).unwrap());
        std::fs::write(b.0.join("lib/Unused.purs"), "module Unused where").unwrap();
        assert_ne!(original, fingerprint(&b.0).unwrap());
        std::fs::remove_file(b.0.join("lib/Unused.purs")).unwrap();
        std::fs::write(b.0.join("conformance/cases.json"), "[]").unwrap();
        assert_ne!(original, fingerprint(&b.0).unwrap());
    }

    #[test]
    fn package_without_required_protocol_is_rejected() {
        let fixture = Fixture::new();
        std::fs::write(fixture.0.join("manifest.json"), b"{}").unwrap();
        let error = inspect(&fixture.0, None).unwrap_err();
        assert!(
            error.contains("unsupported stdlib manifest field"),
            "{error}"
        );
    }
    #[test]
    fn locked_package_rejects_dirty_content_and_invalid_dependency_paths() {
        let fixture = Fixture::new();
        let lock_path = fixture.0.join("consumer.lock.json");
        let lock = serde_json::json!({"schema_version": 1, "path": fixture.0,
            "revision": "0000000000000000000000000000000000000000",
            "source_fingerprint": fingerprint(&fixture.0).unwrap()});
        std::fs::write(&lock_path, serde_json::to_vec(&lock).unwrap()).unwrap();
        let selected = select_locked(&lock_path).unwrap();
        assert_eq!(selected.root, fixture.0.canonicalize().unwrap());
        assert!(selected.locked_revision.is_some());
        std::fs::write(fixture.0.join("lib/Prelude.purs"), "changed").unwrap();
        assert!(
            select_locked(&lock_path)
                .unwrap_err()
                .contains("differs from the lock")
        );
        std::fs::remove_dir_all(fixture.0.join("lib")).unwrap();
        assert!(select_locked(&lock_path).is_err());
    }
    #[test]
    fn manifest_command_runner_is_explicit_and_rejects_malformed_configuration() {
        let fixture = Fixture::new();
        assert!(inspect(&fixture.0, None).unwrap().command_runner.is_none());
        let path = fixture.0.join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for value in [
            serde_json::Value::Null,
            serde_json::json!({"module":"Runner"}),
            serde_json::json!({"module":"Runner", "function":""}),
            serde_json::json!({"module":"Runner", "function":"run", "extra":true}),
        ] {
            manifest["compiler_contract"]["command_runner"] = value;
            std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
            assert!(
                inspect(&fixture.0, None)
                    .unwrap_err()
                    .contains("command_runner")
            );
        }
        manifest["compiler_contract"]["command_runner"] =
            serde_json::json!({"module":"Runner", "function":"run"});
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert_eq!(
            inspect(&fixture.0, None).unwrap().command_runner,
            Some(CommandRunner {
                module: "Runner".into(),
                function: "run".into()
            })
        );
    }
}
