//! Resolve live requirements against offered runtime units.
//!
//! Selection is metadata-first. Byte verification runs only after every live
//! requirement has one compatible provider and the state, growth, and memory
//! identities agree. A failed route does not try another unit or artifact kind.

mod select;

use crate::definitions::ResolvedWorldContext;
use crate::error::{LinkErrors, LinkStage};
use crate::plan::{InitializationStep, ProviderEdge, ResolvedBinding, SelectedUnit};
use crate::target::{
    ArtifactReference, BindingRequirement, Boundary, CoreSignature, ExportKind, ImportKind,
    Provider, RuntimeUnitOffer, TargetPolicy,
};
use crate::verify::{VerifiedArtifact, verify_artifact};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct ClosedProviders {
    pub bindings: Vec<ResolvedBinding>,
    pub units: Vec<SelectedUnit>,
    pub edges: Vec<ProviderEdge>,
    pub initialization: Vec<InitializationStep>,
    pub verified: BTreeMap<String, VerifiedArtifact>,
    pub artifacts: Vec<ArtifactReference>,
    pub growth_owner: String,
    pub shared_import: Option<(String, String)>,
}

pub(crate) fn close(
    context: &ResolvedWorldContext,
    requirements: &[BindingRequirement],
    units: &[RuntimeUnitOffer],
    policy: &TargetPolicy,
    growth_owner: &str,
) -> Result<ClosedProviders, LinkErrors> {
    let stage = LinkStage::Requirements;
    if growth_owner.is_empty() {
        return Err(LinkErrors::plain(stage, "missing growth owner"));
    }
    let mut requirement_ids = BTreeSet::new();
    for requirement in requirements {
        if !requirement_ids.insert(requirement.id) {
            return Err(LinkErrors::one(
                stage,
                &requirement.origin,
                "duplicate requirement identity",
            ));
        }
    }
    index_units(units)?;

    let mut selected = BTreeSet::new();
    let mut order = Vec::new();
    let mut stack = Vec::new();
    let mut edges = Vec::new();
    let mut bindings = Vec::new();
    let mut claims = BTreeMap::<(String, String), (CoreSignature, String)>::new();

    for requirement in requirements {
        match &requirement.provider {
            Provider::Generated => edges.push(ProviderEdge::Generated {
                requirement: requirement.id,
                origin: requirement.origin.clone(),
            }),
            Provider::HostInterface { interface } => select::bind_host(
                context,
                policy,
                requirement,
                interface,
                &mut claims,
                &mut bindings,
                &mut edges,
            )?,
            Provider::RuntimeOperation { name, version } => {
                let provider = select::resolve_operation(
                    units,
                    name,
                    version,
                    requirement.expected.as_ref(),
                    Some(&requirement.boundary),
                    &requirement.origin,
                )?;
                let unit = &units[provider];
                let operation = select::offered(unit, name, version).expect("resolved operation");
                let Boundary::RawCore { module, field } = &requirement.boundary else {
                    return Err(LinkErrors::one(
                        stage,
                        &requirement.origin,
                        "a runtime operation requires a raw-core boundary",
                    ));
                };
                select::claim(
                    &mut claims,
                    (module.clone(), field.clone()),
                    operation.signature.clone(),
                    &unit.id,
                )?;
                edges.push(ProviderEdge::Runtime {
                    requirement: requirement.id,
                    origin: requirement.origin.clone(),
                    operation: name.clone(),
                    version: version.clone(),
                    unit: unit.id.clone(),
                });
                bindings.push(ResolvedBinding {
                    requirement: requirement.id,
                    origin: requirement.origin.clone(),
                    module: module.clone(),
                    field: field.clone(),
                    signature: operation.signature.clone(),
                });
                close_unit(
                    units,
                    provider,
                    &mut selected,
                    &mut order,
                    &mut stack,
                    &mut edges,
                )?;
            }
        }
    }

    let growth = growth_owner.to_string();
    check_state(units, &order, &growth)?;
    let shared_import = shared_memory(units, &order)?;
    let verified = verify_selected(units, &order)?;
    let initialization = initialization(&verified.units);
    Ok(ClosedProviders {
        bindings,
        units: verified.units,
        edges,
        initialization,
        verified: verified.verified,
        artifacts: verified.artifacts,
        growth_owner: growth,
        shared_import,
    })
}

