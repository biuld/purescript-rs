use super::{lower_program_to_core_and_effect_context, typecheck_warnings};
use crate::{
    Artifact, CompilationReport, Diagnostic, DiagnosticOrigin, FrontendPassTrace, IrDumpArtifacts,
    PartialIrDumps, ProgramDiagnostic, backend_warnings,
};
use psrs_backend::{CompileTrace, TraceArtifactId, TracePassStatus};

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
    let report = compile_attempt(sources, trusted_prefix, false, false, None);
    match report.artifact {
        Some(artifact) => Ok(artifact),
        None => Err(report.diagnostics),
    }
}

pub(super) fn compile_attempt(
    sources: &[(&str, &str)],
    trusted_prefix: usize,
    capture_dumps: bool,
    trace_enabled: bool,
    runner: Option<&crate::prelude::CommandRunner>,
) -> CompilationReport {
    let (core, source_warnings, effect_context) = match if let Some(runner) = runner {
        super::lowering::lower_program_inner(
            sources,
            trusted_prefix,
            Some((&runner.module, &runner.function)),
        )
    } else {
        lower_program_to_core_and_effect_context(sources, trusted_prefix)
    } {
        Ok(lowered) => lowered,
        Err(diagnostics) => {
            let mut report = CompilationReport::failed(diagnostics, PartialIrDumps::default());
            if trace_enabled {
                report.frontend_trace = Some(FrontendPassTrace {
                    contract_version: 1,
                    pass_key: "driver.frontend.lower_program_to_core",
                    trusted_prefix,
                    source_names: sources.iter().map(|(name, _)| (*name).to_owned()).collect(),
                    status: TracePassStatus::Rejected,
                    output_core: None,
                    diagnostic_indices: (0..report.diagnostics.len()).collect(),
                });
            }
            return report;
        }
    };

    let mut frontend_trace = trace_enabled.then(|| FrontendPassTrace {
        contract_version: 1,
        pass_key: "driver.frontend.lower_program_to_core",
        trusted_prefix,
        source_names: sources.iter().map(|(name, _)| (*name).to_owned()).collect(),
        status: TracePassStatus::Completed,
        output_core: None,
        diagnostic_indices: Vec::new(),
    });

    if trace_enabled {
        let linked_core = capture_dumps.then(|| core.clone());
        let traced = psrs_backend::compile_with_context_traced(
            core,
            effect_context,
            psrs_backend::TargetCapabilities::default(),
            capture_dumps,
        );
        if let Some(frontend) = &mut frontend_trace {
            frontend.output_core = Some(traced.trace.initial_core);
        }
        return match traced.result {
            Ok(stages) => success_report(
                stages,
                source_warnings,
                trusted_prefix,
                capture_dumps,
                Some(traced.trace),
                frontend_trace,
            ),
            Err(failure) => {
                let (dumps, dump_artifacts) = if capture_dumps {
                    partial_dumps(
                        *failure.partial,
                        linked_core.expect("dump capture retained linked Core"),
                        Some(&traced.trace),
                    )
                } else {
                    (PartialIrDumps::default(), IrDumpArtifacts::default())
                };
                let mut report =
                    CompilationReport::failed(backend_diagnostics(failure.errors), dumps);
                report.backend_trace = Some(traced.trace);
                report.frontend_trace = frontend_trace;
                report.dump_artifacts = dump_artifacts;
                report
            }
        };
    }

    if capture_dumps {
        let linked_core = core.clone();
        let stages = match psrs_backend::compile_with_context_capturing(
            core,
            effect_context,
            psrs_backend::TargetCapabilities::default(),
        ) {
            Ok(stages) => stages,
            Err(failure) => {
                let (dumps, _) = partial_dumps(*failure.partial, linked_core, None);
                return CompilationReport::failed(backend_diagnostics(failure.errors), dumps);
            }
        };
        return success_report(stages, source_warnings, trusted_prefix, true, None, None);
    }

    match psrs_backend::compile_with_context(
        core,
        effect_context,
        psrs_backend::TargetCapabilities::default(),
    ) {
        Ok(stages) => success_report(stages, source_warnings, trusted_prefix, false, None, None),
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
    backend_trace: Option<CompileTrace>,
    frontend_trace: Option<FrontendPassTrace>,
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
    let dump_artifacts = backend_trace
        .as_ref()
        .filter(|_| capture)
        .map(|trace| IrDumpArtifacts {
            core: artifact_from_pass(trace, "backend.core.optimize", 0),
            cc: artifact_from_pass(trace, "backend.cc.lower", 0),
            mir: artifact_from_pass(trace, "backend.mir.optimize", 0),
        })
        .unwrap_or_default();
    CompilationReport {
        artifact: Some(Artifact {
            wasm: stages.artifact.wasm,
            wat: stages.artifact.wat,
            warnings,
        }),
        diagnostics: Vec::new(),
        dumps,
        backend_trace,
        frontend_trace,
        dump_artifacts,
    }
}

fn partial_dumps(
    partial: psrs_backend::PartialStages,
    linked_core: psrs_core::Module,
    trace: Option<&CompileTrace>,
) -> (PartialIrDumps, IrDumpArtifacts) {
    let mut dumps = PartialIrDumps::default();
    let mut artifacts = IrDumpArtifacts::default();
    let (core, stage, artifact) = match partial.core {
        Some(core) => (
            core,
            "P7 Core optimization",
            trace.and_then(|trace| artifact_from_pass(trace, "backend.core.optimize", 0)),
        ),
        None => (
            linked_core,
            "linked Core input to P7",
            trace.map(|trace| trace.initial_core),
        ),
    };
    dumps.core_stage = Some(stage);
    dumps.core = Some(format!("{core:#?}"));
    artifacts.core = artifact;
    if let Some(cc) = partial.cc {
        dumps.cc_stage = Some("P8 closure conversion (verified)");
        dumps.cc = Some(format!("{cc:#?}"));
        artifacts.cc = trace.and_then(|trace| artifact_from_pass(trace, "backend.cc.lower", 0));
    }
    if let Some(mir) = partial.mir {
        dumps.mir_stage = partial.mir_stage;
        dumps.mir = Some(format!("{mir:#?}"));
        artifacts.mir = trace.and_then(|trace| match partial.mir_stage {
            Some("P9 MIR lowering") => artifact_from_pass(trace, "backend.mir.lower", 0),
            _ => artifact_from_pass(trace, "backend.mir.optimize", 0),
        });
    }
    (dumps, artifacts)
}

fn artifact_from_pass(
    trace: &CompileTrace,
    pass_key: &str,
    output_index: usize,
) -> Option<TraceArtifactId> {
    trace
        .executions
        .iter()
        .find(|execution| execution.pass_key == pass_key)
        .and_then(|execution| execution.outputs.get(output_index).copied())
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
