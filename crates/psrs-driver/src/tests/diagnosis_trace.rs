use super::*;

#[test]
fn traced_and_untraced_compilation_produce_the_same_artifact_without_default_dumps() {
    let sources = [("Main.purs", "module Main where\nmain = 42\n")];
    let ordinary =
        compile_program_sources_with_prelude(&sources).expect("ordinary compile should succeed");
    let diagnosis = compile_program_sources_with_prelude_diagnosis(&sources, false);
    let traced = diagnosis
        .artifact
        .expect("diagnosis compile should succeed");

    assert_eq!(ordinary.wasm, traced.wasm);
    assert_eq!(ordinary.wat, traced.wat);
    assert_eq!(diagnosis.dumps, PartialIrDumps::default());
    assert!(diagnosis.backend_trace.is_some());
    assert_eq!(
        diagnosis
            .frontend_trace
            .as_ref()
            .and_then(|frontend| frontend.output_core),
        diagnosis
            .backend_trace
            .as_ref()
            .map(|trace| trace.initial_core),
    );
}

#[test]
fn effect_lowering_output_binding_artifact_flows_into_validation_and_cc() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain :: Effect Unit\nmain = log \"trace\"\n";
    let report = compile_program_sources_with_prelude_diagnosis(&[("Main.purs", source)], false);
    assert!(
        report.artifact.is_some(),
        "effect program should compile: {:?}",
        report.diagnostics
    );
    let trace = report.backend_trace.expect("backend pass trace");
    let effect = trace
        .executions
        .iter()
        .find(|execution| execution.pass_key == "backend.effect.lower")
        .expect("effect context triggers the lowering pass");
    assert_eq!(effect.status, TracePassStatus::Completed);
    let bindings = *effect.outputs.get(1).expect("effect pass outputs bindings");
    let conformance = trace
        .executions
        .iter()
        .find(|execution| execution.pass_key == "backend.external_bindings.validate_conformance")
        .expect("binding conformance is observed");
    let cc = trace
        .executions
        .iter()
        .find(|execution| execution.pass_key == "backend.cc.lower")
        .expect("CC lowering is observed");
    assert!(conformance.inputs.contains(&bindings));
    assert!(cc.inputs.contains(&bindings));
    assert!(trace.edges.iter().any(|edge| {
        edge.execution == effect.id
            && edge.artifact == bindings
            && edge.role == TraceEdgeRole::Output
    }));
}

#[test]
fn trace_dump_references_name_the_exact_produced_artifacts() {
    let source = "module Main where\nmain = 42\n";
    let report = compile_program_sources_with_prelude_diagnosis(&[("Main.purs", source)], true);
    assert!(report.artifact.is_some());
    assert!(
        report
            .dumps
            .core
            .as_ref()
            .is_some_and(|dump| !dump.is_empty())
    );
    assert!(
        report
            .dumps
            .cc
            .as_ref()
            .is_some_and(|dump| !dump.is_empty())
    );
    assert!(
        report
            .dumps
            .mir
            .as_ref()
            .is_some_and(|dump| !dump.is_empty())
    );
    let trace = report.backend_trace.expect("backend pass trace");
    let artifacts = report.dump_artifacts;
    for id in [artifacts.core, artifacts.cc, artifacts.mir]
        .into_iter()
        .flatten()
    {
        assert!(trace.artifacts.iter().any(|artifact| artifact.id == id));
    }
    assert_eq!(
        trace
            .executions
            .iter()
            .find(|execution| execution.pass_key == "backend.core.optimize")
            .and_then(|execution| execution.outputs.first().copied()),
        artifacts.core,
    );
}

#[test]
fn frontend_rejection_has_diagnostics_but_no_backend_trace_or_core_output() {
    let report = compile_program_sources_with_prelude_diagnosis(
        &[("Main.purs", "module Main where\nmain = @\n")],
        false,
    );
    assert!(report.artifact.is_none());
    assert!(report.backend_trace.is_none());
    assert!(!report.diagnostics.is_empty());
    let frontend = report.frontend_trace.expect("frontend attempt is observed");
    assert_eq!(frontend.status, TracePassStatus::Rejected);
    assert!(frontend.output_core.is_none());
    assert_eq!(frontend.diagnostic_indices.len(), report.diagnostics.len());
}

#[test]
fn target_plan_records_provider_and_memory_lineage() {
    let source = "module Main where\nmain = 42\n";
    let report = compile_program_sources_with_prelude_diagnosis(&[("Main.purs", source)], false);
    assert!(
        report.artifact.is_some(),
        "program should compile: {:?}",
        report.diagnostics
    );
    let trace = report.backend_trace.expect("backend pass trace");
    let plan = trace
        .executions
        .iter()
        .find(|execution| execution.pass_key == "backend.target.plan")
        .expect("the checked plan is an observed execution");
    assert_eq!(plan.status, TracePassStatus::Completed);
    let parameter = |key: &str| {
        plan.parameters
            .iter()
            .find(|parameter| parameter.key == key)
            .map(|parameter| parameter.value.as_str())
    };
    assert_eq!(parameter("artifacts"), Some("0"));
    assert_eq!(parameter("heap_start"), Some("24"));
    assert!(
        parameter("selected_providers")
            .expect("selected providers are recorded")
            .contains("wasi:cli/exit"),
        "{:?}",
        plan.parameters
    );
    assert!(
        parameter("external_world")
            .expect("the external world is recorded")
            .contains("wasi:cli/exit"),
    );
    assert!(trace.artifacts.iter().any(|artifact| {
        artifact.representation == TraceRepresentation::LinkPlan
            && artifact.state == TraceArtifactState::Produced
    }));
}

#[test]
fn target_rejection_keeps_prior_artifacts_and_maps_errors_to_its_execution() {
    let prepared = crate::prepare_sources(&[("Main.purs", "module Main where\nmain = 42\n")])
        .expect("frontend should produce checked Core");
    let target = psrs_backend::TargetCapabilities {
        component_model: false,
        ..Default::default()
    };
    let traced = psrs_backend::compile_with_context_traced(
        prepared.core,
        prepared.effect_context,
        target,
        false,
    );
    let failure = traced
        .result
        .expect_err("target profile should be rejected");
    let target_execution = traced
        .trace
        .executions
        .iter()
        .find(|execution| execution.pass_key == "backend.target.requirements")
        .expect("the target check is an observed execution");
    assert_eq!(target_execution.status, TracePassStatus::Rejected);
    assert_eq!(target_execution.outputs, Vec::<TraceArtifactId>::new());
    assert!(!target_execution.diagnostic_indices.is_empty());
    assert!(
        target_execution
            .diagnostic_indices
            .iter()
            .all(|index| *index < failure.errors.len())
    );
    assert!(traced.trace.artifacts.iter().any(|artifact| {
        artifact.representation == TraceRepresentation::Mir
            && artifact.state == TraceArtifactState::Produced
    }));
    assert!(
        !traced
            .trace
            .artifacts
            .iter()
            .any(|artifact| { artifact.representation == TraceRepresentation::WasmModule })
    );
}
