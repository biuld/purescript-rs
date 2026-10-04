use super::{BundleContext, CompilerRevision, Snapshot, SourceInput, WorkerResponse};
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

pub(super) fn write_bundle(
    bundle: &Path,
    sources: &[SourceInput],
    response: &WorkerResponse,
    case: &str,
    input_set_complete: bool,
    context: &BundleContext,
) -> Result<(), String> {
    let _ = fs::remove_dir_all(bundle);
    fs::create_dir_all(bundle.join("inputs"))
        .map_err(|error| format!("{}: {error}", bundle.display()))?;
    let mut paths = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let file = format!("inputs/{index:04}-{}.purs", safe_name(&source.name));
        fs::write(bundle.join(&file), &source.text)
            .map_err(|error| format!("{}: {error}", bundle.join(&file).display()))?;
        paths.push(file);
    }
    write_dumps(bundle, response)?;
    let command = replay_command(&paths, &context.executable);
    fs::write(
        bundle.join("replay.sh"),
        format!("#!/bin/sh\nset -eu\ncd \"$(dirname \"$0\")\"\n{command}\n"),
    )
    .map_err(|error| error.to_string())?;
    let metadata = serde_json::json!({
        "case": case,
        "input_names": sources.iter().map(|source| &source.name).collect::<Vec<_>>(),
        "replay_argv": std::iter::once(context.executable.clone())
            .chain(std::iter::once("build".to_owned()))
            .chain(paths.iter().cloned())
            .chain(["-o".into(), "output.wasm".into()])
            .collect::<Vec<_>>(),
        "diagnostics": &response.diagnostics,
        "input_fingerprint": &response.input_fingerprint,
        "compiler": &context.compiler,
        "trusted_stdlib_fingerprint": &context.trusted_stdlib_fingerprint,
        "input_set_complete": input_set_complete,
    });
    fs::write(
        bundle.join("case.json"),
        serde_json::to_vec_pretty(&metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("{}: {error}", bundle.display()))
}

pub(super) fn write_empty_bundle(
    bundle: &Path,
    sources: &[SourceInput],
    case: &str,
    reason: &str,
    stderr: &str,
    input_set_complete: bool,
    context: &BundleContext,
) -> Result<(), String> {
    let empty = WorkerResponse {
        passed: false,
        input_fingerprint: fingerprint_sources(sources),
        sources: sources.to_vec(),
        input_set_complete,
        elapsed_ms: 0,
        diagnostics: Vec::new(),
        core_stage: None,
        core: None,
        cc_stage: None,
        cc: None,
        mir_stage: None,
        mir: None,
    };
    write_bundle(bundle, sources, &empty, case, input_set_complete, context)?;
    fs::write(bundle.join("worker-failure.txt"), reason).map_err(|error| error.to_string())?;
    if !stderr.is_empty() {
        fs::write(bundle.join("worker-stderr.log"), stderr).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_dumps(bundle: &Path, response: &WorkerResponse) -> Result<(), String> {
    for (name, stage, text) in [
        ("core", &response.core_stage, &response.core),
        ("cc", &response.cc_stage, &response.cc),
        ("mir", &response.mir_stage, &response.mir),
    ] {
        if let Some(text) = text {
            fs::write(bundle.join(format!("{name}.debug")), text)
                .map_err(|error| error.to_string())?;
            fs::write(
                bundle.join(format!("{name}.stage")),
                stage.as_deref().unwrap_or("unknown"),
            )
            .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn replay_command(paths: &[String], executable: &str) -> String {
    let args = paths
        .iter()
        .map(|path| shell_quote(path))
        .collect::<Vec<_>>()
        .join(" ");
    let args = format!("build {args} -o output.wasm");
    format!(
        "if [ -z \"${{PSRS_BIN:-}}\" ]; then PSRS_BIN={}; fi\n\"$PSRS_BIN\" {args}",
        shell_quote(executable)
    )
}

pub(super) fn trusted_stdlib_fingerprint() -> Result<String, String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib/lib");
    let trusted_path = root.join("trusted");
    let trusted = fs::read_to_string(&trusted_path)
        .map_err(|error| format!("{}: {error}", trusted_path.display()))?;
    let mut bytes = trusted.as_bytes().to_vec();
    for name in trusted
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let path = root.join(format!("{}.purs", name.replace('.', "/")));
        let text = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        bytes.extend_from_slice(path.to_string_lossy().as_bytes());
        bytes.extend_from_slice(&text);
    }
    Ok(hash_bytes(&bytes))
}

pub(super) fn compiler_revision() -> CompilerRevision {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let head = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let diff = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["diff", "HEAD", "--binary"])
        .output()
        .ok()
        .map(|output| output.stdout)
        .unwrap_or_default();
    let status = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|output| output.stdout)
        .unwrap_or_default();
    let dirty = !status.is_empty();
    let mut bytes = diff;
    bytes.extend_from_slice(&status);
    if let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .output()
    {
        for path in output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
        {
            bytes.extend_from_slice(path);
            if let Ok(content) = fs::read(root.join(String::from_utf8_lossy(path).as_ref())) {
                bytes.extend_from_slice(&content);
            }
        }
    }
    let binary_fingerprint = env::current_exe()
        .ok()
        .and_then(|path| fs::read(path).ok())
        .map(|binary| hash_bytes(&binary))
        .unwrap_or_else(|| "unavailable".into());
    CompilerRevision {
        head,
        dirty,
        working_tree_fingerprint: hash_bytes(&bytes),
        binary_fingerprint,
    }
}

pub(super) fn corpus_root() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("PURESCRIPT_REPO") {
        let root = PathBuf::from(path).join("tests/purs");
        return if root.is_dir() {
            Ok(root)
        } else {
            Err(format!(
                "{}: corpus directory does not exist",
                root.display()
            ))
        };
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream");
    if root.is_dir() {
        Ok(root)
    } else {
        Err(format!("{}: vendored corpus not found", root.display()))
    }
}

pub(super) fn is_ffi_excluded(path: &Path, text: &str) -> bool {
    text.contains("foreign import") || path.with_extension("js").is_file()
}

pub(super) fn fingerprint_sources(sources: &[SourceInput]) -> String {
    let mut bytes = Vec::new();
    for source in sources {
        bytes.extend_from_slice(source.name.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(source.text.as_bytes());
        bytes.push(0xff);
    }
    hash_bytes(&bytes)
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

pub(super) fn safe_name(value: &str) -> String {
    let name = Path::new(value)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let cleaned = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if cleaned.is_empty() {
        "case".into()
    } else {
        cleaned
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| error.to_string())
    }
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, bytes).map_err(|error| format!("{}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| format!("{}: {error}", path.display()))
}

pub(super) fn write_snapshot(path: &Path, snapshot: &Snapshot) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(|error| error.to_string())?;
    atomic_write(path, &bytes)
}

pub(super) fn read_json<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{path}: invalid JSON: {error}"))
}

pub(super) fn first_line(text: &str) -> &str {
    text.lines()
        .next()
        .unwrap_or("worker crashed without a message")
}

pub(super) fn usage() -> String {
    "usage: psrs diagnose <file.purs> [--out report.json] [--timeout SECONDS]\n       psrs diagnose --corpus passing [--filter TEXT] [--limit N] [--out report.json] [--timeout SECONDS]\n       psrs diagnose --compare OLD.json NEW.json".into()
}
