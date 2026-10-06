//! Provider closure, artifact verification, and memory/instantiation planning.

use crate::definitions::ResolvedWorldContext;
use crate::error::{LinkErrors, LinkStage};
use crate::target::{
    ArtifactContract, ArtifactKind, Boundary, CoreSignature, Provider, RequirementId,
    StorageRegion, TargetLinkInput,
};
use crate::verify::{VerifiedArtifact, verify_artifact};
use std::collections::{BTreeMap, BTreeSet};

/// A requirement bound to its verified provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBinding {
    pub requirement: RequirementId,
    pub origin: String,
    pub module: String,
    pub field: String,
    pub signature: CoreSignature,
}

/// The checked memory ownership and allocator boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryPlan {
    pub heap_start: u32,
    pub heap_alignment: u32,
    pub minimum_pages: u64,
    /// Every owned region, sorted by start address.
    pub reservations: Vec<StorageRegion>,
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
    memory: MemoryPlan,
    external_world: Vec<String>,
    digests: Vec<String>,
}

impl CheckedLinkPlan {
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
}

/// Plans a checked link from a resolved world and target input.
pub fn plan(
    context: &ResolvedWorldContext,
    input: TargetLinkInput,
) -> Result<CheckedLinkPlan, LinkErrors> {
    let TargetLinkInput {
        requirements,
        artifacts: artifact_refs,
        memory: memory_demand,
        policy,
    } = input;
    let stage = LinkStage::Requirements;
    let mut errors = Vec::new();

    let mut requirement_ids = BTreeSet::new();
    for requirement in &requirements {
        if !requirement_ids.insert(requirement.id) {
            errors.push(crate::error::LinkError {
                stage,
                subject: Some(requirement.origin.clone()),
                message: "duplicate requirement identity".into(),
            });
        }
    }

    let mut artifacts: BTreeMap<String, crate::target::ArtifactReference> = BTreeMap::new();
    for artifact in &artifact_refs {
        let id = artifact.contract.id.clone();
        if artifacts.insert(id.clone(), artifact.clone()).is_some() {
            errors.push(crate::error::LinkError {
                stage,
                subject: Some(id),
                message: "duplicate artifact identity".into(),
            });
        }
    }
    if !errors.is_empty() {
        return Err(LinkErrors::new(errors));
    }

    let mut verified: BTreeMap<String, VerifiedArtifact> = BTreeMap::new();
    let mut used_artifacts = BTreeSet::new();
    let mut external = BTreeSet::new();
    // A core import identity resolves to exactly one provider and signature.
    let mut claims = BTreeMap::<(String, String), (CoreSignature, String)>::new();
    let mut bindings = Vec::new();

    for requirement in &requirements {
        match &requirement.provider {
            Provider::Generated => {}
            Provider::ArtifactExport {
                artifact,
                export,
                signature,
            } => {
                let reference = artifacts.get(artifact).ok_or_else(|| {
                    LinkErrors::one(
                        stage,
                        &requirement.origin,
                        format!("absent artifact `{artifact}`"),
                    )
                })?;
                let Boundary::RawCore { module, field } = &requirement.boundary else {
                    return Err(LinkErrors::one(
                        stage,
                        &requirement.origin,
                        "an artifact export requires a raw-core boundary",
                    ));
                };
                if module != &reference.contract.module_name || field != export {
                    return Err(LinkErrors::one(
                        stage,
                        &requirement.origin,
                        "artifact boundary does not name the selected export",
                    ));
                }
                let declared = artifact_export(&reference.contract, export).ok_or_else(|| {
                    LinkErrors::one(
                        stage,
                        &requirement.origin,
                        format!("artifact `{artifact}` does not declare export `{export}`"),
                    )
                })?;
                if declared != *signature || requirement.expected.as_ref() != Some(signature) {
                    return Err(LinkErrors::one(
                        stage,
                        &requirement.origin,
                        "artifact export signature disagrees with the checked requirement",
                    ));
                }
                if !verified.contains_key(artifact) {
                    let value = verify_artifact(&reference.contract, &reference.bytes)?;
                    verified.insert(artifact.clone(), value);
                }
                used_artifacts.insert(artifact.clone());
                claim(
                    &mut claims,
                    (module.clone(), field.clone()),
                    signature.clone(),
                    artifact,
                    &mut errors,
                )?;
                bindings.push(ResolvedBinding {
                    requirement: requirement.id,
                    origin: requirement.origin.clone(),
                    module: module.clone(),
                    field: field.clone(),
                    signature: signature.clone(),
                });
            }
            Provider::HostInterface { interface } => {
                if !policy
                    .permitted_host_interfaces
                    .iter()
                    .any(|permitted| permitted == interface)
                {
                    return Err(LinkErrors::one(
                        stage,
                        &requirement.origin,
                        format!(
                            "host interface `{interface}` is not permitted by the selected target profile"
                        ),
                    ));
                }
                let Some(expected) = requirement.expected.clone() else {
                    return Err(LinkErrors::one(
                        stage,
                        &requirement.origin,
                        "a host binding requires a checked raw signature",
                    ));
                };
                let (module, field) = match &requirement.boundary {
                    Boundary::ResolvedWit {
                        interface: name,
                        function,
                    } if name == interface => (name.clone(), function.clone()),
                    _ => {
                        return Err(LinkErrors::one(
                            stage,
                            &requirement.origin,
                            "host binding must cross the selected WIT interface",
                        ));
                    }
                };
                external.insert(interface.clone());
                claim(
                    &mut claims,
                    (module.clone(), field.clone()),
                    expected.clone(),
                    interface,
                    &mut errors,
                )?;
                bindings.push(ResolvedBinding {
                    requirement: requirement.id,
                    origin: requirement.origin.clone(),
                    module,
                    field,
                    signature: expected,
                });
            }
        }
    }
    if !errors.is_empty() {
        return Err(LinkErrors::new(errors));
    }

    let memory = plan_memory(&artifact_refs, &memory_demand, &used_artifacts, &verified)?;

    let mut digests = verified
        .values()
        .map(|value| value.sha256.clone())
        .collect::<Vec<_>>();
    digests.sort();
    digests.dedup();

    Ok(CheckedLinkPlan {
        bindings,
        artifacts: verified.into_values().collect(),
        memory,
        external_world: context.host_closure(&external),
        digests,
    })
}

