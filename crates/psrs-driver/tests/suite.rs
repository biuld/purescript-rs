use std::collections::{BTreeMap, HashSet};
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

/// The `errorCode`s the M2 milestone is accountable for, from D-04.
const M2_CODES: [&str; 18] = [
    "UnknownName",
    "DeclConflict",
    "TransitiveExportError",
    "ExportConflict",
    "ScopeConflict",
    "TransitiveDctorExportError",
    "OrphanTypeDeclaration",
    "OrphanKindDeclaration",
    "OverlappingNamesInLet",
    "OverlappingArgNames",
    "DuplicateValueDeclaration",
    "DuplicateModule",
    "CycleInModules",
    "UnknownImport",
    "UnknownImportDataConstructor",
    "UnknownExport",
    "UnknownExportDataConstructor",
    "ModuleNotFound",
];

fn annotation_codes(text: &str) -> Vec<String> {
    text.lines()
        .take_while(|line| line.trim_start().starts_with("--"))
        .filter_map(|line| line.split("@shouldFailWith").nth(1))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

fn diagnostic_codes(errors: &[psrs_driver::ProgramDiagnostic]) -> HashSet<&str> {
    errors
        .iter()
        .filter_map(|error| error.diagnostic.code)
        .collect()
}

/// The `errorCode`s the M3 milestone is accountable for, from D-04.
const M3_CODES: [&str; 7] = [
    "KindsDoNotUnify",
    "PartiallyAppliedSynonym",
    "CycleInTypeSynonym",
    "CycleInKindDeclaration",
    "UndefinedTypeVariable",
    "InfiniteKind",
    "UnsupportedTypeInKind",
];

/// Measures M3 kind coverage using the corpus's `@shouldFailWith` annotations.
/// It resolves each `failing` file leniently and then kind-checks it, so a file
/// agrees when every M3 `errorCode` it declares is reported.
#[test]
#[ignore = "requires a PureScript checkout"]
fn l3_kind_scoreboard_with_annotations() {
    let Some(repo) = corpus_root() else {
        eprintln!("skipping: vendored corpus not found and PURESCRIPT_REPO is unset");
        return;
    };

    let mut per_code: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut mismatches = Vec::new();
    let mut failing_agree = 0usize;
    let mut failing_total = 0usize;

    let failing_dir = repo.join("failing");
    for path in collected_files(&failing_dir, None) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if is_ffi_excluded(&path, &text) {
            continue;
        }
        let all_codes = annotation_codes(&text);
        if all_codes.iter().any(|code| code == "ErrorParsingModule") {
            continue;
        }
        let expected: Vec<String> = all_codes
            .iter()
            .filter(|code| M3_CODES.contains(&code.as_str()))
            .cloned()
            .collect();
        if expected.is_empty() || expected.len() != all_codes.len() {
            continue;
        }

        failing_total += 1;
        let sources = support_sources(&path, &text);
        let inputs: Vec<(&str, &str)> = sources
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let result = psrs_driver::check_program_kinds_lenient(&inputs);
        let ours = match &result {
            Ok(()) => HashSet::new(),
            Err(errors) => diagnostic_codes(errors),
        };
        let mut matched_all = true;
        for code in &expected {
            let entry = per_code.entry(code.clone()).or_insert((0, 0));
            entry.1 += 1;
            if ours.contains(code.as_str()) {
                entry.0 += 1;
            } else {
                matched_all = false;
            }
        }
        if matched_all {
            failing_agree += 1;
        } else {
            mismatches.push((path.clone(), expected.join(","), {
                let mut produced: Vec<&str> = ours.into_iter().collect();
                produced.sort_unstable();
                produced.join(",")
            }));
        }
    }

    println!("M3 failing agreement: {failing_agree}/{failing_total}");
    println!("per-code agreement:");
    for (code, (agree, total)) in &per_code {
        println!("  {code}: {agree}/{total}");
    }
    if !mismatches.is_empty() {
        println!("failing mismatches ({}):", mismatches.len());
        for (path, expected, produced) in mismatches.iter().take(40) {
            let relative = path.strip_prefix(&repo).unwrap_or(path);
            println!(
                "  {}: expected [{expected}], produced [{produced}]",
                relative.display()
            );
        }
    }
}

