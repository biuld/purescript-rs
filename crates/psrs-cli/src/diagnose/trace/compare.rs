use super::super::{PassDifference, TraceComparison};
use super::{CaseTrace, TraceExecutionRecord};

pub fn compare_traces(
    before: Option<&CaseTrace>,
    after: Option<&CaseTrace>,
    same_inputs: bool,
) -> TraceComparison {
    let unavailable = |status: &str| TraceComparison {
        status: status.into(),
        first_pass_difference: None,
        artifact_content: "unavailable_no_canonical_summary".into(),
    };
    if !same_inputs {
        return unavailable("not_compared_input_mismatch");
    }
    let (Some(before), Some(after)) = (before, after) else {
        return unavailable("unavailable_trace_missing");
    };
    if before.version != after.version {
        return unavailable("unavailable_trace_version_mismatch");
    }
    if before.version != 1 {
        return unavailable("unavailable_unsupported_trace_version");
    }
    if before.pass_capture.status == "unavailable" || after.pass_capture.status == "unavailable" {
        return unavailable("unavailable_pass_capture_gap");
    }

    let max_len = before.executions.len().max(after.executions.len());
    for index in 0..max_len {
        let left = before.executions.get(index);
        let right = after.executions.get(index);
        let Some(kind) = execution_difference(left, right) else {
            continue;
        };
        return TraceComparison {
            status: "pass_difference_observed".into(),
            first_pass_difference: Some(PassDifference {
                index,
                kind: kind.into(),
                before_pass: left.map(|pass| pass.pass_key.clone()),
                after_pass: right.map(|pass| pass.pass_key.clone()),
                before_status: left.map(|pass| pass.status.clone()),
                after_status: right.map(|pass| pass.status.clone()),
            }),
            artifact_content: "unavailable_no_canonical_summary".into(),
        };
    }
    if before.pass_capture.status != after.pass_capture.status {
        let index = max_len;
        return TraceComparison {
            status: "pass_capture_coverage_changed".into(),
            first_pass_difference: Some(PassDifference {
                index,
                kind: "pass_capture_coverage_changed".into(),
                before_pass: None,
                after_pass: None,
                before_status: Some(before.pass_capture.status.clone()),
                after_status: Some(after.pass_capture.status.clone()),
            }),
            artifact_content: "unavailable_no_canonical_summary".into(),
        };
    }
    if before.pass_capture.status == "observed_prefix" {
        return TraceComparison {
            status: "no_difference_in_observed_prefix".into(),
            first_pass_difference: None,
            artifact_content: "unavailable_no_canonical_summary".into(),
        };
    }
    TraceComparison {
        status: "no_observed_pass_difference".into(),
        first_pass_difference: None,
        artifact_content: "unavailable_no_canonical_summary".into(),
    }
}

fn execution_difference(
    before: Option<&TraceExecutionRecord>,
    after: Option<&TraceExecutionRecord>,
) -> Option<&'static str> {
    match (before, after) {
        (None, Some(_)) => Some("pass_added"),
        (Some(_), None) => Some("pass_removed"),
        (Some(before), Some(after)) if before.pass_key != after.pass_key => {
            Some("pass_order_or_identity_changed")
        }
        (Some(before), Some(after)) if before.contract_version != after.contract_version => {
            Some("pass_contract_changed")
        }
        (Some(before), Some(after)) if before.status != after.status => Some("pass_status_changed"),
        (Some(before), Some(after))
            if before.input_representations != after.input_representations
                || before.output_representations != after.output_representations =>
        {
            Some("pass_boundary_changed")
        }
        (Some(before), Some(after)) if before.validation_coverage != after.validation_coverage => {
            Some("validation_coverage_changed")
        }
        (Some(before), Some(after))
            if before.parameters != after.parameters
                || before.user_source_names != after.user_source_names
                || before.trusted_source_names != after.trusted_source_names =>
        {
            Some("pass_inputs_or_parameters_changed")
        }
        (Some(_), Some(_)) => None,
        (None, None) => None,
    }
}
