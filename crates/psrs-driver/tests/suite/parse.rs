//! The L1 parse-agreement scoreboard.
//!
//! L1 asks one question per corpus file: does our parser accept exactly the files
//! the oracle rejects as `ErrorParsingModule`? `PSRS_ORACLE=annotations` decides
//! the `failing` column from the file's own annotation instead of from an
//! installed `purs`, because a missing support library can stop the installed
//! compiler from reaching a file's body.

use super::corpus::{collected_files, corpus_root, is_ffi_excluded, suite_filter, suite_limit};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const CATEGORIES: [&str; 4] = ["passing", "failing", "warning", "layout"];

struct Mismatch {
    relative_path: String,
    oracle_parse_error: bool,
    ours_ok: bool,
    detail: Option<String>,
}

fn purs_available() -> bool {
    Command::new("purs")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// When set, `failing` files are classified by their `@shouldFailWith`
/// annotation instead of invoking `purs`. The annotation is the corpus's own
/// ground truth and does not depend on support libraries being installed or on
/// the installed `purs` matching the checkout.
fn use_annotation_oracle() -> bool {
    std::env::var("PSRS_ORACLE").is_ok_and(|value| value == "annotations")
}

fn annotation_parse_error(text: &str) -> bool {
    text.lines()
        .take_while(|line| line.trim_start().starts_with("--"))
        .any(|line| line.contains("@shouldFailWith") && line.contains("ErrorParsingModule"))
}

fn oracle_parse_error(path: &Path) -> bool {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let output_dir = std::env::temp_dir().join(format!("psrs-suite-{}-{id}", std::process::id()));
    let output = Command::new("purs")
        .arg("compile")
        .arg("--json-errors")
        .arg(path)
        .arg("-o")
        .arg(&output_dir)
        .output()
        .expect("failed to run purs");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined.contains("\"errorCode\":\"ErrorParsingModule\"")
}

#[test]
#[ignore = "requires purs and a PureScript checkout"]
fn l1_parse_scoreboard_against_purs() {
    let Some(repo) = corpus_root() else {
        eprintln!("skipping: vendored corpus not found and PURESCRIPT_REPO is unset");
        return;
    };
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    let limit = suite_limit();
    let filter = suite_filter();
    let mut mismatches = Vec::new();
    let mut grand_agree = 0usize;
    let mut grand_total = 0usize;
    let mut grand_excluded = 0usize;

    for category in CATEGORIES {
        let category_dir = repo.join(category);
        if !category_dir.is_dir() {
            eprintln!("skipping category `{category}`: directory not found");
            continue;
        }
        let files = collected_files(&category_dir, limit);
        let mut agree = 0usize;
        let mut excluded = 0usize;
        let mut considered = 0usize;

        for path in &files {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let relative_path = path
                .strip_prefix(&repo)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            if let Some(filter) = filter.as_deref()
                && !relative_path.contains(filter)
            {
                continue;
            }
            if is_ffi_excluded(path, &text) {
                excluded += 1;
                continue;
            }
            considered += 1;
            let oracle_parse_error = if use_annotation_oracle() && category == "failing" {
                annotation_parse_error(&text)
            } else {
                oracle_parse_error(path)
            };
            let result = psrs_driver::parse_source(&path.to_string_lossy(), &text);
            let ours_ok = result.is_ok();
            let expected_ok = !oracle_parse_error;
            if ours_ok == expected_ok {
                agree += 1;
            } else {
                let detail = result.err().map(|errors| {
                    errors
                        .first()
                        .map(|error| {
                            let offset = error.span.start as usize;
                            let line = text[..offset.min(text.len())].matches('\n').count() + 1;
                            format!("{}:{}: {}", line, offset, error.message)
                        })
                        .unwrap_or_default()
                });
                mismatches.push(Mismatch {
                    relative_path,
                    oracle_parse_error,
                    ours_ok,
                    detail,
                });
            }
        }

        grand_agree += agree;
        grand_total += considered;
        grand_excluded += excluded;
        println!("{category}: {agree}/{considered} parse agreement, {excluded} excluded");
    }

    println!("total: {grand_agree}/{grand_total} parse agreement, {grand_excluded} excluded");
    if !mismatches.is_empty() {
        println!("mismatches ({}):", mismatches.len());
        for mismatch in &mismatches {
            let reason = if mismatch.oracle_parse_error {
                "oracle said parse error, ours parsed"
            } else {
                "oracle parsed, ours failed"
            };
            let detail = mismatch
                .detail
                .as_deref()
                .map(|detail| format!(" ({detail})"))
                .unwrap_or_default();
            println!(
                "  {}: {reason} (ours_ok={}){detail}",
                mismatch.relative_path, mismatch.ours_ok
            );
        }
    }
}
