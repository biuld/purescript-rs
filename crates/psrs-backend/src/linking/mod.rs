//! Checked IR-to-linker requests and diagnostic mapping.
//!
//! The backend converts its optimized MIR imports and target capability
//! profile into linker-owned target records, runs the independent planner, and
//! attaches language diagnostics to any structured link failure. The resulting
//! plan is consumed by both Wasm emission and component assembly.

use crate::abi::{self, WasiRegistry, names};
use crate::mir;
use crate::target_runtime;
use crate::{BackendError, TargetCapabilities};
use psrs_hir::{ModuleId, SymbolId};
use psrs_linker::{
    ArtifactReference, BindingRequirement, Boundary, CheckedLinkPlan, CoreSignature, CoreType,
    MemoryDemand, Provider, RequirementId, ResolvedWorldContext, TargetLinkInput, TargetPolicy,
};
use psrs_span::TextRange;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

/// A checked plan together with the backend's requirement identity mappings.
pub(crate) struct LinkPlan {
    pub context: Arc<ResolvedWorldContext>,
    pub plan: CheckedLinkPlan,
    /// The planned core import `(module, field)` for each MIR import symbol.
    pub imports: HashMap<SymbolId, (String, String)>,
}

/// The default resolved-world context, parsed once per process.
pub(crate) fn default_context() -> Result<Arc<ResolvedWorldContext>, Vec<BackendError>> {
    static CONTEXT: OnceLock<Result<Arc<ResolvedWorldContext>, String>> = OnceLock::new();
    match CONTEXT.get_or_init(|| {
        psrs_linker::resolve_default_definitions()
            .map(Arc::new)
            .map_err(|error| error.to_string())
    }) {
        Ok(context) => Ok(Arc::clone(context)),
        Err(message) => Err(definitions_error(message)),
    }
}

fn definitions_error(message: &str) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 target definitions",
        TextRange::new(0, 0),
        message,
    )]
}

/// Builds and checks the target link plan for an optimized MIR module.
pub(crate) fn plan_for_module(
    context: &Arc<ResolvedWorldContext>,
    module: &mir::Module,
    wasi: &mut WasiRegistry,
    target: TargetCapabilities,
) -> Result<LinkPlan, Vec<BackendError>> {
    let mut requirements = Vec::new();
    let mut symbols: Vec<(SymbolId, RequirementId)> = Vec::new();
    let mut spans: HashMap<String, (TextRange, Option<ModuleId>)> = HashMap::new();
    let mut artifacts: BTreeMap<String, ArtifactReference> = BTreeMap::new();
    let mut next = 0_u32;
    let owner = module.entry.map(|entry| entry.module);

    for import in &module.imports {
        let id = RequirementId(next);
        next += 1;
        // Generated helpers are roots even though no source foreign declaration
        // names them; they are lowered locally and need no external provider.
        if let Some(name) = local_symbol_name(import.symbol) {
            let requirement = BindingRequirement {
                id,
                origin: format!("generated.{name}"),
                boundary: Boundary::RawCore {
                    module: "generated".into(),
                    field: name.into(),
                },
                expected: None,
                provider: Provider::Generated,
            };
            spans.insert(requirement.origin.clone(), (module.span, owner));
            requirements.push(requirement);
            continue;
        }
        if let Some(implementation) = target_runtime::for_symbol(import.symbol) {
            let requirement = implementation.requirement(id);
            artifacts
                .entry(implementation.artifact.id.to_string())
                .or_insert_with(|| implementation.artifact_reference());
            spans.insert(requirement.origin.clone(), (module.span, owner));
            requirements.push(requirement);
            symbols.push((import.symbol, id));
        } else if let Some((interface, field)) = wasi.symbol_name(import.symbol) {
            let interface = interface.to_string();
            let field = field.to_string();
            let signature = core_signature(import, module.span)?;
            let requirement = BindingRequirement {
                id,
                origin: format!("{interface}.{field}"),
                boundary: Boundary::ResolvedWit {
                    interface: interface.clone(),
                    function: field,
                },
                expected: Some(signature),
                provider: Provider::HostInterface { interface },
            };
            spans.insert(requirement.origin.clone(), (module.span, owner));
            requirements.push(requirement);
            symbols.push((import.symbol, id));
        } else {
            return Err(vec![BackendError::new(
                "P9 target linking",
                module.span,
                "a MIR import symbol has no selected provider",
            )]);
        }
    }

    // The synthesized command entry exits through WASI; the plan owns that
    // capability even though no source foreign declaration names it.
    if target.wasi_cli && module.entry.is_some() {
        let exit = wasi
            .import(names::EXIT, names::EXIT_WITH_CODE)
            .map_err(|message| {
                vec![BackendError::new("P9 target linking", module.span, message)]
            })?;
        let id = RequirementId(next);
        let requirement = BindingRequirement {
            id,
            origin: format!("{}.{}", exit.module, exit.name),
            boundary: Boundary::ResolvedWit {
                interface: exit.module.clone(),
                function: exit.name.clone(),
            },
            expected: Some(CoreSignature {
                parameters: exit
                    .parameters
                    .iter()
                    .copied()
                    .map(core_value_type)
                    .collect(),
                result: exit.result.map(core_value_type),
            }),
            provider: Provider::HostInterface {
                interface: exit.module.clone(),
            },
        };
        spans.insert(requirement.origin.clone(), (module.span, owner));
        requirements.push(requirement);
        symbols.push((exit.symbol, id));
    }

    let input = TargetLinkInput {
        requirements,
        artifacts: artifacts.into_values().collect(),
        policy: TargetPolicy {
            permitted_host_interfaces: permitted_host_interfaces(context, target),
        },
        memory: memory_demand(),
    };
    let plan =
        psrs_linker::plan(context, input).map_err(|errors| map_link_errors(&errors, &spans))?;

    let imports = symbols
        .into_iter()
        .filter_map(|(symbol, id)| {
            plan.import(id)
                .map(|binding| (symbol, (binding.module.clone(), binding.field.clone())))
        })
        .collect();

    Ok(LinkPlan {
        context: Arc::clone(context),
        plan,
        imports,
    })
}

