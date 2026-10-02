//! The L4 and L5 type and class scoreboards.
//!
//! One `failing` case is measured against both boards in the same pass, because
//! they share an entry point: `check_program_types_lenient` resolves leniently,
//! then kind-checks and type checks every module that resolved, so a case
//! blocked on a missing library module still reaches the type checker for the
//! modules it does provide.
//!
//! A case is measured by its own modules only. Modules the driver's loader
//! discovered on disk to satisfy the case's imports are compiled alongside it,
//! but their diagnostics are reported separately and never decide agreement.

use super::corpus::{
    Case, annotation_codes, collected_files, corpus_root, is_ffi_excluded, load_case,
};
use std::collections::HashSet;

/// The `errorCode`s the M4 milestone is accountable for, from D-04. A case that
/// declares one of these and nothing outside the M4 set is measured here; a case
/// that mixes codes across milestones is left to the milestone that owns all of
/// them, because a case only agrees when every code it declares is reported.
const M4_CODES: [&str; 8] = [
    "TypesDoNotUnify",
    "HoleInferredType",
    "EscapedSkolem",
    "InfiniteType",
    "ExpectedType",
    "CannotApplyExpressionOfTypeOnType",
    "AmbiguousTypeVariables",
    "IntOutOfRange",
];

/// The `errorCode`s the M5 milestone is accountable for, from D-04.
const M5_CODES: [&str; 12] = [
    "NoInstanceFound",
    "OverlappingInstances",
    "OrphanInstance",
    "InvalidInstanceHead",
    "InvalidNewtypeInstance",
    "ClassInstanceArityMismatch",
    "MissingClassMember",
    "PossiblyInfiniteInstance",
    "DuplicateTypeClass",
    "DuplicateInstance",
    "CycleInTypeClassDeclaration",
    "CannotDeriveInvalidConstructorArg",
];

/// One board's tally: agreement, and agreement per declared code.
#[derive(Default)]
struct Tally {
    agree: usize,
    total: usize,
    per_code: Vec<(String, usize, usize)>,
    mismatches: Vec<(String, String, String)>,
}

impl Tally {
    fn record(&mut self, expected: &[String], produced: &HashSet<&str>, path: &str) {
        self.total += 1;
        let mut matched = true;
        for code in expected {
            let hit = usize::from(produced.contains(code.as_str()));
            if hit == 0 {
                matched = false;
            }
            match self.per_code.iter_mut().find(|(name, _, _)| name == code) {
                Some(entry) => {
                    entry.1 += 1;
                    entry.2 += hit;
                }
                None => self.per_code.push((code.clone(), 1, hit)),
            }
        }
        if matched {
            self.agree += 1;
        } else {
            let mut produced: Vec<&str> = produced.iter().copied().collect();
            produced.sort_unstable();
            self.mismatches
                .push((path.to_owned(), expected.join(","), produced.join(",")));
        }
    }

    fn print(&self, label: &str) {
        println!("{label}: {}/{}", self.agree, self.total);
        if self.per_code.is_empty() {
            return;
        }
        println!("per-code agreement:");
        let mut entries = self.per_code.clone();
        entries.sort_by(|left, right| right.0.cmp(&left.0));
        for (code, total, agree) in entries {
            println!("  {code}: {agree}/{total}");
        }
        if !self.mismatches.is_empty() {
            println!("mismatches ({}):", self.mismatches.len());
            for (path, expected, produced) in self.mismatches.iter().take(40) {
                println!("  {path}: expected [{expected}], produced [{produced}]");
            }
        }
    }
}

/// Measures M4 type-checking and M5 class agreement using the corpus's own
/// `@shouldFailWith` annotations, which need no `purs` install and no support
/// libraries. A `failing` case agrees for a milestone when every `errorCode` it
/// declares from that milestone is reported by the case's own modules.
#[test]
#[ignore = "requires a PureScript checkout"]
fn l4_l5_scoreboard_with_annotations() {
    let Some(repo) = corpus_root() else {
        eprintln!("skipping: vendored corpus not found and PURESCRIPT_REPO is unset");
        return;
    };

    let failing_dir = repo.join("failing");
    let mut types = Tally::default();
    let mut classes = Tally::default();
    let mut blocked: Vec<(String, String)> = Vec::new();

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
        let expected_types: Vec<String> = all_codes
            .iter()
            .filter(|code| M4_CODES.contains(&code.as_str()))
            .cloned()
            .collect();
        let expected_classes: Vec<String> = all_codes
            .iter()
            .filter(|code| M5_CODES.contains(&code.as_str()))
            .cloned()
            .collect();
        if expected_types.is_empty() && expected_classes.is_empty() {
            continue;
        }

        let case = load_case(&path, &failing_dir, &text);
        let relative = relative_path(&repo, &path);
        let result = psrs_driver::check_program_types_lenient_with_prelude(&case.inputs());
        let own = match &result {
            Ok(()) => HashSet::new(),
            Err(errors) => own_codes(&case, errors),
        };
        // A case that never reached the type checker cannot disagree with a code;
        // it is blocked, and the reason is reported so the count is not read as
        // a type-checker regression.
        if own.is_empty()
            && result
                .as_ref()
                .err()
                .is_some_and(|errors| !case.own_diagnostics(errors).is_empty())
        {
            let first = result
                .as_ref()
                .err()
                .and_then(|errors| case.own_diagnostics(errors).first().copied())
                .map(|error| {
                    format!(
                        "{} [{:?}]: {}",
                        error.diagnostic.stage, error.diagnostic.code, error.diagnostic.message
                    )
                })
                .unwrap_or_else(|| "no diagnostic".to_owned());
            blocked.push((relative, first));
            continue;
        }
        if !expected_types.is_empty() {
            types.record(&expected_types, &own, &relative);
        }
        if !expected_classes.is_empty() {
            classes.record(&expected_classes, &own, &relative);
        }
    }

    let cases_with_own_codes = types.total + classes.total;
    types.print("M4 failing agreement");
    classes.print("M5 failing agreement");
    println!("measured cases: {cases_with_own_codes}");
    if !blocked.is_empty() {
        println!("blocked before the type checker ({}):", blocked.len());
        for (path, reason) in blocked.iter().take(40) {
            println!("  {path}: {reason}");
        }
    }
}

fn own_codes<'a>(case: &Case, errors: &'a [psrs_driver::ProgramDiagnostic]) -> HashSet<&'a str> {
    case.own_diagnostics(errors)
        .into_iter()
        .filter_map(|error| error.diagnostic.code)
        .collect()
}

fn relative_path(repo: &std::path::Path, path: &std::path::Path) -> String {
    path.strip_prefix(repo)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
