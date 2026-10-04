use super::*;
use psrs_driver::{
    TraceArtifactState, TraceEdgeRole, TracePassStatus, TraceRepresentation,
    TraceValidationCoverage, TraceValidationStatus,
};
use std::collections::BTreeMap;

fn representation_key(value: TraceRepresentation) -> &'static str {
    match value {
        TraceRepresentation::Core => "core",
        TraceRepresentation::ExternalBindings => "external_bindings",
        TraceRepresentation::ClosureConverted => "closure_converted",
        TraceRepresentation::Mir => "mir",
        TraceRepresentation::WasmModule => "wasm_module",
        TraceRepresentation::WasmCoreBinary => "wasm_core_binary",
        TraceRepresentation::ComponentBinary => "component_binary",
        TraceRepresentation::WatText => "wat_text",
        TraceRepresentation::WitWorld => "wit_world",
        TraceRepresentation::WasiRegistry => "wasi_registry",
    }
}

fn artifact_state_key(value: TraceArtifactState) -> &'static str {
    match value {
        TraceArtifactState::Provided => "provided",
        TraceArtifactState::Produced => "produced",
    }
}

fn pass_status_key(value: TracePassStatus) -> &'static str {
    match value {
        TracePassStatus::Completed => "completed",
        TracePassStatus::Rejected => "rejected",
        TracePassStatus::NotApplicable => "not_applicable",
    }
}

fn edge_role_key(value: TraceEdgeRole) -> &'static str {
    match value {
        TraceEdgeRole::Input => "input",
        TraceEdgeRole::Output => "output",
    }
}

fn validation_status_key(value: TraceValidationStatus) -> &'static str {
    match value {
        TraceValidationStatus::Passed => "passed",
        TraceValidationStatus::Rejected => "rejected",
    }
}

fn coverage_key(value: TraceValidationCoverage) -> &'static str {
    match value {
        TraceValidationCoverage::Direct => "direct",
        TraceValidationCoverage::Composite => "composite",
        TraceValidationCoverage::NotObserved => "not_observed",
    }
}

