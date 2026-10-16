//! Provider closure, artifact verification, and memory/instantiation planning.

use crate::definitions::ResolvedWorldContext;
use crate::error::{LinkErrors, LinkStage};
use crate::target::{ArtifactKind, CoreSignature, RequirementId, StorageRegion, TargetLinkInput};
use crate::verify::VerifiedArtifact;

/// A requirement bound to its verified provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBinding {
    pub requirement: RequirementId,
    pub origin: String,
    pub module: String,
    pub field: String,
    pub signature: CoreSignature,
}

/// The application's exported heap boundary and the provider import it satisfies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeapBoundary {
    /// Core-module name of the runtime unit that imports the boundary.
    pub provider_module: String,
    /// Import module, for example `__main_module__`.
    pub import_module: String,
    /// Import field and application export name.
    pub field: String,
}

/// The checked memory ownership and allocator boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryPlan {
    /// Identity of this shared memory. One plan owns one memory.
    pub memory_id: String,
    pub heap_start: u32,
    pub heap_alignment: u32,
    pub minimum_pages: u64,
    pub maximum_pages: u64,
    /// The only owner allowed to grow this memory.
    pub growth_owner: String,
    /// The core import every selected memory-using unit shares, when any does.
    pub shared_import: Option<(String, String)>,
    /// The constant heap-boundary getter the selected growth owner imports.
    ///
    /// `provider_module` is the owner's core-module name. The application
    /// exports `field`; composition aliases that export into `import_module`.
    pub heap_getter: Option<HeapBoundary>,
    /// Every owned region, sorted by start address.
    pub reservations: Vec<StorageRegion>,
    pub(crate) canonical_scratch: (u32, u32),
    pub(crate) allocator_state: (u32, u32),
}

/// A runtime unit selected by provider closure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedUnit {
    pub id: String,
    pub artifact: String,
    pub kind: crate::ArtifactKind,
    pub state_owner: Option<String>,
    pub grows_memory: bool,
    pub instantiate_after_shims: bool,
}

/// One edge in the checked provider graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderEdge {
    Generated {
        requirement: RequirementId,
        origin: String,
    },
    Runtime {
        requirement: RequirementId,
        origin: String,
        operation: String,
        version: String,
        unit: String,
    },
    /// A selected unit's required operation, closed to another unit.
    Dependency {
        from_unit: String,
        operation: String,
        version: String,
        unit: String,
    },
    Host {
        requirement: RequirementId,
        origin: String,
        interface: String,
    },
}

/// Checked instantiation order. Calls happen only after shim resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitializationStep {
    Instantiate { unit: String },
    ResolveShims,
}

/// The published identities and digests of one successful plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkReport {
    pub memory_id: String,
    pub growth_owner: String,
    pub units: Vec<String>,
    pub initialization: Vec<InitializationStep>,
    pub digests: Vec<String>,
    pub external_world: Vec<String>,
}

/// One immutable checked target link plan.
///
/// Construction is private to successful planning, so every live requirement
/// has exactly one verified provider and the memory and external world are
/// internally consistent.
#[derive(Clone, Debug)]
pub struct CheckedLinkPlan {
    bindings: Vec<ResolvedBinding>,
    artifacts: Vec<VerifiedArtifact>,
    units: Vec<SelectedUnit>,
    edges: Vec<ProviderEdge>,
    initialization: Vec<InitializationStep>,
    memory: MemoryPlan,
    external_world: Vec<String>,
    digests: Vec<String>,
    context: (
        std::sync::Arc<wit_parser::Resolve>,
        Option<wit_parser::WorldId>,
    ),
}

impl CheckedLinkPlan {
    pub(crate) fn uses_context(&self, context: &ResolvedWorldContext) -> bool {
        std::sync::Arc::ptr_eq(&self.context.0, &context.shared_resolve())
            && self.context.1 == context.composition_world()
    }
    /// The binding for a requirement, when it contributes a core import.
    pub fn import(&self, requirement: RequirementId) -> Option<&ResolvedBinding> {
        self.bindings
            .iter()
            .find(|binding| binding.requirement == requirement)
    }

    pub fn bindings(&self) -> &[ResolvedBinding] {
        &self.bindings
    }

    pub fn artifacts(&self) -> &[VerifiedArtifact] {
        &self.artifacts
    }

    pub fn memory(&self) -> &MemoryPlan {
        &self.memory
    }

    /// The permitted residual host interfaces, sorted.
    pub fn external_world(&self) -> &[String] {
        &self.external_world
    }

