use crate::trace::{
    CompileTrace, TraceArtifactId, TraceParameter, TraceRecorder, TraceRepresentation,
    TraceValidationCoverage, TraceValidationSpec,
};
use crate::{
    Artifact, BackendError, CompileFailure, PartialStages, Stages, TargetCapabilities,
    annotate_errors,
};

use crate::{cc, effects, mir};

mod emission;

pub(crate) fn compile_with_context_inner(
    module: psrs_core::Module,
    effect_context: Option<psrs_core::effect::EffectCompilation>,
    target: TargetCapabilities,
    mut capture: Option<&mut PartialStages>,
    mut trace: Option<&mut TraceRecorder>,
) -> Result<Stages, Vec<BackendError>> {
    let target_parameter = || {
        vec![TraceParameter {
            key: "target_capabilities",
            value: format!("{target:?}"),
        }]
    };
    let mut core_id = trace.as_deref().map(|trace| trace.initial_core());
    let owner = module.entry.map(|entry| entry.module);

    let core_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.core.optimize",
            &[core_id.expect("traced Core input")],
            TraceValidationCoverage::Composite,
            target_parameter(),
        )
    });
    let mut module = match psrs_core::opt::optimize(module, psrs_core::opt::Budget::default()) {
        Ok(module) => module,
        Err(errors) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), core_call) {
                trace.reject(call, errors.len());
            }
            return Err(annotate_errors(
                errors
                    .into_iter()
                    .map(|error| {
                        BackendError::new("P7 Core optimization", error.span, error.message)
                            .with_module(error.module)
                    })
                    .collect(),
                owner,
            ));
        }
    };
    let optimized_core = module.clone();
    if let Some(capture) = capture.as_deref_mut() {
        capture.core = Some(optimized_core.clone());
    }
    if let (Some(trace), Some(call)) = (trace.as_deref_mut(), core_call) {
        core_id = trace
            .complete(
                call,
                &[TraceRepresentation::Core],
                &[TraceValidationSpec::output(
                    "psrs_core::opt::optimize",
                    0,
                    TraceValidationCoverage::Composite,
                )],
            )
            .first()
            .copied();
    }

    let binding_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.external_bindings.extract",
            &[core_id.expect("traced optimized Core")],
            TraceValidationCoverage::NotObserved,
            Vec::new(),
        )
    });
    let mut external_bindings = crate::ExternalBindings::from_core(&module);
    let mut binding_id = if let (Some(trace), Some(call)) = (trace.as_deref_mut(), binding_call) {
        trace
            .complete(call, &[TraceRepresentation::ExternalBindings], &[])
            .first()
            .copied()
    } else {
        None
    };

    let mut effect_source = None;
    let mut effect_registry = crate::boundary::RepresentationRegistry::new();
    if let Some(context) = effect_context.as_ref() {
        let effect_call = trace.as_deref_mut().map(|trace| {
            trace.begin(
                "backend.effect.lower",
                &[
                    core_id.expect("traced optimized Core"),
                    binding_id.expect("traced external bindings"),
                ],
                TraceValidationCoverage::Composite,
                vec![TraceParameter {
                    key: "effect_context",
                    value: "present".into(),
                }],
            )
        });
        let prepared = match effects::lower_effects(&mut module, &mut external_bindings, context) {
            Ok(prepared) => prepared,
            Err(errors) => {
                if let (Some(trace), Some(call)) = (trace.as_deref_mut(), effect_call) {
                    trace.reject(call, errors.len());
                }
                return Err(errors);
            }
        };
        effect_registry = prepared.registry;
        effect_source = Some(prepared.source);
        if let (Some(trace), Some(call)) = (trace.as_deref_mut(), effect_call) {
            let outputs = trace.complete(
                call,
                &[
                    TraceRepresentation::Core,
                    TraceRepresentation::ExternalBindings,
                ],
                &[TraceValidationSpec::outputs(
                    "psrs_backend::effects::lower_effects",
                    &[0, 1],
                    TraceValidationCoverage::Composite,
                )],
            );
            core_id = outputs.first().copied();
            binding_id = outputs.get(1).copied();
        }
    } else if let Some(trace) = trace.as_deref_mut() {
        trace.not_applicable("backend.effect.lower", "no effect context supplied");
    }

    let conformance_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.external_bindings.validate_conformance",
            &[
                core_id.expect("traced Core before binding validation"),
                binding_id.expect("traced external bindings"),
            ],
            TraceValidationCoverage::Direct,
            target_parameter(),
        )
    });
    if let Err(errors) = external_bindings.validate_conformance(&module, target) {
        if let (Some(trace), Some(call)) = (trace.as_deref_mut(), conformance_call) {
            trace.reject_validation(
                call,
                errors.len(),
                "ExternalBindings::validate_conformance",
                &[
                    core_id.expect("traced Core before binding validation"),
                    binding_id.expect("traced external bindings"),
                ],
            );
        }
        return Err(errors);
    }
    if let (Some(trace), Some(call)) = (trace.as_deref_mut(), conformance_call) {
        trace.complete(
            call,
            &[],
            &[TraceValidationSpec::inputs(
                "ExternalBindings::validate_conformance",
                &[0, 1],
                TraceValidationCoverage::Direct,
            )],
        );
    }

    let cc_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.cc.lower",
            &[
                core_id.expect("traced Core before CC lowering"),
                binding_id.expect("traced external bindings"),
            ],
            TraceValidationCoverage::Composite,
            target_parameter(),
        )
    });
    let lowered_cc = match cc::lower_module_with_relations(
        module,
        external_bindings,
        effect_source.as_ref(),
        effect_registry,
    ) {
        Ok(lowered) => lowered,
        Err(errors) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), cc_call) {
                trace.reject(call, errors.len());
            }
            return Err(errors);
        }
    };
    let cc = lowered_cc.cc;
    let (cc_id, binding_id) = if let (Some(trace), Some(call)) = (trace.as_deref_mut(), cc_call) {
        let outputs = trace.complete(
            call,
            &[
                TraceRepresentation::ClosureConverted,
                TraceRepresentation::ExternalBindings,
            ],
            &[TraceValidationSpec::outputs(
                "cc::lower_module_with_bindings",
                &[0],
                TraceValidationCoverage::Composite,
            )],
        );
        (outputs.first().copied(), outputs.get(1).copied())
    } else {
        (None, None)
    };
    if let Some(capture) = capture.as_deref_mut() {
        capture.cc = Some(cc.clone());
    }

    let mir_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.mir.lower",
            &[
                cc_id.expect("traced CC before MIR lowering"),
                binding_id.expect("traced external bindings before MIR lowering"),
            ],
            TraceValidationCoverage::Composite,
            target_parameter(),
        )
    });
    let context = match crate::linking::default_context() {
        Ok(context) => context,
        Err(errors) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), mir_call) {
                trace.reject(call, errors.len());
            }
            return Err(annotate_errors(errors, cc.entry.map(|entry| entry.module)));
        }
    };
    let (mir, mut wasi) = match mir::lower_module_with_bindings_and_resolve(
        cc.clone(),
        lowered_cc.externals,
        target,
        context.shared_resolve(),
    ) {
        Ok(result) => result,
        Err(errors) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), mir_call) {
                trace.reject(call, errors.len());
            }
            return Err(errors);
        }
    };
    let (mut mir, mir_ids) = if let (Some(trace), Some(call)) = (trace.as_deref_mut(), mir_call) {
        let outputs = trace.complete(
            call,
            &[TraceRepresentation::Mir, TraceRepresentation::WasiRegistry],
            &[TraceValidationSpec::output(
                "mir::lower_module_with_bindings",
                0,
                TraceValidationCoverage::Composite,
            )],
        );
        (mir, outputs)
    } else {
        (mir, Vec::new())
    };
    let mut mir_id = mir_ids.first().copied();
    let wasi_id = mir_ids.get(1).copied();
    if let Some(capture) = capture.as_deref_mut() {
        capture.mir = Some(mir.clone());
        capture.mir_stage = Some("P9 MIR lowering");
    }

    let mir_opt_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.mir.optimize",
            &[mir_id.expect("traced MIR before optimization")],
            TraceValidationCoverage::Composite,
            target_parameter(),
        )
    });
    mir = match mir::opt::optimize(mir, target) {
        Ok(mir) => mir,
        Err(errors) => {
            if let (Some(trace), Some(call)) = (trace.as_deref_mut(), mir_opt_call) {
                trace.reject(call, errors.len());
            }
            return Err(errors);
        }
    };
    if let (Some(trace), Some(call)) = (trace.as_deref_mut(), mir_opt_call) {
        mir_id = trace
            .complete(
                call,
                &[TraceRepresentation::Mir],
                &[TraceValidationSpec::output(
                    "mir::opt::optimize",
                    0,
                    TraceValidationCoverage::Composite,
                )],
            )
            .first()
            .copied();
    }
    if let Some(capture) = capture {
        capture.mir = Some(mir.clone());
        capture.mir_stage = Some("P10 MIR optimization");
    }

    let owner = mir.entry.map(|entry| entry.module);
    let target_call = trace.as_deref_mut().map(|trace| {
        trace.begin(
            "backend.target.requirements",
            &[mir_id.expect("traced MIR before target check")],
            TraceValidationCoverage::Direct,
            target_parameter(),
        )
    });
    if !target.component_model
        || !target.wasi_p2
        || !target.wasi_cli
        || !target.wasi_io
        || !target.wasi_clocks
        || !target.wasi_random
    {
        let errors = annotate_errors(
            vec![BackendError::new(
                "P11 target capabilities",
                mir.span,
                "the current artifact pipeline requires Component Model and WASI 0.2 capabilities",
            )],
            owner,
        );
        if let (Some(trace), Some(call)) = (trace.as_deref_mut(), target_call) {
            trace.reject(call, errors.len());
        }
        return Err(errors);
    }
    if let (Some(trace), Some(call)) = (trace.as_deref_mut(), target_call) {
        trace.complete(
            call,
            &[],
            &[TraceValidationSpec::input(
                "backend_component_wasi_target_requirements",
                0,
                TraceValidationCoverage::Direct,
            )],
        );
    }

    let emitted = emission::emit(&mir, &mut wasi, target, owner, mir_id, wasi_id, trace)?;
    let warnings = lowered_cc.warnings;

    Ok(Stages {
        core: optimized_core,
        effect_context,
        cc,
        mir,
        wasm: emitted.module,
        artifact: Artifact {
            wasm: emitted.component,
            wat: emitted.wat,
            warnings,
        },
    })
}

pub(crate) fn compile_with_context_traced(
    module: psrs_core::Module,
    effect_context: Option<psrs_core::effect::EffectCompilation>,
    target: TargetCapabilities,
    capture_partial: bool,
) -> (Result<Stages, CompileFailure>, CompileTrace) {
    let mut trace = TraceRecorder::new();
    let mut partial = PartialStages::default();
    let result = compile_with_context_inner(
        module,
        effect_context,
        target,
        capture_partial.then_some(&mut partial),
        Some(&mut trace),
    )
    .map_err(|errors| CompileFailure {
        errors,
        partial: Box::new(partial),
    });
    (result, trace.finish())
}
