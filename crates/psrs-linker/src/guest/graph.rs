use super::{CheckedComponentPlan, ComponentLinkInput, ComponentReference, ResolvedGuestBinding};
use crate::{LinkErrors, LinkStage, LinkedArtifact};
use std::collections::{BTreeMap, BTreeSet};
use wasm_compose::graph::{Component, CompositionGraph, EncodeOptions};
use wasmparser::{ComponentTypeRef, Validator};

/// Close executable component dependencies without files, automatic discovery,
/// or a host fallback for an explicitly selected guest provider.
pub fn plan_components(input: ComponentLinkInput) -> Result<CheckedComponentPlan, LinkErrors> {
    build(input).map_err(|message| LinkErrors::plain(LinkStage::Requirements, message))
}

fn build(input: ComponentLinkInput) -> Result<CheckedComponentPlan, String> {
    let ComponentLinkInput {
        application,
        guests,
        bindings,
        policy,
        features,
    } = input;
    let mut references = BTreeMap::new();
    for reference in std::iter::once(application.clone()).chain(guests) {
        if references.insert(reference.id.clone(), reference).is_some() {
            return Err("duplicate component artifact identity".into());
        }
    }
    let mut selections = BTreeMap::new();
    for binding in bindings {
        if binding.export != binding.interface {
            return Err("guest exports must match the pinned canonical interface identity; implicit version adaptation is unsupported".into());
        }
        if selections
            .insert(binding.interface.clone(), binding)
            .is_some()
        {
            return Err("duplicate or conflicting guest interface provider".into());
        }
    }
    let mut graph = CompositionGraph::new();
    let mut validator = Validator::new_with_features(features);
    let mut instances = BTreeMap::new();
    let mut pending = vec![application.id.clone()];
    let mut used = BTreeSet::new();
    let mut edges = Vec::new();
    let mut external = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !used.insert(id.clone()) {
            continue;
        }
        let reference = references
            .get(&id)
            .ok_or_else(|| format!("absent selected guest artifact `{id}`"))?;
        verify_digest(reference)?;
        if id != application.id {
            // Provider initialization has no executable-start contract in v1.
            for payload in wasmparser::Parser::new(0).parse_all(&reference.bytes) {
                match payload.map_err(|error| error.to_string())? {
                    wasmparser::Payload::StartSection { .. }
                    | wasmparser::Payload::ComponentStartSection { .. } => {
                        return Err(format!(
                            "guest `{id}` has unsupported executable initialization"
                        ));
                    }
                    _ => {}
                }
            }
        }
        let component = Component::from_bytes(&mut validator, id.clone(), reference.bytes.clone())
            .map_err(|error| format!("component `{id}` is not executable: {error:#}"))?;
        let imports = component
            .imports()
            .map(|(index, name, ty)| (index, name.to_owned(), ty))
            .collect::<Vec<_>>();
        let component_id = graph
            .add_component(component)
            .map_err(|error| error.to_string())?;
        let instance = graph
            .instantiate(component_id)
            .map_err(|error| error.to_string())?;
        instances.insert(id.clone(), (component_id, instance));
        for (index, name, ty) in imports {
            if !matches!(ty, ComponentTypeRef::Instance(_)) {
                return Err(format!(
                    "component `{id}` imports unsupported non-interface `{name}`"
                ));
            }
            if let Some(binding) = selections.get(&name) {
                if binding.artifact == application.id {
                    return Err("the root application cannot provide a guest dependency".into());
                }
                pending.push(binding.artifact.clone());
                edges.push((
                    id.clone(),
                    index,
                    binding.artifact.clone(),
                    binding.export.clone(),
                ));
            } else if policy.permitted_host_interfaces.contains(&name) {
                external.insert(name);
            } else {
                return Err(format!(
                    "unresolved or disallowed component interface `{name}` required by `{id}`"
                ));
            }
        }
    }
    let mut selected_bindings = Vec::new();
    for (target, import, source, export) in edges {
        selected_bindings.push(ResolvedGuestBinding {
            consumer: target.clone(),
            interface: export.clone(),
            provider: source.clone(),
            export: export.clone(),
        });
        let (component_id, source_instance) = instances[&source];
        let target_instance = instances[&target].1;
        let export_index = graph
            .get_component(component_id)
            .unwrap()
            .export_by_name(&export)
            .map(|(index, _, _)| index)
            .ok_or_else(|| format!("guest `{source}` omits selected export `{export}`"))?;
        graph
            .connect(source_instance, Some(export_index), target_instance, import)
            .map_err(|error| {
                format!("incompatible guest binding `{source}.{export}`: {error:#}")
            })?;
    }
    // Graph encoding validates resource remapping, dependency ordering and all
    // canonical instance connections. Cycles are rejected, never made host imports.
    let bytes = graph
        .encode(EncodeOptions {
            define_components: true,
            export: Some(instances[&application.id].1),
            validate: true,
        })
        .map_err(|error| format!("component graph cannot be composed: {error:#}"))?;
    let actual = crate::compose::component_imports(&bytes).map_err(|error| error.to_string())?;
    let expected: Vec<_> = external.into_iter().collect();
    if actual != expected {
        return Err(format!(
            "component import closure differs from the plan: expected {expected:?}, actual {actual:?}"
        ));
    }
    let artifacts: Vec<_> = used
        .iter()
        .map(|id| (id.clone(), references[id].sha256.clone()))
        .collect();
    Validator::new_with_features(features)
        .validate_all(&bytes)
        .map_err(|error| error.to_string())?;
    Ok(CheckedComponentPlan {
        application_digest: application.sha256,
        bytes,
        artifacts,
        external_world: expected,
        bindings: selected_bindings,
        features,
    })
}

fn verify_digest(reference: &ComponentReference) -> Result<(), String> {
    if crate::sha256_hex(&reference.bytes) != reference.sha256 {
        return Err(format!(
            "component `{}` digest does not match its pin",
            reference.id
        ));
    }
    Ok(())
}

/// Execute a component plan only for the exact application it checked.
pub fn compose_component(
    plan: &CheckedComponentPlan,
    application: &[u8],
) -> Result<LinkedArtifact, LinkErrors> {
    let component = plan;
    if crate::sha256_hex(application) != component.application_digest {
        return Err(LinkErrors::plain(
            LinkStage::Compose,
            "application bytes differ from the checked component plan",
        ));
    }
    Ok(LinkedArtifact {
        bytes: component.bytes.clone(),
        external_world: plan.external_world().to_vec(),
    })
}
