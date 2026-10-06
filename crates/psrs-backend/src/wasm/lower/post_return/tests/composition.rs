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
            artifacts: Vec::new(),
            memory: MemoryDemand {
                canonical_scratch: (0, crate::abi::SCRATCH_SIZE),
                allocator_state: (
                    crate::abi::HEAP_STATE,
                    crate::abi::HEAP_STATE + crate::abi::HEAP_STATE_SIZE,
                ),
                base_heap_start: crate::abi::HEAP_START,
                heap_alignment: crate::abi::MIN_BLOCK,
            },
        },
    )
    .map_err(|error| error.to_string())?;
    module.memories[0].minimum = plan.memory().minimum_pages;
    let core = crate::wasm::encode_module(&module).map_err(|error| format!("{error:?}"))?;
    psrs_linker::compose(&context, &plan, &core)
        .map(|artifact| artifact.bytes)
        .map_err(|errors| errors.to_string())
}
