//! Compiler-owned raw Core instance assembly, before canonical WIT boundaries.
use crate::{CheckedLinkPlan, LinkErrors, LinkStage};
use std::collections::{BTreeMap, BTreeSet};
use wasm_encoder::{ComponentBuilder, ModuleArg};
use wasmparser::{Parser, Payload};

/// A provisional instance graph connecting original modules directly.
/// Canonical lift/lower and target-world closure remain separate obligations.
pub struct CoreAssembly {
    component: ComponentBuilder,
    application: u32,
    application_module: u32,
}

impl CoreAssembly {
    pub fn application_instance(&self) -> u32 {
        self.application
    }

    pub fn application_module(&self) -> u32 {
        self.application_module
    }

    /// Consume the assembly to add canonical boundaries. This returns an encoder,
    /// not a checked target artifact; callers must validate their completed graph.
    pub fn into_encoder(self) -> ComponentBuilder {
        self.component
    }
}

/// Provider instances and the original application module, before host
/// canonical lowering supplies application imports.
pub(crate) struct PreparedCore {
    pub component: ComponentBuilder,
    pub application_module: u32,
    pub providers: BTreeMap<String, u32>,
}

pub(crate) fn prepare_core(
    plan: &CheckedLinkPlan,
    application: &[u8],
    canonical_libraries: &BTreeSet<String>,
) -> Result<PreparedCore, LinkErrors> {
    crate::application::verify(plan, application)?;
    // Every occurrence has been checked against its complete provider type.
    // Components require unique import names, unlike standalone Core modules.
    let application = crate::application::imports::normalize(application)
        .map_err(|message| LinkErrors::plain(LinkStage::Compose, message))?;
    let error = |message| LinkErrors::plain(LinkStage::Compose, message);
    let mut pending = plan
        .artifacts()
        .iter()
        .filter(|artifact| !canonical_libraries.contains(&artifact.module_name))
        .map(|artifact| {
            Ok((
                artifact.module_name.as_str(),
                artifact.bytes.as_slice(),
                imports(&artifact.bytes)?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()
        .map_err(error)?;
    let mut component = ComponentBuilder::default();
    let mut providers = BTreeMap::new();
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .position(|(_, _, imports)| imports.iter().all(|name| providers.contains_key(name)));
        let Some(ready) = ready else {
            let dependencies = pending
                .iter()
                .map(|(name, _, imports)| {
                    let missing = imports
                        .iter()
                        .filter(|name| !providers.contains_key(*name))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{name} -> [{missing}]")
                })
                .collect::<Vec<_>>()
                .join("; ");
            return Err(error(format!(
                "raw provider graph has unresolved imports or cyclic provisioning: {dependencies}"
            )));
        };
        let (name, bytes, imports) = pending.remove(ready);
        if name == "__main_module__" || providers.contains_key(name) {
            return Err(error(
                "raw provider graph has duplicate or reserved module namespaces".into(),
            ));
        }
        let module = component.core_module_raw(Some(name), bytes);
        let args = imports
            .iter()
            .map(|name| (name.as_str(), ModuleArg::Instance(providers[name])))
            .collect::<Vec<_>>();
        let instance = component.core_instantiate(Some(name), module, args);
        providers.insert(name.to_string(), instance);
    }
    let application_module = component.core_module_raw(Some("__main_module__"), &application);
    Ok(PreparedCore {
        component,
        application_module,
        providers,
    })
}

/// Assemble a closed acyclic raw provider graph without scalar function shims.
/// Host canonical lowering uses `prepare_core` and supplies host instances
/// before application instantiation. Cyclic provisioning remains unsupported.
pub fn assemble_core(
    plan: &CheckedLinkPlan,
    application: &[u8],
) -> Result<CoreAssembly, LinkErrors> {
    if !plan.external_world().is_empty() {
        return Err(LinkErrors::plain(
            LinkStage::Compose,
            "raw instance assembly requires a closed provider graph",
        ));
    }
    let mut prepared = prepare_core(plan, application, &BTreeSet::new())?;
    let names =
        imports(application).map_err(|message| LinkErrors::plain(LinkStage::Compose, message))?;
    let args = names
        .iter()
        .map(|name| {
            prepared
                .providers
                .get(name)
                .map(|instance| (name.as_str(), ModuleArg::Instance(*instance)))
                .ok_or_else(|| {
                    LinkErrors::plain(
                        LinkStage::Compose,
                        "raw application has an unresolved import",
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let instance = prepared.component.core_instantiate(
        Some("__main_module__"),
        prepared.application_module,
        args,
    );
    Ok(CoreAssembly {
        component: prepared.component,
        application: instance,
        application_module: prepared.application_module,
    })
}

/// Reference interfaces and their consumers remain in original raw Core
/// modules. Scalar libraries retain the canonical encoder's checked memory
/// and application-shim provisioning instead of being instantiated too early.
pub(crate) fn canonical_libraries(plan: &CheckedLinkPlan) -> BTreeSet<String> {
    let references = |signature: &crate::CoreSignature| {
        signature
            .parameters
            .iter()
            .chain(signature.result.iter())
            .any(|ty| matches!(ty, crate::CoreType::Ref(_)))
    };
    let mut raw = plan
        .artifacts()
        .iter()
        .filter(|artifact| {
            artifact
                .exports
                .iter()
                .filter_map(|export| export.signature.as_ref())
                .any(references)
                || artifact
                    .imports
                    .iter()
                    .filter_map(|import| match &import.kind {
                        crate::ImportKind::Function(signature) => Some(signature),
                        _ => None,
                    })
                    .any(references)
        })
        .map(|artifact| artifact.module_name.clone())
        .collect::<BTreeSet<_>>();
    loop {
        let consumers = plan
            .artifacts()
            .iter()
            .filter(|artifact| {
                artifact
                    .imports
                    .iter()
                    .any(|import| raw.contains(&import.module))
            })
            .map(|artifact| artifact.module_name.clone())
            .collect::<Vec<_>>();
        let before = raw.len();
        raw.extend(consumers);
        if raw.len() == before {
            break;
        }
    }
    plan.artifacts()
        .iter()
        .filter(|artifact| !raw.contains(&artifact.module_name))
        .map(|artifact| artifact.module_name.clone())
        .collect()
}

fn imports(bytes: &[u8]) -> Result<BTreeSet<String>, String> {
    let mut imports = BTreeSet::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ImportSection(section) = payload.map_err(|error| error.to_string())? {
            for import in section.into_imports() {
                imports.insert(
                    import
                        .map_err(|error| error.to_string())?
                        .module
                        .to_string(),
                );
            }
        }
    }
    Ok(imports)
}