/// The main source plus the modules in a sibling directory named after the file
/// stem, which is how the corpus supplies a case's support modules.
fn support_sources(path: &Path, text: &str) -> Vec<(String, String)> {
    let mut sources = vec![(path.to_string_lossy().into_owned(), text.to_owned())];
    let support_dir = path.with_extension("");
    if support_dir.is_dir() {
        let mut files = Vec::new();
        collect_purs_files(&support_dir, &mut files);
        files.sort();
        for support in files {
            if let Ok(text) = std::fs::read_to_string(&support) {
                sources.push((support.to_string_lossy().into_owned(), text));
            }
        }
    }
    sources
}

/// Measures M2 resolution coverage using the corpus's `@shouldFailWith`
/// annotations, which need no `purs` install and no support libraries. Each
/// `failing` file is resolved on its own with missing imports tolerated, so a
/// file agrees when every M2 `errorCode` it declares is reported.
#[test]
#[ignore = "requires a PureScript checkout"]
fn l2_resolution_scoreboard_with_annotations() {
    let Some(repo) = corpus_root() else {
        eprintln!("skipping: vendored corpus not found and PURESCRIPT_REPO is unset");
        return;
    };

    let mut per_code: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut mismatches = Vec::new();
    let mut failing_agree = 0usize;
    let mut failing_total = 0usize;

    let failing_dir = repo.join("failing");
    for path in collected_files(&failing_dir, None) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if is_ffi_excluded(&path, &text) {
            continue;
        }
        let all_codes = annotation_codes(&text);
        if all_codes.iter().any(|code| code == "ErrorParsingModule") {
            continue;
        }
        let expected: Vec<String> = all_codes
            .iter()
            .filter(|code| M2_CODES.contains(&code.as_str()))
            .cloned()
            .collect();
        if expected.is_empty() || expected.len() != all_codes.len() {
            continue;
        }

        failing_total += 1;
        let sources = support_sources(&path, &text);
        let inputs: Vec<(&str, &str)> = sources
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let result = psrs_driver::check_program_lenient(&inputs);
        let ours = match &result {
            Ok(()) => HashSet::new(),
            Err(errors) => diagnostic_codes(errors),
        };
        let mut matched_all = true;
        for code in &expected {
            let entry = per_code.entry(code.clone()).or_insert((0, 0));
            entry.1 += 1;
            if ours.contains(code.as_str()) {
                entry.0 += 1;
            } else {
                matched_all = false;
            }
        }
        if matched_all {
            failing_agree += 1;
        } else {
            mismatches.push((path.clone(), expected.join(","), {
                let mut produced: Vec<&str> = ours.into_iter().collect();
                produced.sort_unstable();
                produced.join(",")
            }));
        }
    }

    let passing_dir = repo.join("passing");
    let mut passing_ok = 0usize;
    let mut passing_total = 0usize;
    for path in collected_files(&passing_dir, None) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if is_ffi_excluded(&path, &text) {
            continue;
        }
        passing_total += 1;
        let sources = support_sources(&path, &text);
        let inputs: Vec<(&str, &str)> = sources
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        if psrs_driver::check_program_lenient(&inputs).is_ok() {
            passing_ok += 1;
        }
    }

    println!("M2 failing agreement: {failing_agree}/{failing_total}");
    println!("per-code agreement:");
    for (code, (agree, total)) in &per_code {
        println!("  {code}: {agree}/{total}");
    }
    println!("passing modules resolved: {passing_ok}/{passing_total}");
    if !mismatches.is_empty() {
        println!("failing mismatches ({}):", mismatches.len());
        for (path, expected, produced) in mismatches.iter().take(40) {
            let relative = path.strip_prefix(&repo).unwrap_or(path);
            println!(
                "  {}: expected [{expected}], produced [{produced}]",
                relative.display()
            );
        }
    }
}
