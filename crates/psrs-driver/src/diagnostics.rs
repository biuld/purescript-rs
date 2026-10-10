use crate::{Artifact, ProgramDiagnostic};
use psrs_backend::{CompileTrace, TraceArtifactId, TracePassStatus};

/// IR dumps retained when a program reaches only part of the compile pipeline.
/// The boundary label identifies the last pass completed or input boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartialIrDumps {
    pub core_stage: Option<&'static str>,
    pub core: Option<String>,
    pub cc_stage: Option<&'static str>,
    pub cc: Option<String>,
    pub mir_stage: Option<&'static str>,
    pub mir: Option<String>,
}

/// Result of one whole-program compile attempt, including partial IR on failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilationReport {
    pub artifact: Option<Artifact>,
    pub diagnostics: Vec<ProgramDiagnostic>,
    pub dumps: PartialIrDumps,
    /// Backend pass/artifact events, populated only by the diagnosis entrypoint.
    pub backend_trace: Option<CompileTrace>,
    /// The driver-owned frontend boundary for a diagnosis compile attempt.
    pub frontend_trace: Option<FrontendPassTrace>,
    /// Exact trace artifacts represented by the retained Core/CC/MIR dumps.
    pub dump_artifacts: IrDumpArtifacts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontendPassTrace {
    pub contract_version: u32,
    pub pass_key: &'static str,
    /// Number of leading entries owned by the trusted library.
    pub trusted_prefix: usize,
    pub source_names: Vec<String>,
    pub status: TracePassStatus,
    pub output_core: Option<TraceArtifactId>,
    pub diagnostic_indices: Vec<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IrDumpArtifacts {
    pub core: Option<TraceArtifactId>,
    pub cc: Option<TraceArtifactId>,
    pub mir: Option<TraceArtifactId>,
}

impl CompilationReport {
    pub(crate) fn failed(diagnostics: Vec<ProgramDiagnostic>, dumps: PartialIrDumps) -> Self {
        Self {
            artifact: None,
            diagnostics,
            dumps,
            backend_trace: None,
            frontend_trace: None,
            dump_artifacts: IrDumpArtifacts::default(),
        }
    }
}
