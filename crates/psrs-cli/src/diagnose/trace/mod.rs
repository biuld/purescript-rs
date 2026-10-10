use super::{SourceInput, fingerprint_sources};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum TraceMode {
    #[default]
    Manifest,
    Dumps,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct EnvironmentMetadata {
    pub(super) host_os: Option<String>,
    pub(super) host_arch: Option<String>,
    pub(super) rustc_observed_version: Option<String>,
    pub(super) cargo_observed_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct CaseTrace {
    pub(super) version: u32,
    pub(super) dumps_requested: bool,
    pub(super) source_set_complete: bool,
    pub(super) pass_capture: TraceAvailability,
    pub(super) artifacts: Vec<TraceArtifactRecord>,
    pub(super) executions: Vec<TraceExecutionRecord>,
    pub(super) edges: Vec<TraceEdgeRecord>,
    pub(super) validations: Vec<TraceValidationRecord>,
    pub(super) lineage: TraceAvailability,
    pub(super) canonical_summaries: TraceAvailability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TraceArtifactRecord {
    pub(super) id: String,
    pub(super) representation: String,
    pub(super) state: String,
    pub(super) producer: Option<String>,
    pub(super) format_version: Option<u32>,
    pub(super) input_fingerprint: Option<InputFingerprint>,
    pub(super) sources: Vec<TraceSourceRecord>,
    pub(super) canonical_summary: TraceAvailability,
    pub(super) retained_dump: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct InputFingerprint {
    pub(super) algorithm: String,
    pub(super) version: u32,
    pub(super) value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TraceSourceRecord {
    pub(super) logical_name: String,
    pub(super) fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TraceExecutionRecord {
    pub(super) id: String,
    pub(super) pass_key: String,
    pub(super) contract_version: u32,
    pub(super) inputs: Vec<String>,
    pub(super) outputs: Vec<String>,
    pub(super) input_representations: Vec<String>,
    pub(super) output_representations: Vec<String>,
    pub(super) status: String,
    pub(super) diagnostic_indices: Vec<usize>,
    pub(super) validation_coverage: String,
    pub(super) parameters: BTreeMap<String, String>,
    pub(super) user_source_names: Vec<String>,
    pub(super) trusted_source_names: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TraceEdgeRecord {
    pub(super) execution: String,
    pub(super) artifact: String,
    pub(super) role: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TraceValidationRecord {
    pub(super) execution: String,
    pub(super) validator_key: String,
    pub(super) artifacts: Vec<String>,
    pub(super) status: String,
    pub(super) coverage: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct TraceAvailability {
    pub(super) status: String,
    pub(super) reason: Option<String>,
}

impl TraceAvailability {
    fn unavailable(reason: &str) -> Self {
        Self {
            status: "unavailable".into(),
            reason: Some(reason.into()),
        }
    }
}

pub(super) fn empty_case_trace(
    sources: &[SourceInput],
    complete: bool,
    fingerprint: &str,
    dumps_requested: bool,
) -> CaseTrace {
    CaseTrace {
        version: 1,
        dumps_requested,
        source_set_complete: complete,
        pass_capture: TraceAvailability::unavailable(
            "compiler did not reach an instrumented frontend pass boundary",
        ),
        artifacts: vec![source_set_artifact(sources, complete, fingerprint)],
        executions: Vec::new(),
        edges: Vec::new(),
        validations: Vec::new(),
        lineage: TraceAvailability::unavailable("node lineage is not captured by this trace slice"),
        canonical_summaries: TraceAvailability::unavailable(
            "no canonical artifact summary is implemented for this representation",
        ),
    }
}

fn source_set_artifact(
    sources: &[SourceInput],
    complete: bool,
    fingerprint: &str,
) -> TraceArtifactRecord {
    TraceArtifactRecord {
        id: "inputs:i0".into(),
        representation: "source_set".into(),
        state: if complete { "provided" } else { "partial" }.into(),
        producer: None,
        format_version: Some(1),
        input_fingerprint: Some(InputFingerprint {
            algorithm: "fnv1a64_ordered_sources".into(),
            version: 1,
            value: fingerprint.to_owned(),
        }),
        sources: sources
            .iter()
            .map(|source| TraceSourceRecord {
                logical_name: source.name.clone(),
                fingerprint: Some(fingerprint_sources(std::slice::from_ref(source))),
            })
            .collect(),
        canonical_summary: TraceAvailability {
            status: "not_applicable".into(),
            reason: None,
        },
        retained_dump: None,
    }
}

mod build;
mod compare;
mod environment;

pub(super) use build::from_report;
pub(super) use compare::compare_traces;
pub(super) use environment::observed_environment;