fn index_units(units: &[RuntimeUnitOffer]) -> Result<(), LinkErrors> {
    let stage = LinkStage::Requirements;
    let mut ids = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    for unit in units {
        if unit.id.is_empty() || unit.semantic_contract_version.is_empty() {
            return Err(LinkErrors::plain(stage, "runtime unit is missing identity"));
        }
        if !ids.insert(unit.id.clone()) {
            return Err(LinkErrors::one(stage, &unit.id, "duplicate runtime unit"));
        }
        if unit.artifact.contract.id.is_empty()
            || !artifacts.insert(unit.artifact.contract.id.clone())
        {
            return Err(LinkErrors::one(
                stage,
                &unit.id,
                "runtime unit is missing a unique artifact variant",
            ));
        }
        let mut operations = BTreeSet::new();
        for operation in &unit.provided {
            if operation.name.is_empty()
                || operation.version.is_empty()
                || operation.export.is_empty()
            {
                return Err(LinkErrors::one(
                    stage,
                    &unit.id,
                    "runtime operation is missing identity",
                ));
            }
            if !operations.insert((operation.name.clone(), operation.version.clone())) {
                return Err(LinkErrors::one(
                    stage,
                    &unit.id,
                    format!(
                        "unit advertises `{}@{}` more than once",
                        operation.name, operation.version
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn close_unit(
    units: &[RuntimeUnitOffer],
    index: usize,
    selected: &mut BTreeSet<usize>,
    order: &mut Vec<usize>,
    stack: &mut Vec<usize>,
    edges: &mut Vec<ProviderEdge>,
) -> Result<(), LinkErrors> {
    if selected.contains(&index) {
        return Ok(());
    }
    if stack.contains(&index) {
        return Err(LinkErrors::one(
            LinkStage::Requirements,
            &units[index].id,
            "unsupported provider cycle",
        ));
    }
    agree_with_contract(&units[index])?;
    stack.push(index);
    let required = units[index].required.clone();
    let from_unit = units[index].id.clone();
    for operation in required {
        let provider = select::resolve_operation(
            units,
            &operation.name,
            &operation.version,
            Some(&operation.signature),
            None,
            &from_unit,
        )?;
        edges.push(ProviderEdge::Dependency {
            from_unit: from_unit.clone(),
            operation: operation.name,
            version: operation.version,
            unit: units[provider].id.clone(),
        });
        close_unit(units, provider, selected, order, stack, edges)?;
    }
    stack.pop();
    selected.insert(index);
    order.push(index);
    Ok(())
}

fn agree_with_contract(unit: &RuntimeUnitOffer) -> Result<(), LinkErrors> {
    for operation in &unit.provided {
        let matches = unit.artifact.contract.exports.iter().any(|export| {
            export.kind == ExportKind::Func
                && export.name == operation.export
                && export.signature.as_ref() == Some(&operation.signature)
        });
        if !matches {
            return Err(LinkErrors::one(
                LinkStage::Requirements,
                &unit.id,
                format!(
                    "operation `{}` disagrees with the artifact contract",
                    operation.export
                ),
            ));
        }
    }
    Ok(())
}

fn check_state(
    units: &[RuntimeUnitOffer],
    order: &[usize],
    growth_owner: &str,
) -> Result<(), LinkErrors> {
    let stage = LinkStage::Requirements;
    let mut owners = BTreeMap::new();
    let mut growth = Vec::new();
    for index in order {
        let unit = &units[*index];
        if let Some(owner) = &unit.state_owner {
            if owner.is_empty() {
                return Err(LinkErrors::one(stage, &unit.id, "missing state owner"));
            }
            if owners.insert(owner.clone(), unit.id.clone()).is_some() {
                return Err(LinkErrors::one(
                    stage,
                    owner.clone(),
                    "duplicate state owner",
                ));
            }
        }
        // The demand names one growth owner. That owner may be the generated
        // allocator today or the allocator runtime unit that replaces it.
        // Any other selected unit that grows the same memory conflicts.
        if unit.grows_memory && unit.id != growth_owner {
            growth.push(unit.id.clone());
        }
    }
    if !growth.is_empty() {
        return Err(LinkErrors::one(
            stage,
            growth.join(","),
            format!("conflicting growth owners; `{growth_owner}` already owns growth"),
        ));
    }
    Ok(())
}

fn shared_memory(
    units: &[RuntimeUnitOffer],
    order: &[usize],
) -> Result<Option<(String, String)>, LinkErrors> {
    let mut shared = None;
    for index in order {
        let unit = &units[*index];
        let memories: Vec<_> = unit
            .artifact
            .contract
            .imports
            .iter()
            .filter(|import| matches!(import.kind, ImportKind::Memory { .. }))
            .collect();
        if memories.len() > 1 {
            return Err(LinkErrors::one(
                LinkStage::Memory,
                &unit.id,
                "unsupported memory profile",
            ));
        }
        if let Some(import) = memories.first() {
            let name = (import.module.clone(), import.field.clone());
            if shared.get_or_insert_with(|| name.clone()) != &name {
                return Err(LinkErrors::one(
                    LinkStage::Memory,
                    &unit.id,
                    "conflicting shared-memory imports",
                ));
            }
        }
    }
    Ok(shared)
}

struct VerifiedSelection {
    verified: BTreeMap<String, VerifiedArtifact>,
    artifacts: Vec<ArtifactReference>,
    units: Vec<SelectedUnit>,
}

fn verify_selected(
    units: &[RuntimeUnitOffer],
    order: &[usize],
) -> Result<VerifiedSelection, LinkErrors> {
    let mut verified = BTreeMap::new();
    let mut artifacts = Vec::new();
    let mut selected = Vec::new();
    for index in order {
        let unit = &units[*index];
        let id = unit.artifact.contract.id.clone();
        if !verified.contains_key(&id) {
            verified.insert(
                id.clone(),
                verify_artifact(&unit.artifact.contract, &unit.artifact.bytes)?,
            );
        }
        selected.push(SelectedUnit {
            id: unit.id.clone(),
            artifact: id,
            kind: unit.artifact.contract.kind,
            state_owner: unit.state_owner.clone(),
            grows_memory: unit.grows_memory,
            instantiate_after_shims: unit.artifact.contract.instantiate_after_shims,
        });
        artifacts.push(unit.artifact.clone());
    }
    Ok(VerifiedSelection {
        verified,
        artifacts,
        units: selected,
    })
}

fn initialization(units: &[SelectedUnit]) -> Vec<InitializationStep> {
    let mut steps = Vec::new();
    for unit in units.iter().filter(|unit| !unit.instantiate_after_shims) {
        steps.push(InitializationStep::Instantiate {
            unit: unit.id.clone(),
        });
    }
    steps.push(InitializationStep::ResolveShims);
    for unit in units.iter().filter(|unit| unit.instantiate_after_shims) {
        steps.push(InitializationStep::Instantiate {
            unit: unit.id.clone(),
        });
    }
    steps
}
