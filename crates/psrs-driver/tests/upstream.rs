use std::path::{Path, PathBuf};
use std::process::Command;

fn corpus_root() -> Option<PathBuf> {
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

fn purs_available() -> bool {
    Command::new("purs")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn purs_accepts(source: &Path) -> bool {
    let output_dir = std::env::temp_dir().join(format!("psrs-purs-out-{}", std::process::id()));
    Command::new("purs")
        .arg("compile")
        .arg(source)
        .arg("-o")
        .arg(&output_dir)
        .output()
        .expect("failed to run purs")
        .status
        .success()
}

#[test]
fn differential_against_purs_on_selected_cases() {
    let Some(repo) = corpus_root() else {
        eprintln!("skipping: vendored corpus not found and PURESCRIPT_REPO is unset");
        return;
    };
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    let mut failures = Vec::new();

    let reject_cases = ["failing/InfiniteType.purs", "failing/IntOutOfRange.purs"];
    for relative in reject_cases {
        let path = repo.join(relative);
        if !path.is_file() {
            failures.push(format!("missing upstream case `{relative}`"));
            continue;
        }
        if purs_accepts(&path) {
            failures.push(format!("`{relative}`: purs accepted a failing test"));
        }
        let text = std::fs::read_to_string(&path).expect("read upstream test");
        if psrs_driver::check_source(&path.to_string_lossy(), &text).is_ok() {
            failures.push(format!("`{relative}`: psrs accepted but should reject"));
        }
    }

    let accept_cases: [(&str, &str); 2] = [
        (
            "identity.purs",
            "module Main where\nidentity :: forall a. a -> a\nidentity x = x\n",
        ),
        (
            "const.purs",
            "module Main where\nconst :: forall a b. a -> b -> a\nconst x y = x\n",
        ),
    ];
    for (name, source) in accept_cases {
        let path = std::env::temp_dir().join(format!("psrs-{}-{name}", std::process::id()));
        std::fs::write(&path, source).expect("write fixture");
        if !purs_accepts(&path) {
            failures.push(format!("`{name}`: purs rejected an accept case"));
        }
        if let Err(errors) = psrs_driver::check_source(name, source) {
            failures.push(format!("`{name}`: psrs rejected: {errors:?}"));
        }
        let _ = std::fs::remove_file(&path);
    }

    assert!(
        failures.is_empty(),
        "upstream differential failures:\n{}",
        failures.join("\n")
    );
}
