use super::{Module, Resolve, WorldId};

/// Fixtures explicitly declare their imports, then emit the checked minimum.
pub(super) fn componentize(
    mut module: Module,
    resolve: Resolve,
    world: WorldId,
    requirements: Vec<psrs_linker::BindingRequirement>,
) -> Result<Vec<u8>, String> {
    use psrs_linker::{MemoryDemand, TargetLinkInput, TargetPolicy};
    let context = psrs_linker::ResolvedWorldContext::from_resolve(resolve, world);
    let plan = psrs_linker::plan(
        &context,
        TargetLinkInput {
            policy: TargetPolicy {
                permitted_host_interfaces: requirements
                    .iter()
                    .filter_map(|requirement| match &requirement.provider {
                        psrs_linker::Provider::HostInterface { interface } => {
                            Some(interface.clone())
                        }
                        _ => None,
                    })
                    .collect(),
            },
            requirements,
            units: psrs_linker::runtime::package_offers(&psrs_runtime::PSRS_RUNTIME).unwrap(),
            memory: MemoryDemand {
                canonical_scratch: (0, crate::abi::SCRATCH_SIZE),
                allocator_state: (crate::abi::HEAP_START, crate::abi::HEAP_START),
                base_heap_start: crate::abi::HEAP_START,
                heap_alignment: crate::abi::MIN_BLOCK,
                growth_owner: psrs_runtime::ALLOCATOR_UNIT.id.into(),
                maximum_pages: None,
            },
        },
    )
    .map_err(|error| error.to_string())?;
    module.memories[0].minimum = plan.memory().minimum_pages;
    if let Some(boundary) = plan.memory().heap_getter.as_ref() {
        let export = module
            .exports
            .iter()
            .find(|export| export.name == boundary.field)
            .ok_or_else(|| format!("missing heap export `{}`", boundary.field))?;
        let crate::wasm::ExportIndex::Function(_) = export.index else {
            return Err("the heap boundary export is not a function".into());
        };
        let getter = module
            .helpers
            .iter_mut()
            .find(|function| function.name == boundary.field)
            .ok_or("the heap boundary getter is missing")?;
        getter.body = vec![crate::wasm::Op::Leaf(wasm_encoder::Instruction::I32Const(
            plan.memory().heap_start as i32,
        ))];
    }
    let core = crate::wasm::encode_module(&module).map_err(|error| format!("{error:?}"))?;
    psrs_linker::compose(&context, &plan, &core)
        .map(|artifact| artifact.bytes)
        .map_err(|errors| errors.to_string())
}
