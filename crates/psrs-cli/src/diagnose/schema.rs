use super::trace::{CaseTrace, EnvironmentMetadata, TraceMode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct SourceInput {
    pub(super) name: String,
    pub(super) text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct DiagnosticRecord {
    pub(super) origin: String,
    pub(super) source: Option<String>,
    pub(super) stage: String,
    pub(super) start: u32,
    pub(super) end: u32,
    pub(super) code: Option<String>,
    pub(super) kind: Option<String>,
    pub(super) message: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CaseStatus {
    Passed,
    Failed,
    Excluded,
    TimedOut,
    Crashed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct FirstBlocker {
    pub(super) stage: String,
    pub(super) category: String,
    pub(super) message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct CaseRecord {
    pub(super) path: String,
    pub(super) input_fingerprint: String,
    pub(super) input_set_complete: bool,
    pub(super) elapsed_ms: u64,
    pub(super) status: CaseStatus,
    pub(super) excluded_reason: Option<String>,
    pub(super) diagnostics: Vec<DiagnosticRecord>,
    pub(super) first_blocker: Option<FirstBlocker>,
    pub(super) bundle: Option<String>,
    #[serde(default)]
    pub(super) trace: Option<CaseTrace>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct GroupRecord {
    pub(super) stage: String,
    pub(super) category: String,
    pub(super) sample_messages: Vec<String>,
    pub(super) cases: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Cohort {
    pub(super) mode: String,
    pub(super) corpus: Option<String>,
    pub(super) filter: Option<String>,
    pub(super) limit: Option<usize>,
    pub(super) timeout_seconds: u64,
    pub(super) trusted_stdlib_fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct CompilerRevision {
    pub(super) head: Option<String>,
    pub(super) dirty: bool,
    pub(super) working_tree_fingerprint: String,
    pub(super) binary_fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct BundleContext {
    pub(super) compiler: CompilerRevision,
    pub(super) trusted_stdlib_fingerprint: String,
    pub(super) executable: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Snapshot {
    pub(super) schema_version: u32,
    pub(super) cohort: Cohort,
    pub(super) compiler: CompilerRevision,
    #[serde(default)]
    pub(super) environment: EnvironmentMetadata,
    #[serde(default)]
    pub(super) trace_mode: TraceMode,
    pub(super) cases: Vec<CaseRecord>,
    pub(super) groups: Vec<GroupRecord>,
    pub(super) counts: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct CompareRow {
    pub(super) path: String,
    pub(super) change: String,
    pub(super) input_comparison: String,
    pub(super) before: Option<String>,
    pub(super) after: Option<String>,
    #[serde(default)]
    pub(super) trace_comparison: Option<TraceComparison>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TraceComparison {
    pub(super) status: String,
    pub(super) first_pass_difference: Option<PassDifference>,
    pub(super) artifact_content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct PassDifference {
    pub(super) index: usize,
    pub(super) kind: String,
    pub(super) before_pass: Option<String>,
    pub(super) after_pass: Option<String>,
    pub(super) before_status: Option<String>,
    pub(super) after_status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct CompareReport {
    pub(super) compatible_cohort: bool,
    pub(super) observed_environment_compatibility: String,
    pub(super) build_toolchain_compatibility: String,
    pub(super) before_compiler: CompilerRevision,
    pub(super) after_compiler: CompilerRevision,
    pub(super) changes: Vec<CompareRow>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Options {
    pub(super) file: Option<PathBuf>,
    pub(super) additional_inputs: Vec<PathBuf>,
    pub(super) corpus: Option<String>,
    pub(super) filter: Option<String>,
    pub(super) limit: Option<usize>,
    pub(super) timeout_seconds: u64,
    pub(super) output: PathBuf,
    pub(super) trace: bool,
}
