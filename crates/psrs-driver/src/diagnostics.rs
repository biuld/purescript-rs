use crate::{Artifact, ProgramDiagnostic};

/// IR dumps retained when a program reaches only part of the compile pipeline.
/// A stage name records exactly which pass produced the dump.
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
}

impl CompilationReport {
    pub(crate) fn failed(diagnostics: Vec<ProgramDiagnostic>, dumps: PartialIrDumps) -> Self {
        Self {
            artifact: None,
            diagnostics,
            dumps,
        }
    }
}
