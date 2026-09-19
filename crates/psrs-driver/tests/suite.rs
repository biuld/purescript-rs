use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const CATEGORIES: [&str; 4] = ["passing", "failing", "warning", "layout"];

struct Mismatch {
    relative_path: String,
    oracle_parse_error: bool,
    ours_ok: bool,
    detail: Option<String>,
}

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

fn suite_limit() -> Option<usize> {
    std::env::var("PSRS_SUITE_LIMIT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|limit| *limit != 0)
}

fn suite_filter() -> Option<String> {
    std::env::var("PSRS_SUITE_FILTER")
        .ok()
        .filter(|value| !value.is_empty())
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

fn collected_files(category_dir: &Path, limit: Option<usize>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_purs_files(category_dir, &mut files);
    files.sort();
    if let Some(limit) = limit {
        files.truncate(limit);
    }
    files
}

fn collect_purs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_purs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "purs") {
            out.push(path);
        }
    }
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

fn is_ffi_excluded(path: &Path, text: &str) -> bool {
    text.contains("foreign import") || path.with_extension("js").is_file()
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
            if is_ffi_excluded(path, &text) {
                excluded += 1;
                continue;
            }
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
