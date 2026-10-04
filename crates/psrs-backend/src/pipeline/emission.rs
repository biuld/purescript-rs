use super::*;

use crate::component;

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
    let module = match crate::wasm::lower_module_with_capabilities(mir, wasi, target) {
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
                    "wasm::lower_module_with_capabilities",
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

    let resolve_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.component.resolve_world",
            &[],
            TraceValidationCoverage::NotObserved,
            vec![TraceParameter {
                key: "wit_source",
                value: "vendored".into(),
            }],
        )
    });
    let (resolve, world) = match component::command_world() {
        Ok(result) => result,
        Err(message) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), resolve_call) {
                trace.reject(call, 1);
            }
            return Err(annotate_errors(
                vec![BackendError::new("P11 component", mir.span, message)],
                owner,
            ));
        }
    };
    let world_id = if let (Some(trace), Some(call)) = (trace.as_deref_mut(), resolve_call) {
        trace
            .complete(call, &[TraceRepresentation::WitWorld], &[])
            .first()
            .copied()
    } else {
        None
    };

    let component_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.component.assemble",
            &[
                core_id.expect("traced core Wasm before component assembly"),
                world_id.expect("traced WIT world before component assembly"),
            ],
            TraceValidationCoverage::Composite,
            Vec::new(),
        )
    });
    let binary = match component::componentize(&core, &resolve, world) {
        Ok(binary) => binary,
        Err(message) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), component_call) {
                trace.reject(call, 1);
            }
            return Err(annotate_errors(
                vec![BackendError::new("P11 component", mir.span, message)],
                owner,
            ));
        }
    };
    let component_id = if let (Some(trace), Some(call)) = (trace.as_deref_mut(), component_call) {
        trace
            .complete(
                call,
                &[TraceRepresentation::ComponentBinary],
                &[TraceValidationSpec::output(
                    "wit_component::ComponentEncoder::validate_and_encode",
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