    pub fn digests(&self) -> &[String] {
        &self.digests
    }

    pub fn units(&self) -> &[SelectedUnit] {
        &self.units
    }

    pub fn edges(&self) -> &[ProviderEdge] {
        &self.edges
    }

    pub fn initialization(&self) -> &[InitializationStep] {
        &self.initialization
    }

    pub fn growth_owner(&self) -> &str {
        &self.memory.growth_owner
    }

    /// Identities, initialization order, and digests for this plan.
    pub fn report(&self) -> LinkReport {
        LinkReport {
            memory_id: self.memory.memory_id.clone(),
            growth_owner: self.memory.growth_owner.clone(),
            units: self.units.iter().map(|unit| unit.id.clone()).collect(),
            initialization: self.initialization.clone(),
            digests: self.digests.clone(),
            external_world: self.external_world.clone(),
        }
    }
}

/// Plans a checked link from a resolved world and target input.
pub fn plan(
    context: &ResolvedWorldContext,
    input: TargetLinkInput,
) -> Result<CheckedLinkPlan, LinkErrors> {
    let TargetLinkInput {
        requirements,
        units,
        memory: memory_demand,
        policy,
    } = input;
    let stage = LinkStage::Requirements;
    let closed = crate::graph::close(
        context,
        &requirements,
        &units,
        &policy,
        &memory_demand.growth_owner,
    )?;
    let memory = plan_memory(&closed, &memory_demand)?;

    let mut digests = closed
        .verified
        .values()
        .map(|value| value.sha256.clone())
        .collect::<Vec<_>>();
    digests.sort();
    digests.dedup();

    let external = crate::closure::host_imports(context, &closed.bindings)?;
    for interface in &external {
        if !context.imports_interface(interface)
            || !policy.permitted_host_interfaces.contains(interface)
        {
            return Err(LinkErrors::one(
                stage,
                interface,
                "component resource dependency is outside the selected world or target profile",
            ));
        }
    }
    Ok(CheckedLinkPlan {
        bindings: closed.bindings,
        artifacts: closed.verified.into_values().collect(),
        units: closed.units,
        edges: closed.edges,
        initialization: closed.initialization,
        memory,
        external_world: external,
        digests,
        context: (context.shared_resolve(), context.composition_world()),
    })
}

