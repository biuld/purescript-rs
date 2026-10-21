//! One checked runtime binding inventory shared by all function lowerers.
use super::*;
use psrs_hir::SymbolId;
use std::collections::HashMap;

pub(in crate::mir) struct RuntimeContext {
    pub(in crate::mir) imports: HashMap<SymbolId, Import>,
    pub(in crate::mir) record_primitives: HashMap<SymbolId, psrs_runtime::RecordPrimitive>,
    /// Provider bindings discharged by the MIR lowering into inline try
    /// regions; they plan no import because the provider has no host ABI.
    pub(in crate::mir) exception_providers: HashMap<SymbolId, crate::bindings::ExceptionProvider>,
    /// The module's planned exception tags. Empty unless a live binding raises
    /// or recovers the source error tag; `checkThrows`-only modules therefore
    /// never gain a tag section.
    pub(in crate::mir) exception_tags: Vec<crate::mir::Tag>,
}

impl RuntimeContext {
    pub(in crate::mir) fn checked(
        bindings: &[RuntimeBinding],
        physical: &cc::Module,
        layout: &PlannedLayout,
    ) -> Result<Self, Vec<BackendError>> {
        let live = ReachableHandles::from_module(physical).map_err(|failure| {
            vec![BackendError::invalid_ir(
                "P9 runtime projection",
                physical.span,
                failure.to_string(),
            )]
        })?;
        // The source error tag is planned once, before any provider is
        // checked, so every live `throw`/`catch` binding shares it. A module
        // that only uses `checkThrows` plans none and keeps its tag section
        // empty.
        let source_error = if bindings.iter().any(|binding| {
            binding.module == crate::bindings::RUNTIME_EXCEPTION_MODULE
                && live.direct_calls.contains(&binding.symbol)
                && crate::bindings::ExceptionOperation::from_export(&binding.function)
                    .is_some_and(crate::bindings::ExceptionOperation::uses_source_error_tag)
        }) {
            let payload = layout
                .value_type(&cc::ValueShape::String)
                .map_err(|error| {
                    vec![BackendError::invalid_ir(
                        "P9 runtime projection",
                        physical.span,
                        format!("source-error payload has no concrete layout: {error:?}"),
                    )]
                })?;
            Some((crate::mir::TagId(0), payload))
        } else {
            None
        };
        let mut imports = HashMap::new();
        let mut record_primitives = HashMap::new();
        let mut exception_providers = HashMap::new();
        for binding in bindings {
            if binding.module == crate::bindings::RUNTIME_EXCEPTION_MODULE {
                // The provider has no host ABI: plan_import plans no import.
                // P9 rechecks the CC shape here and the function lowerer
                // discharges every live call into an inline try region.
                let external = physical
                    .externals
                    .iter()
                    .find(|external| external.symbol == binding.symbol)
                    .and_then(|external| external.signature.as_ref())
                    .ok_or_else(|| {
                        binding_error(binding, "exception provider has no CC signature")
                    })?;
                if !live.direct_calls.contains(&binding.symbol) {
                    crate::bindings::runtime_cc_signature(
                        binding,
                        external,
                        &physical.representations,
                    )?;
                    continue;
                }
                let provider = crate::bindings::checked_provider(
                    binding,
                    external,
                    &physical.representations,
                    layout,
                    source_error,
                )?;
                exception_providers.insert(binding.symbol, provider);
                continue;
            }
            if binding.module == psrs_runtime::RECORD_STORAGE_MODULE {
                let primitive = psrs_runtime::RecordPrimitive::from_export(&binding.function)
                    .ok_or_else(|| binding_error(binding, "unknown record-storage export"))?;
                let external = physical
                    .externals
                    .iter()
                    .find(|external| external.symbol == binding.symbol)
                    .and_then(|external| external.signature.as_ref())
                    .ok_or_else(|| binding_error(binding, "record provider has no CC signature"))?;
                let canonical = crate::bindings::runtime_cc_signature(
                    binding,
                    external,
                    &physical.representations,
                )?;
                if &canonical != external {
                    return Err(binding_error(
                        binding,
                        "record provider CC signature is not canonical",
                    ));
                }
                record_primitives.insert(binding.symbol, primitive);
                if live.direct_calls.contains(&binding.symbol) {
                    return Err(binding_error(
                        binding,
                        "record provider bypasses its checked CC operation",
                    ));
                }
                continue;
            }
            if live.direct_calls.contains(&binding.symbol) {
                imports.insert(binding.symbol, plan_import(binding, physical, layout)?);
            }
        }
        validate_record_operations(physical, bindings, &record_primitives)?;
        let exception_tags = match source_error {
            Some((_, payload)) => vec![crate::mir::Tag {
                name: crate::bindings::SOURCE_ERROR_TAG_NAME.into(),
                payload: vec![payload],
                span: physical.span,
            }],
            None => Vec::new(),
        };
        Ok(Self {
            imports,
            record_primitives,
            exception_providers,
            exception_tags,
        })
    }
}

fn binding_error(binding: &RuntimeBinding, message: &str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P9 runtime projection", binding.span, message)
            .with_module(binding.source_module),
    ]
}

fn validate_record_operations(
    module: &cc::Module,
    bindings: &[RuntimeBinding],
    providers: &HashMap<SymbolId, psrs_runtime::RecordPrimitive>,
) -> Result<(), Vec<BackendError>> {
    let metadata = bindings
        .iter()
        .map(|binding| (binding.symbol, binding))
        .collect::<HashMap<_, _>>();
    for function in &module.functions {
        validate_assignments(&function.assignments, providers, &metadata)?;
    }
    Ok(())
}

fn validate_assignments(
    assignments: &[cc::Assignment],
    providers: &HashMap<SymbolId, psrs_runtime::RecordPrimitive>,
    metadata: &HashMap<SymbolId, &RuntimeBinding>,
) -> Result<(), Vec<BackendError>> {
    for assignment in assignments {
        match &assignment.kind {
            cc::AssignmentKind::RecordPrimitive {
                provider,
                record,
                operation,
            } => {
                let Some(primitive) = providers.get(provider) else {
                    let message = "CC record operation has no checked source provider";
                    let mut error =
                        BackendError::invalid_ir("P9 runtime projection", assignment.span, message);
                    if let Some(binding) = metadata.get(provider) {
                        error = error.with_module(binding.source_module);
                    }
                    return Err(vec![error]);
                };
                let valid = crate::bindings::operation_matches(*primitive, *record, operation);
                if !valid {
                    let message = "CC record operation disagrees with its checked provider export";
                    let mut error =
                        BackendError::invalid_ir("P9 runtime projection", assignment.span, message);
                    if let Some(binding) = metadata.get(provider) {
                        error = error.with_module(binding.source_module);
                    }
                    return Err(vec![error]);
                }
            }
            cc::AssignmentKind::Protect { body, handler, .. } => {
                validate_assignments(&body.assignments, providers, metadata)?;
                validate_assignments(&handler.assignments, providers, metadata)?;
            }
            cc::AssignmentKind::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                validate_assignments(then_assignments, providers, metadata)?;
                validate_assignments(else_assignments, providers, metadata)?;
            }
            cc::AssignmentKind::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                for case in cases {
                    validate_assignments(&case.assignments, providers, metadata)?;
                }
                validate_assignments(default_assignments, providers, metadata)?;
            }
            _ => {}
        }
    }
    Ok(())
}
