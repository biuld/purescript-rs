//! The L3 kind-agreement scoreboard.
//!
//! A `failing` case is resolved leniently and then kind-checked, so a case
//! agrees when every M3 `errorCode` it declares is reported.

use super::corpus::{
    annotation_codes, collected_files, corpus_root, diagnostic_codes, is_ffi_excluded, load_case,
};
use std::collections::BTreeMap;

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
        let case = load_case(&path, &failing_dir, &text);
        let result = psrs_driver::check_program_kinds_lenient_with_prelude(&case.inputs());
        let ours = match &result {
            Ok(()) => Default::default(),
            Err(errors) => diagnostic_codes(case.own_diagnostics(errors)),
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