fn plan_memory(
    closed: &crate::graph::ClosedProviders,
    demand: &crate::target::MemoryDemand,
) -> Result<MemoryPlan, LinkErrors> {
    let stage = LinkStage::Memory;
    let maximum = demand.maximum_pages.unwrap_or(65536);
    if maximum == 0 || maximum > 65536 {
        return Err(LinkErrors::plain(stage, "unsupported memory maximum"));
    }
    if demand.canonical_scratch.0 > demand.canonical_scratch.1
        || demand.allocator_state.0 > demand.allocator_state.1
    {
        return Err(LinkErrors::plain(
            stage,
            "storage reservation has reversed bounds",
        ));
    }
    let mut reservations = Vec::new();
    if demand.canonical_scratch.0 < demand.canonical_scratch.1 {
        reservations.push(region("canonical-scratch", demand.canonical_scratch));
    }
    if demand.allocator_state.0 < demand.allocator_state.1 {
        reservations.push(region("allocator-state", demand.allocator_state));
    }
    let mut heap_start = demand.base_heap_start;
    let mut minimum_pages = u64::from(heap_start).div_ceil(0x1_0000) + 1;

    for reference in &closed.artifacts {
        let id = &reference.contract.id;
        let contract = &reference.contract;
        let Some(storage) = &contract.storage else {
            continue;
        };
        if contract.kind == ArtifactKind::CoreModule && storage.stack.end <= storage.stack.start {
            return Err(LinkErrors::one(
                stage,
                id,
                "artifact stack reservation is empty",
            ));
        }
        if storage.stack.end - storage.stack.start < storage.stack_bound_bytes {
            return Err(LinkErrors::one(
                stage,
                id,
                "artifact stack reservation is smaller than its reviewed bound",
            ));
        }
        reservations.push(StorageRegion {
            owner: id.clone(),
            start: storage.static_data.start,
            end: storage.static_data.end,
        });
        reservations.push(StorageRegion {
            owner: id.clone(),
            start: storage.stack.start,
            end: storage.stack.end,
        });
        heap_start = heap_start.max(storage.heap_start);
        minimum_pages = minimum_pages.max(storage.minimum_pages);
    }

    reservations.sort_by_key(|region| (region.start, region.end));
    if reservations.iter().any(|region| region.start > region.end) {
        return Err(LinkErrors::plain(
            stage,
            "storage reservation has reversed bounds",
        ));
    }
    for pair in reservations.windows(2) {
        if pair[1].start < pair[0].end {
            return Err(LinkErrors::one(
                stage,
                pair[1].owner.clone(),
                format!(
                    "reservation [{}, {}) overlaps `{}` at [{}, {})",
                    pair[1].start, pair[1].end, pair[0].owner, pair[0].start, pair[0].end
                ),
            ));
        }
    }
    let alignment = demand.heap_alignment;
    if !alignment.is_power_of_two() || !heap_start.is_multiple_of(alignment) {
        return Err(LinkErrors::plain(
            stage,
            "allocator boundary is not aligned to its block granularity",
        ));
    }
    if let Some(last) = reservations.last()
        && last.end > heap_start
    {
        return Err(LinkErrors::one(
            stage,
            last.owner.clone(),
            "a reservation extends past the allocator boundary",
        ));
    }
    if minimum_pages > maximum {
        return Err(LinkErrors::plain(
            stage,
            "memory plan exceeds its declared maximum",
        ));
    }
    if u64::from(heap_start) > minimum_pages * 0x1_0000 {
        return Err(LinkErrors::plain(
            stage,
            "declared minimum pages do not cover the allocator boundary",
        ));
    }
    // The allocator grows memory on demand; reserve one initial page of heap
    // beyond the boundary where every reserved region ends.
    minimum_pages = minimum_pages.max(u64::from(heap_start).div_ceil(0x1_0000) + 1);
    if minimum_pages > maximum {
        return Err(LinkErrors::plain(
            stage,
            "memory plan exceeds its declared maximum",
        ));
    }

    Ok(MemoryPlan {
        memory_id: crate::APPLICATION_MEMORY.to_string(),
        heap_start,
        heap_alignment: alignment,
        minimum_pages,
        maximum_pages: maximum,
        growth_owner: closed.growth_owner.clone(),
        shared_import: closed.shared_import.clone(),
        heap_getter: heap_boundary(closed, demand)?,
        reservations,
        canonical_scratch: demand.canonical_scratch,
        allocator_state: demand.allocator_state,
    })
}

fn heap_boundary(
    closed: &crate::graph::ClosedProviders,
    demand: &crate::target::MemoryDemand,
) -> Result<Option<HeapBoundary>, LinkErrors> {
    let Some(position) = closed
        .units
        .iter()
        .position(|unit| unit.id == demand.growth_owner)
    else {
        if closed
            .artifacts
            .iter()
            .any(|artifact| artifact.contract.imports.iter().any(is_heap_getter))
        {
            return Err(LinkErrors::plain(
                LinkStage::Memory,
                "heap getter imports require a planned runtime growth owner",
            ));
        }
        return Ok(None);
    };
    let stage = LinkStage::Memory;
    let unit = &closed.units[position];
    if !unit.grows_memory {
        return Err(LinkErrors::one(
            stage,
            &unit.id,
            "the named growth owner does not grow memory",
        ));
    }
    let contract = &closed.artifacts[position].contract;
    let functions: Vec<_> = contract
        .imports
        .iter()
        .filter(|import| matches!(import.kind, crate::ImportKind::Function(_)))
        .collect();
    let [import] = functions.as_slice() else {
        return Err(LinkErrors::one(
            stage,
            &unit.id,
            "the growth owner must import exactly one constant heap-boundary getter",
        ));
    };
    if !is_heap_getter(import) {
        return Err(LinkErrors::one(
            stage,
            &unit.id,
            "invalid application heap getter contract",
        ));
    }
    Ok(Some(HeapBoundary {
        provider_module: contract.module_name.clone(),
        import_module: import.module.clone(),
        field: import.field.clone(),
    }))
}

fn region(owner: &str, range: (u32, u32)) -> StorageRegion {
    StorageRegion {
        owner: owner.to_string(),
        start: range.0,
        end: range.1,
    }
}

/// The application-bound getter protocol; composition verifies its constant body.
pub(crate) fn is_heap_getter(import: &crate::DeclaredImport) -> bool {
    import.module == psrs_runtime::APPLICATION_MODULE
        && import.field == psrs_runtime::HEAP_BOUNDARY_IMPORT
        && import.kind
            == crate::ImportKind::Function(CoreSignature {
                parameters: Vec::new(),
                result: Some(crate::CoreType::I32),
            })
}