/// Assembles the component from a checked plan.
pub(crate) fn compose(
    link: &LinkPlan,
    application: &[u8],
    span: TextRange,
    owner: Option<ModuleId>,
) -> Result<Vec<u8>, Vec<BackendError>> {
    psrs_linker::compose(&link.context, &link.plan, application)
        .map(|artifact| artifact.bytes)
        .map_err(|errors| {
            errors
                .0
                .into_iter()
                .map(|error| {
                    attach(
                        BackendError::new("P11 component", span, error.to_string()),
                        owner,
                    )
                })
                .collect()
        })
}

fn local_symbol_name(symbol: SymbolId) -> Option<&'static str> {
    match symbol {
        abi::REALLOC_SYMBOL => Some("realloc"),
        abi::STRING_TO_BYTES_SYMBOL => Some("string_to_bytes"),
        abi::BYTES_TO_STRING_SYMBOL => Some("bytes_to_string"),
        abi::VALIDATE_STEP_SYMBOL => Some("validate_step"),
        _ => None,
    }
}

fn core_signature(
    import: &mir::Import,
    span: TextRange,
) -> Result<CoreSignature, Vec<BackendError>> {
    let mut parameters = Vec::with_capacity(import.parameters.len());
    for ty in &import.parameters {
        let Some(ty) = core_value_type_opt(*ty) else {
            return Err(vec![BackendError::invalid_ir(
                "P9 target linking",
                span,
                "a host import has a non-scalar canonical parameter",
            )]);
        };
        parameters.push(ty);
    }
    let result = match import.result {
        Some(ty) => Some(core_value_type_opt(ty).ok_or_else(|| {
            vec![BackendError::invalid_ir(
                "P9 target linking",
                span,
                "a host import has a non-scalar canonical result",
            )]
        })?),
        None => None,
    };
    Ok(CoreSignature { parameters, result })
}

fn core_value_type(ty: crate::types::ValueType) -> CoreType {
    core_value_type_opt(ty).expect("a canonical ABI import is scalar")
}

fn core_value_type_opt(ty: crate::types::ValueType) -> Option<CoreType> {
    match ty {
        crate::types::ValueType::I32 | crate::types::ValueType::Boolean => Some(CoreType::I32),
        crate::types::ValueType::I64 => Some(CoreType::I64),
        crate::types::ValueType::F32 => Some(CoreType::F32),
        crate::types::ValueType::F64 => Some(CoreType::F64),
        crate::types::ValueType::Ref(_) => None,
    }
}

fn memory_demand() -> MemoryDemand {
    MemoryDemand {
        canonical_scratch: (abi::PRINT_SCRATCH as u32, abi::SCRATCH_END),
        allocator_state: (abi::HEAP_STATE, abi::HEAP_STATE + abi::HEAP_STATE_SIZE),
        base_heap_start: abi::HEAP_START,
        heap_alignment: abi::MIN_BLOCK,
    }
}

/// The canonical ids the selected target profile permits: the world's import
/// interfaces, with the capability-family gate applied to WASI services only.
fn permitted_host_interfaces(
    context: &ResolvedWorldContext,
    target: TargetCapabilities,
) -> Vec<String> {
    context
        .world_imports()
        .iter()
        .filter(|interface| {
            !interface.starts_with("wasi:") || abi::wasi_interface_enabled(target, interface)
        })
        .cloned()
        .collect()
}

fn map_link_errors(
    errors: &psrs_linker::LinkErrors,
    spans: &HashMap<String, (TextRange, Option<ModuleId>)>,
) -> Vec<BackendError> {
    errors
        .0
        .iter()
        .map(|error| {
            let (span, owner) = error
                .subject
                .as_ref()
                .and_then(|subject| spans.get(subject))
                .copied()
                .unwrap_or((TextRange::new(0, 0), None));
            attach(
                BackendError::new("P11 target linking", span, error.to_string()),
                owner,
            )
        })
        .collect()
}

fn attach(error: BackendError, owner: Option<ModuleId>) -> BackendError {
    match owner {
        Some(module) => error.with_module(module),
        None => error,
    }
}
