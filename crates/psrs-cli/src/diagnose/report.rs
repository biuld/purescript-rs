use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn first_blocker(diagnostic: &DiagnosticRecord) -> FirstBlocker {
    let category = diagnostic.code.as_deref().unwrap_or_else(|| {
        if diagnostic.stage == "P8 CC verification"
            && (diagnostic.message.starts_with("call shape mismatch:")
                || diagnostic
                    .message
                    .starts_with("call result shape mismatch:"))
        {
            "call_shape_mismatch"
        } else {
            diagnostic.kind.as_deref().unwrap_or("uncoded")
        }
    });
    FirstBlocker {
        stage: diagnostic.stage.clone(),
        category: category.to_owned(),
        message: diagnostic.message.clone(),
    }
}

pub(super) fn group_cases(snapshot: &mut Snapshot) {
    let mut groups: BTreeMap<(String, String), (BTreeSet<String>, BTreeSet<String>)> =
        BTreeMap::new();
    let mut counts = BTreeMap::new();
    for case in &snapshot.cases {
        let key = match case.status {
            CaseStatus::Passed => "passed",
            CaseStatus::Failed => "failed",
            CaseStatus::Excluded => "excluded",
            CaseStatus::TimedOut => "timed_out",
            CaseStatus::Crashed => "crashed",
        };
        *counts.entry(key.to_owned()).or_default() += 1;
        if let Some(first) = &case.first_blocker {
            let (cases, messages) = groups
                .entry((first.stage.clone(), first.category.clone()))
                .or_default();
            cases.insert(case.path.clone());
            messages.insert(first.message.clone());
        }
    }
    snapshot.groups = groups
        .into_iter()
        .map(|((stage, category), (cases, messages))| GroupRecord {
            stage,
            category,
            sample_messages: messages.into_iter().take(3).collect(),
            cases: cases.into_iter().collect(),
        })
        .collect();
    snapshot.groups.sort_by(|left, right| {
        right
            .cases
            .len()
            .cmp(&left.cases.len())
            .then_with(|| left.stage.cmp(&right.stage))
    });
    snapshot.counts = counts;
}

pub(super) fn print_summary(snapshot: &Snapshot) {
    println!(
        "{} cases: {} passed, {} failed, {} excluded, {} timed out, {} crashed",
        snapshot.cases.len(),
        count(snapshot, "passed"),
        count(snapshot, "failed"),
        count(snapshot, "excluded"),
        count(snapshot, "timed_out"),
        count(snapshot, "crashed")
    );
    println!(
        "Largest first-blocker signatures (identical signature does not prove a shared root cause):"
    );
    for group in snapshot.groups.iter().take(12) {
        println!(
            "  {} case(s) at {} [{}]",
            group.cases.len(),
            group.stage,
            group.category
        );
        for message in &group.sample_messages {
            println!("    {message}");
        }
    }
}

pub(super) fn count(snapshot: &Snapshot, key: &str) -> usize {
    snapshot.counts.get(key).copied().unwrap_or(0)
}

