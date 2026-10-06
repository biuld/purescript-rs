use super::*;

use crate::linking;

pub(crate) struct EmittedWasm {
    pub module: crate::wasm::Module,
    pub component: Vec<u8>,
    pub wat: String,
}

pub(crate) fn emit(
    mir: &mir::Module,
    wasi: &mut crate::abi::WasiRegistry,
    target: TargetCapabilities,
    owner: Option<psrs_hir::ModuleId>,
    mir_id: Option<TraceArtifactId>,
    wasi_id: Option<TraceArtifactId>,
    mut trace: Option<&mut TraceRecorder>,
) -> Result<EmittedWasm, Vec<BackendError>> {
    let target_parameter = || {
        vec![TraceParameter {
            key: "target_capabilities",
            value: format!("{target:?}"),
        }]
    };
    let context = linking::default_context().map_err(|errors| annotate_errors(errors, owner))?;
    let mut plan_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.target.plan",
            &[
                mir_id.expect("traced MIR before target planning"),
                wasi_id.expect("traced WASI registry before target planning"),
            ],
            TraceValidationCoverage::Composite,
            target_parameter(),
        )
    });
    let link = match linking::plan_for_module(&context, mir, wasi, target) {
        Ok(link) => link,
        Err(errors) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), plan_call) {
                trace.reject(call, errors.len());
            }
            return Err(annotate_errors(errors, owner));
        }
    };
    if let Some(call) = plan_call.as_mut() {
        annotate_plan(call, &link);
    }
    if let (Some(trace), Some(call)) = (trace.as_deref_mut(), plan_call) {
        trace.complete(
            call,
            &[TraceRepresentation::LinkPlan],
            &[TraceValidationSpec::output(
                "psrs_linker::plan",
                0,
                TraceValidationCoverage::Composite,
            )],
        );
    }

    let wasm_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.wasm.lower",
            &[
                mir_id.expect("traced MIR before Wasm lowering"),
                wasi_id.expect("traced WASI registry before Wasm lowering"),
            ],
            TraceValidationCoverage::Composite,
            target_parameter(),
        )
    });
    let module = match crate::wasm::lower_module_with_plan(mir, wasi, target, &link) {
        Ok(module) => module,
        Err(errors) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), wasm_call) {
                trace.reject(call, errors.len());
            }
            return Err(annotate_errors(errors, owner));
        }
    };
    let wasm_id = if let (Some(trace), Some(call)) = (trace.as_deref_mut(), wasm_call) {
        trace
            .complete(
                call,
                &[TraceRepresentation::WasmModule],
                &[TraceValidationSpec::output(
                    "wasm::lower_module_with_plan",
                    0,
                    TraceValidationCoverage::Composite,
                )],
            )
            .first()
            .copied()
    } else {
        None
    };

    let encode_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.wasm.encode",
            &[wasm_id.expect("traced Wasm module before encoding")],
            TraceValidationCoverage::NotObserved,
            Vec::new(),
        )
    });
    let core = match crate::wasm::encode_module(&module) {
        Ok(core) => core,
        Err(errors) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), encode_call) {
                trace.reject(call, errors.len());
            }
            return Err(annotate_errors(errors, owner));
        }
    };
    let core_id = if let (Some(trace), Some(call)) = (trace.as_deref_mut(), encode_call) {
        trace
            .complete(call, &[TraceRepresentation::WasmCoreBinary], &[])
            .first()
            .copied()
    } else {
        None
    };

    let component_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.component.assemble",
            &[core_id.expect("traced core Wasm before component assembly")],
            TraceValidationCoverage::Composite,
            Vec::new(),
        )
    });
    let binary = match linking::compose(&link, &core, mir.span, owner) {
        Ok(binary) => binary,
        Err(errors) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), component_call) {
                trace.reject(call, errors.len());
            }
            return Err(annotate_errors(errors, owner));
        }
    };
    let component_id = if let (Some(trace), Some(call)) = (trace.as_deref_mut(), component_call) {
        trace
            .complete(
                call,
                &[TraceRepresentation::ComponentBinary],
                &[TraceValidationSpec::output(
                    "psrs_linker::compose",
                    0,
                    TraceValidationCoverage::Composite,
                )],
            )
            .first()
            .copied()
    } else {
        None
    };

    let validate_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.component.validate",
            &[component_id.expect("traced component before validation")],
            TraceValidationCoverage::Direct,
            target_parameter(),
        )
    });
    if let Err(error) = crate::validator_for(target).validate_all(&binary) {
        let errors = annotate_errors(
            vec![BackendError::new(
                "P11 Wasm validation",
                mir.span,
                format!("generated WebAssembly failed validation: {error}"),
            )],
            owner,
        );
        if let (Some(trace), Some(call)) = (trace.as_deref_mut(), validate_call) {
            trace.reject_validation(
                call,
                errors.len(),
                "wasmparser::Validator::validate_all",
                &[component_id.expect("traced component before validation")],
            );
        }
        return Err(errors);
    }
    if let (Some(trace), Some(call)) = (trace.as_deref_mut(), validate_call) {
        trace.complete(
            call,
            &[],
            &[TraceValidationSpec::input(
                "wasmparser::Validator::validate_all",
                0,
                TraceValidationCoverage::Direct,
            )],
        );
    }

    let print_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.wat.print",
            &[component_id.expect("traced component before WAT printing")],
            TraceValidationCoverage::NotObserved,
            Vec::new(),
        )
    });
    let wat = match wasmprinter::print_bytes(&binary) {
        Ok(wat) => wat,
        Err(error) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), print_call) {
                trace.reject(call, 1);
            }
            return Err(annotate_errors(
                vec![BackendError::new(
                    "P11 WAT printing",
                    mir.span,
                    format!("generated WebAssembly could not be printed as WAT: {error}"),
                )],
                owner,
            ));
        }
    };
    if let (Some(trace), Some(call)) = (trace, print_call) {
        trace.complete(call, &[TraceRepresentation::WatText], &[]);
    }

    Ok(EmittedWasm {
        module,
        component: binary,
        wat,
    })
}

/// Records the checked plan's lineage: selected providers, artifact digests,
/// memory boundary, and the planned external world.
fn annotate_plan(call: &mut crate::trace::TraceCall, link: &linking::LinkPlan) {
    let plan = &link.plan;
    call.add_parameter("requirements", plan.bindings().len().to_string());
    call.add_parameter("artifacts", plan.artifacts().len().to_string());
    call.add_parameter(
        "selected_providers",
        plan.bindings()
            .iter()
            .map(|binding| format!("{}={}.{}", binding.origin, binding.module, binding.field))
            .collect::<Vec<_>>()
            .join(";"),
    );
    call.add_parameter(
        "artifact_digests",
        plan.artifacts()
            .iter()
            .map(|artifact| format!("{}={}", artifact.id, artifact.sha256))
            .collect::<Vec<_>>()
            .join(";"),
    );
    call.add_parameter("external_world", plan.external_world().join(";"));
    call.add_parameter(
        "stack_bounds",
        plan.artifacts()
            .iter()
            .filter_map(|artifact| {
                artifact
                    .stack_bound_bytes
                    .map(|bound| format!("{}={bound}", artifact.id))
            })
            .collect::<Vec<_>>()
            .join(";"),
    );
    call.add_parameter("heap_start", plan.memory().heap_start.to_string());
    call.add_parameter("minimum_pages", plan.memory().minimum_pages.to_string());
}
