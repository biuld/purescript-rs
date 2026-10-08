//! Choose the one compatible provider for an operation or host import.

use crate::definitions::ResolvedWorldContext;
use crate::error::{LinkErrors, LinkStage};
use crate::plan::{ProviderEdge, ResolvedBinding};
use crate::target::{
    ArtifactKind, BindingRequirement, Boundary, CoreSignature, RuntimeUnitOffer, TargetPolicy,
};
use std::collections::BTreeMap;

pub(super) fn resolve_operation(
    units: &[RuntimeUnitOffer],
    name: &str,
    version: &str,
    expected: Option<&CoreSignature>,
    boundary: Option<&Boundary>,
    origin: &str,
) -> Result<usize, LinkErrors> {
    let stage = LinkStage::Requirements;
    let Some(expected) = expected else {
        return Err(LinkErrors::one(
            stage,
            origin,
            format!("runtime operation `{name}@{version}` has no checked signature"),
        ));
    };
    let mut compatible = Vec::new();
    let mut boundary_mismatch = false;
    let mut kind_mismatch = false;
    for (index, unit) in units.iter().enumerate() {
        let Some(operation) = offered(unit, name, version) else {
            continue;
        };
        let signature_ok = operation.signature == *expected;
        let kind_ok = unit.artifact.contract.kind == ArtifactKind::CoreModule;
        let boundary_ok = match boundary {
            None => true,
            Some(Boundary::RawCore { module, field }) => {
                module == &unit.artifact.contract.module_name && field == &operation.export
            }
            Some(Boundary::ResolvedWit { .. }) => false,
        };
        if signature_ok && kind_ok && boundary_ok {
            compatible.push(index);
        } else if signature_ok && kind_ok {
            boundary_mismatch = true;
        } else if signature_ok {
            kind_mismatch = true;
        }
    }
    match compatible.len() {
        1 => Ok(compatible[0]),
        0 if !units
            .iter()
            .any(|unit| offered(unit, name, version).is_some()) =>
        {
            Err(LinkErrors::one(
                stage,
                origin,
                format!("no runtime unit provides `{name}@{version}`"),
            ))
        }
        0 if kind_mismatch && !boundary_mismatch => Err(LinkErrors::one(
            stage,
            origin,
            format!("incompatible artifact kind for `{name}@{version}`"),
        )),
        0 if boundary_mismatch => Err(LinkErrors::one(
            stage,
            origin,
            "artifact boundary does not name the selected export",
        )),
        0 => Err(LinkErrors::one(
            stage,
            origin,
            "runtime operation signature disagrees with the checked requirement",
        )),
        _ => Err(LinkErrors::one(
            stage,
            origin,
            format!("ambiguous providers for `{name}@{version}`"),
        )),
    }
}

pub(super) fn offered<'a>(
    unit: &'a RuntimeUnitOffer,
    name: &str,
    version: &str,
) -> Option<&'a crate::target::OfferedOperation> {
    unit.provided
        .iter()
        .find(|operation| operation.name == name && operation.version == version)
}

pub(super) fn bind_host(
    context: &ResolvedWorldContext,
    policy: &TargetPolicy,
    requirement: &BindingRequirement,
    interface: &str,
    claims: &mut BTreeMap<(String, String), (CoreSignature, String)>,
    bindings: &mut Vec<ResolvedBinding>,
    edges: &mut Vec<ProviderEdge>,
) -> Result<(), LinkErrors> {
    let stage = LinkStage::Requirements;
    if !context.imports_interface(interface) {
        return Err(LinkErrors::one(
            stage,
            &requirement.origin,
            format!("host interface `{interface}` is outside the resolved world"),
        ));
    }
    if !policy
        .permitted_host_interfaces
        .iter()
        .any(|permitted| permitted == interface)
    {
        return Err(LinkErrors::one(
            stage,
            &requirement.origin,
            format!("host interface `{interface}` is not permitted by the selected target profile"),
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
    claim(
        claims,
        (module.clone(), field.clone()),
        expected.clone(),
        interface,
    )?;
    edges.push(ProviderEdge::Host {
        requirement: requirement.id,
        origin: requirement.origin.clone(),
        interface: interface.to_string(),
    });
    bindings.push(ResolvedBinding {
        requirement: requirement.id,
        origin: requirement.origin.clone(),
        module,
        field,
        signature: expected,
    });
    Ok(())
}

pub(super) fn claim(
    claims: &mut BTreeMap<(String, String), (CoreSignature, String)>,
    key: (String, String),
    signature: CoreSignature,
    provider: &str,
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
    Ok(())
}