fn artifact_export(contract: &ArtifactContract, export: &str) -> Option<CoreSignature> {
    contract.exports.iter().find_map(|declared| {
        (declared.name == export && declared.kind == crate::target::ExportKind::Func)
            .then(|| declared.signature.clone())
            .flatten()
    })
}

fn claim(
    claims: &mut BTreeMap<(String, String), (CoreSignature, String)>,
    key: (String, String),
    signature: CoreSignature,
    provider: &str,
    errors: &mut Vec<crate::error::LinkError>,
) -> Result<(), LinkErrors> {
    if let Some((existing, existing_provider)) = claims.get(&key) {
        if existing != &signature || existing_provider != provider {
            return Err(LinkErrors::one(
                LinkStage::Requirements,
                format!("{}.{}", key.0, key.1),
                "ambiguous or conflicting providers for one import identity",
            ));
        }
        return Ok(());
    }
    claims.insert(key, (signature, provider.to_string()));
    let _ = errors;
    Ok(())
}

fn plan_memory(
    artifacts: &[crate::target::ArtifactReference],
    demand: &crate::target::MemoryDemand,
    used_artifacts: &BTreeSet<String>,
    verified: &BTreeMap<String, VerifiedArtifact>,
) -> Result<MemoryPlan, LinkErrors> {
    let stage = LinkStage::Memory;
    let mut reservations = vec![
        region("canonical-scratch", demand.canonical_scratch),
        region("allocator-state", demand.allocator_state),
    ];
    let mut heap_start = demand.base_heap_start;
    let mut minimum_pages = u64::from(heap_start).div_ceil(0x1_0000) + 1;

    for id in used_artifacts {
        let artifact = verified.get(id).expect("a used artifact is verified");
        let contract = artifacts
            .iter()
            .find(|reference| &reference.contract.id == id)
            .map(|reference| &reference.contract)
            .ok_or_else(|| LinkErrors::one(stage, id, "verified artifact has no contract"))?;
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
        let _ = artifact;
    }

    reservations.sort_by_key(|region| (region.start, region.end));
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
    if alignment == 0 || !heap_start.is_multiple_of(alignment) {
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
    if u64::from(heap_start) > minimum_pages * 0x1_0000 {
        return Err(LinkErrors::plain(
            stage,
            "declared minimum pages do not cover the allocator boundary",
        ));
    }
    // The allocator does not grow memory; it needs at least one page of heap
    // beyond the boundary where every reserved region ends.
    minimum_pages = minimum_pages.max(u64::from(heap_start).div_ceil(0x1_0000) + 1);

    Ok(MemoryPlan {
        heap_start,
        heap_alignment: alignment,
        minimum_pages,
        reservations,
    })
}

fn region(owner: &str, range: (u32, u32)) -> StorageRegion {
    StorageRegion {
        owner: owner.to_string(),
        start: range.0,
        end: range.1,
    }
}
