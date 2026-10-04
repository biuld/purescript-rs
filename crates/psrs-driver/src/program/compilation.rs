use super::{lower_program_to_core_and_effect_context, typecheck_warnings};
use crate::{
    Artifact, CompilationReport, Diagnostic, DiagnosticOrigin, PartialIrDumps, ProgramDiagnostic,
    backend_warnings,
};

/// Compiles a whole program to a single Wasm component.
pub fn compile_program_sources(
    sources: &[(&str, &str)],
) -> Result<Artifact, Vec<ProgramDiagnostic>> {
    compile_program_sources_with_trusted_prefix(sources, 0)
}

pub(crate) fn compile_program_sources_with_trusted_prefix(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> Result<Artifact, Vec<ProgramDiagnostic>> {
    let report = compile_attempt(sources, trusted_prefix, false);
    match report.artifact {
        Some(artifact) => Ok(artifact),
        None => Err(report.diagnostics),
    }
}

pub(crate) fn compile_program_sources_with_trusted_prefix_report(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
) -> CompilationReport {
    compile_attempt(sources, trusted_prefix, true)
}

fn compile_attempt(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
    capture: bool,
) -> CompilationReport {
    let (core, source_warnings, effect_context) =
        match lower_program_to_core_and_effect_context(sources, trusted_prefix) {
            Ok(lowered) => lowered,
            Err(diagnostics) => {
                return CompilationReport::failed(diagnostics, PartialIrDumps::default());
            }
        };
    if capture {
        let linked_core = core.clone();
        let stages = match psrs_backend::compile_with_context_capturing(
            core,
            effect_context,
            psrs_backend::TargetCapabilities::default(),
        ) {
            Ok(stages) => stages,
            Err(failure) => {
                let mut dumps = PartialIrDumps::default();
                let (core, stage) = match failure.partial.core {
                    Some(core) => (core, "P7 Core optimization"),
                    None => (linked_core, "P7 Core verification"),
                };
                dumps.core_stage = Some(stage);
                dumps.core = Some(format!("{core:#?}"));
                if let Some(cc) = failure.partial.cc {
                    dumps.cc_stage = Some("P8 closure conversion (verified)");
                    dumps.cc = Some(format!("{cc:#?}"));
                }
                if let Some(mir) = failure.partial.mir {
                    dumps.mir_stage = failure.partial.mir_stage;
                    dumps.mir = Some(format!("{mir:#?}"));
                }
                return CompilationReport::failed(backend_diagnostics(failure.errors), dumps);
            }
        };
        return success_report(stages, source_warnings, trusted_prefix, true);
    }

    match psrs_backend::compile_with_context(
        core,
        effect_context,
        psrs_backend::TargetCapabilities::default(),
    ) {
        Ok(stages) => success_report(stages, source_warnings, trusted_prefix, false),
        Err(errors) => {
            CompilationReport::failed(backend_diagnostics(errors), PartialIrDumps::default())
        }
    }
}

fn success_report(
    stages: psrs_backend::Stages,
    source_warnings: Vec<super::ProgramWarning>,
    trusted_prefix: usize,
    capture: bool,
) -> CompilationReport {
    let mut warnings = typecheck_warnings(source_warnings, trusted_prefix);
    warnings.extend(backend_warnings(stages.artifact.warnings, trusted_prefix));
    let dumps = if capture {
        PartialIrDumps {
            core_stage: Some("P7 Core optimization"),
            core: Some(format!("{:#?}", stages.core)),
            cc_stage: Some("P8 closure conversion (verified)"),
            cc: Some(format!("{:#?}", stages.cc)),
            mir_stage: Some("P10 MIR optimization"),
            mir: Some(format!("{:#?}", stages.mir)),
        }
    } else {
        PartialIrDumps::default()
    };
    CompilationReport {
        artifact: Some(Artifact {
            wasm: stages.artifact.wasm,
            wat: stages.artifact.wat,
            warnings,
        }),
        diagnostics: Vec::new(),
        dumps,
    }
}

fn backend_diagnostics(errors: Vec<psrs_backend::BackendError>) -> Vec<ProgramDiagnostic> {
    errors
        .into_iter()
        .map(|error| ProgramDiagnostic {
            source: error.module.map_or(DiagnosticOrigin::Program, |module| {
                DiagnosticOrigin::Source(module.0 as usize)
            }),
            diagnostic: Diagnostic {
                stage: error.pass,
                span: error.span,
                message: error.message,
                code: None,
                kind: Some(error.kind),
            },
        })
        .collect()
}