pub(super) fn compare(before_path: &str, after_path: &str) -> Result<(), String> {
    let before: Snapshot = read_json(before_path)?;
    let after: Snapshot = read_json(after_path)?;
    if before.schema_version != SCHEMA_VERSION || after.schema_version != SCHEMA_VERSION {
        return Err("cannot compare unsupported diagnosis snapshot schema".into());
    }
    if before.cohort.mode != after.cohort.mode
        || before.cohort.corpus != after.cohort.corpus
        || before.cohort.filter != after.cohort.filter
        || before.cohort.limit != after.cohort.limit
        || before.cohort.timeout_seconds != after.cohort.timeout_seconds
        || before.cohort.trusted_stdlib_fingerprint != after.cohort.trusted_stdlib_fingerprint
    {
        return Err("snapshot cohorts differ (mode, selected cases, timeout, or trusted stdlib); comparison is not meaningful".into());
    }
    let old = before
        .cases
        .iter()
        .map(|case| (case.path.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let new = after
        .cases
        .iter()
        .map(|case| (case.path.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let paths = old
        .keys()
        .chain(new.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let changes = paths
        .into_iter()
        .map(|path| {
            let left = old.get(path).copied();
            let right = new.get(path).copied();
            let (change, input_comparison) = match (left, right) {
                (None, Some(_)) => ("unmatched_new_case", "unavailable"),
                (Some(_), None) => ("unmatched_removed_case", "unavailable"),
                (Some(a), Some(b))
                    if a.input_set_complete
                        && b.input_set_complete
                        && a.input_fingerprint != b.input_fingerprint =>
                {
                    ("input_changed", "changed")
                }
                (Some(a), Some(b)) if !a.input_set_complete || !b.input_set_complete => {
                    (compare_status(a, b), "incomplete")
                }
                (Some(a), Some(b)) => (compare_status(a, b), "same"),
                (None, None) => unreachable!(),
            };
            CompareRow {
                path: path.to_owned(),
                change: change.into(),
                input_comparison: input_comparison.into(),
                before: left.and_then(case_label),
                after: right.and_then(case_label),
            }
        })
        .collect::<Vec<_>>();
    let report = CompareReport {
        compatible_cohort: true,
        before_compiler: before.compiler,
        after_compiler: after.compiler,
        changes,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
    );
    Ok(())
}

pub(super) fn compare_status(before: &CaseRecord, after: &CaseRecord) -> &'static str {
    match (before.status, after.status) {
        (CaseStatus::Passed, CaseStatus::Passed) => "unchanged_pass",
        (CaseStatus::Passed, CaseStatus::Excluded) => "scope_changed",
        (CaseStatus::Passed, _) => "regressed",
        (CaseStatus::Excluded, CaseStatus::Excluded) => "unchanged_excluded",
        (CaseStatus::Excluded, _) => "scope_changed",
        (_, CaseStatus::Excluded) => "scope_changed",
        (_, CaseStatus::Passed) => "recovered",
        (CaseStatus::Failed, CaseStatus::Failed) => {
            match (&before.first_blocker, &after.first_blocker) {
                (Some(left), Some(right)) if left.stage != right.stage => "stage_changed",
                (Some(left), Some(right)) if left.category != right.category => "category_changed",
                (Some(left), Some(right)) if left.message != right.message => {
                    "diagnostic_details_changed"
                }
                _ => "unchanged_failure",
            }
        }
        (a, b) if status_key(a) == status_key(b) => "unchanged_failure",
        _ => "execution_failure_changed",
    }
}

pub(super) fn status_key(status: CaseStatus) -> &'static str {
    match status {
        CaseStatus::Passed => "passed",
        CaseStatus::Failed => "failed",
        CaseStatus::Excluded => "excluded",
        CaseStatus::TimedOut => "timed_out",
        CaseStatus::Crashed => "crashed",
    }
}

pub(super) fn case_label(case: &CaseRecord) -> Option<String> {
    Some(match &case.first_blocker {
        Some(first) => format!("{} [{}]: {}", first.stage, first.category, first.message),
        None => status_key(case.status).to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed_case(path: &str, message: &str) -> CaseRecord {
        CaseRecord {
            path: path.into(),
            input_fingerprint: "input".into(),
            input_set_complete: true,
            elapsed_ms: 1,
            status: CaseStatus::Failed,
            excluded_reason: None,
            diagnostics: Vec::new(),
            first_blocker: Some(FirstBlocker {
                stage: "P8 CC verification".into(),
                category: "call_shape_mismatch".into(),
                message: message.into(),
            }),
            bundle: None,
        }
    }

    #[test]
    fn first_blocker_groups_call_shapes_by_stable_category() {
        let diagnostic = DiagnosticRecord {
            origin: "source".into(),
            source: Some("Main.purs".into()),
            stage: "P8 CC verification".into(),
            start: 4,
            end: 9,
            code: None,
            kind: Some("InvalidCompilerIr".into()),
            message: "call shape mismatch: expected Integer, got Number".into(),
        };
        assert_eq!(first_blocker(&diagnostic).category, "call_shape_mismatch");
    }

    #[test]
    fn grouping_keeps_distinct_messages_inside_one_stage_category() {
        let mut snapshot = Snapshot {
            schema_version: SCHEMA_VERSION,
            cohort: Cohort {
                mode: "file".into(),
                corpus: None,
                filter: None,
                limit: None,
                timeout_seconds: 20,
                trusted_stdlib_fingerprint: "stdlib".into(),
            },
            compiler: CompilerRevision {
                head: None,
                dirty: false,
                working_tree_fingerprint: "tree".into(),
                binary_fingerprint: "binary".into(),
            },
            cases: vec![
                failed_case("A.purs", "expected Integer, got Number"),
                failed_case("B.purs", "expected Integer, got Boolean"),
            ],
            groups: Vec::new(),
            counts: BTreeMap::new(),
        };
        group_cases(&mut snapshot);
        assert_eq!(snapshot.groups.len(), 1);
        assert_eq!(snapshot.groups[0].cases, ["A.purs", "B.purs"]);
        assert_eq!(snapshot.groups[0].sample_messages.len(), 2);
    }

    #[test]
    fn incomplete_inputs_do_not_hide_a_pass_recovery() {
        let before = CaseRecord {
            status: CaseStatus::TimedOut,
            input_set_complete: false,
            ..failed_case("Main.purs", "worker timeout")
        };
        let after = CaseRecord {
            status: CaseStatus::Passed,
            first_blocker: None,
            ..failed_case("Main.purs", "worker timeout")
        };
        assert_eq!(compare_status(&before, &after), "recovered");
    }
}
