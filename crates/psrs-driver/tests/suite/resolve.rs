//! The L2 resolution-agreement scoreboard.
//!
//! Every `failing` case is resolved on its own with missing imports tolerated,
//! so a case agrees when every M2 `errorCode` it declares is reported. The
//! `passing` column is the same measurement in the other direction: how many
//! corpus programs resolve at all, since a file that cannot resolve cannot be
//! measured by any later gate.

use super::corpus::{
    annotation_codes, collected_files, corpus_root, diagnostic_codes, is_ffi_excluded,
    support_sources,
};
use std::collections::BTreeMap;

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
            Ok(()) => std::collections::HashSet::new(),
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