pub fn from_report(
    sources: &[SourceInput],
    complete: bool,
    fingerprint: &str,
    trusted_stdlib_fingerprint: &str,
    dumps_requested: bool,
    report: &psrs_driver::CompilationReport,
) -> Result<CaseTrace, String> {
    let mut trace = empty_case_trace(sources, complete, fingerprint, dumps_requested);
    let mut artifact_representations = BTreeMap::new();
    let trusted_source_names = report
        .frontend_trace
        .as_ref()
        .map(|frontend| {
            frontend
                .source_names
                .iter()
                .take(frontend.trusted_prefix)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let user_source_names = report
        .frontend_trace
        .as_ref()
        .map(|frontend| {
            frontend
                .source_names
                .iter()
                .skip(frontend.trusted_prefix)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if let Some(frontend) = report.frontend_trace.as_ref() {
        if frontend.trusted_prefix > frontend.source_names.len() {
            return Err("frontend trace trusted prefix exceeds its source list".into());
        }
        trace.artifacts.push(TraceArtifactRecord {
            id: "inputs:trusted_stdlib:i0".into(),
            representation: "trusted_library_set".into(),
            state: "provided".into(),
            producer: None,
            format_version: Some(1),
            input_fingerprint: Some(InputFingerprint {
                algorithm: "fnv1a64_trusted_stdlib".into(),
                version: 1,
                value: trusted_stdlib_fingerprint.to_owned(),
            }),
            sources: trusted_source_names
                .iter()
                .map(|logical_name| TraceSourceRecord {
                    logical_name: logical_name.clone(),
                    fingerprint: None,
                })
                .collect(),
            canonical_summary: TraceAvailability {
                status: "not_applicable".into(),
                reason: None,
            },
            retained_dump: None,
        });
        let source_input = "inputs:i0".to_owned();
        let trusted_input = "inputs:trusted_stdlib:i0".to_owned();
        let output = frontend.output_core.map(|id| format!("backend:a{}", id.0));
        trace.executions.push(TraceExecutionRecord {
            id: "frontend:e0".into(),
            pass_key: frontend.pass_key.into(),
            contract_version: frontend.contract_version,
            inputs: vec![source_input.clone(), trusted_input.clone()],
            outputs: output.iter().cloned().collect(),
            input_representations: vec!["source_set".into(), "trusted_library_set".into()],
            output_representations: output
                .as_ref()
                .map(|_| vec!["core".into()])
                .unwrap_or_default(),
            status: pass_status_key(frontend.status).into(),
            diagnostic_indices: frontend.diagnostic_indices.clone(),
            validation_coverage: "not_observed".into(),
            parameters: BTreeMap::from([(
                "trusted_prefix".into(),
                frontend.trusted_prefix.to_string(),
            )]),
            user_source_names: user_source_names.clone(),
            trusted_source_names: trusted_source_names.clone(),
        });
        trace.edges.extend([
            TraceEdgeRecord {
                execution: "frontend:e0".into(),
                artifact: source_input,
                role: "input".into(),
            },
            TraceEdgeRecord {
                execution: "frontend:e0".into(),
                artifact: trusted_input,
                role: "input".into(),
            },
        ]);
        if let Some(artifact) = output {
            trace.edges.push(TraceEdgeRecord {
                execution: "frontend:e0".into(),
                artifact,
                role: "output".into(),
            });
        }
    }

    if let Some(backend) = report.backend_trace.as_ref() {
        let frontend_core = report
            .frontend_trace
            .as_ref()
            .and_then(|frontend| frontend.output_core);
        if frontend_core.is_some_and(|id| id != backend.initial_core) {
            return Err("frontend trace output does not match backend initial Core".into());
        }
        for artifact in &backend.artifacts {
            let id = format!("backend:a{}", artifact.id.0);
            let representation = representation_key(artifact.representation).to_owned();
            artifact_representations.insert(artifact.id.0, representation.clone());
            let frontend_produced = artifact.id == backend.initial_core && frontend_core.is_some();
            trace.artifacts.push(TraceArtifactRecord {
                id,
                representation,
                state: if frontend_produced {
                    "produced"
                } else {
                    artifact_state_key(artifact.state)
                }
                .into(),
                producer: if frontend_produced {
                    Some("frontend:e0".into())
                } else {
                    artifact
                        .producer
                        .map(|producer| format!("backend:e{}", producer.0))
                },
                format_version: None,
                input_fingerprint: None,
                sources: Vec::new(),
                canonical_summary: TraceAvailability::unavailable(
                    "canonical artifact summaries are not implemented",
                ),
                retained_dump: retained_dump_for(artifact.id.0, report, dumps_requested),
            });
        }
        validate_dump_refs(report, backend)?;
        for execution in &backend.executions {
            let inputs = execution
                .inputs
                .iter()
                .map(|artifact| format!("backend:a{}", artifact.0))
                .collect::<Vec<_>>();
            let outputs = execution
                .outputs
                .iter()
                .map(|artifact| format!("backend:a{}", artifact.0))
                .collect::<Vec<_>>();
            let input_representations = representation_list(
                execution.id.0,
                "input",
                &execution.inputs,
                &artifact_representations,
            )?;
            let output_representations = representation_list(
                execution.id.0,
                "output",
                &execution.outputs,
                &artifact_representations,
            )?;
            trace.executions.push(TraceExecutionRecord {
                id: format!("backend:e{}", execution.id.0),
                pass_key: execution.pass_key.into(),
                contract_version: execution.contract_version,
                input_representations,
                output_representations,
                inputs,
                outputs,
                status: pass_status_key(execution.status).into(),
                diagnostic_indices: execution.diagnostic_indices.clone(),
                validation_coverage: coverage_key(execution.validation_coverage).into(),
                parameters: execution
                    .parameters
                    .iter()
                    .map(|parameter| (parameter.key.to_owned(), parameter.value.clone()))
                    .collect(),
                user_source_names: Vec::new(),
                trusted_source_names: Vec::new(),
            });
        }
        trace
            .edges
            .extend(backend.edges.iter().map(|edge| TraceEdgeRecord {
                execution: format!("backend:e{}", edge.execution.0),
                artifact: format!("backend:a{}", edge.artifact.0),
                role: edge_role_key(edge.role).into(),
            }));
        for validation in &backend.validations {
            trace.validations.push(TraceValidationRecord {
                execution: format!("backend:e{}", validation.execution.0),
                validator_key: validation.validator_key.into(),
                artifacts: validation
                    .artifacts
                    .iter()
                    .map(|artifact| format!("backend:a{}", artifact.0))
                    .collect(),
                status: validation_status_key(validation.status).into(),
                coverage: coverage_key(validation.coverage).into(),
            });
        }
    }
    trace.pass_capture = match (&report.frontend_trace, &report.artifact) {
        (None, _) => TraceAvailability::unavailable(
            "trusted prelude loading stopped before the frontend trace boundary",
        ),
        (Some(_), Some(_)) => TraceAvailability {
            status: "complete".into(),
            reason: None,
        },
        (Some(_), None) => TraceAvailability {
            status: "observed_prefix".into(),
            reason: Some("compilation stopped after the recorded execution prefix".into()),
        },
    };
    validate_references(&trace)?;
    Ok(trace)
}

fn representation_list(
    execution_id: u32,
    role: &str,
    ids: &[psrs_driver::TraceArtifactId],
    representations: &BTreeMap<u32, String>,
) -> Result<Vec<String>, String> {
    ids.iter()
        .map(|artifact| {
            representations.get(&artifact.0).cloned().ok_or_else(|| {
                format!(
                    "backend trace execution {execution_id} references missing {role} artifact {}",
                    artifact.0
                )
            })
        })
        .collect()
}

fn retained_dump_for(
    artifact_id: u32,
    report: &psrs_driver::CompilationReport,
    requested: bool,
) -> Option<String> {
    if !requested {
        return None;
    }
    if report
        .dump_artifacts
        .core
        .is_some_and(|id| id.0 == artifact_id)
    {
        Some("core.debug".into())
    } else if report
        .dump_artifacts
        .cc
        .is_some_and(|id| id.0 == artifact_id)
    {
        Some("cc.debug".into())
    } else if report
        .dump_artifacts
        .mir
        .is_some_and(|id| id.0 == artifact_id)
    {
        Some("mir.debug".into())
    } else {
        None
    }
}

fn validate_dump_refs(
    report: &psrs_driver::CompilationReport,
    backend: &psrs_driver::CompileTrace,
) -> Result<(), String> {
    for id in [
        report.dump_artifacts.core,
        report.dump_artifacts.cc,
        report.dump_artifacts.mir,
    ]
    .into_iter()
    .flatten()
    {
        if !backend.artifacts.iter().any(|artifact| artifact.id == id) {
            return Err(format!(
                "IR dump references missing backend artifact {}",
                id.0
            ));
        }
    }
    Ok(())
}

fn validate_references(trace: &CaseTrace) -> Result<(), String> {
    let artifact_ids = trace
        .artifacts
        .iter()
        .map(|artifact| artifact.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let execution_ids = trace
        .executions
        .iter()
        .map(|execution| execution.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for execution in &trace.executions {
        for artifact in execution.inputs.iter().chain(&execution.outputs) {
            if !artifact_ids.contains(artifact.as_str()) {
                return Err(format!(
                    "trace execution {} references unknown artifact {artifact}",
                    execution.id
                ));
            }
        }
    }
    for edge in &trace.edges {
        if !execution_ids.contains(edge.execution.as_str())
            || !artifact_ids.contains(edge.artifact.as_str())
        {
            return Err(format!(
                "trace edge references unknown execution/artifact {} / {}",
                edge.execution, edge.artifact
            ));
        }
    }
    for validation in &trace.validations {
        if !execution_ids.contains(validation.execution.as_str())
            || validation
                .artifacts
                .iter()
                .any(|artifact| !artifact_ids.contains(artifact.as_str()))
        {
            return Err(format!(
                "trace validation references unknown execution/artifact {}",
                validation.execution
            ));
        }
    }
    Ok(())
}
