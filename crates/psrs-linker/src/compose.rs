//! Component composition and final import-closure verification.

use crate::definitions::ResolvedWorldContext;
use crate::error::{LinkErrors, LinkStage};
use crate::plan::CheckedLinkPlan;
use wasmparser::{Parser, Payload};
use wit_component::{ComponentEncoder, LibraryInfo, StringEncoding, embed_component_metadata};

/// The final composed component and the host interfaces it still imports.
#[derive(Clone, Debug)]
pub struct LinkedArtifact {
    pub bytes: Vec<u8>,
    pub external_world: Vec<String>,
}

/// Composes an encoded application with the plan's verified libraries.
///
/// The final component's unresolved imports must equal the plan's
/// external world; the private runtime import must be closed.
pub fn compose(
    context: &ResolvedWorldContext,
    plan: &CheckedLinkPlan,
    application: &[u8],
) -> Result<LinkedArtifact, LinkErrors> {
    let stage = LinkStage::Compose;
    if !plan.uses_context(context) {
        return Err(LinkErrors::plain(
            stage,
            "WIT context differs from the checked plan",
        ));
    }
    let world = context.composition_world().ok_or_else(|| {
        LinkErrors::plain(
            stage,
            "definition-only context cannot compose an executable component",
        )
    })?;
    crate::application::verify(plan, application)?;
    let mut bytes = application.to_vec();
    embed_component_metadata(&mut bytes, context.resolve(), world, StringEncoding::UTF8).map_err(
        |error| {
            LinkErrors::plain(
                stage,
                format!("failed to embed component metadata: {error}"),
            )
        },
    )?;
    let mut encoder = ComponentEncoder::default()
        .module(&bytes)
        .map_err(|error| {
            LinkErrors::plain(stage, format!("failed to read the core module: {error}"))
        })?;
    for artifact in plan.artifacts() {
        encoder = encoder
            .library(
                &artifact.module_name,
                &artifact.bytes,
                LibraryInfo {
                    instantiate_after_shims: artifact.instantiate_after_shims,
                    arguments: Vec::new(),
                },
            )
            .map_err(|error| {
                LinkErrors::one(
                    stage,
                    &artifact.id,
                    format!("failed to attach artifact: {error:#}"),
                )
            })?;
    }
    let output = encoder.validate(true).encode().map_err(|error| {
        LinkErrors::plain(stage, format!("failed to encode the component: {error:#}"))
    })?;
    wasmparser::Validator::new()
        .validate_all(&output)
        .map_err(|error| {
            LinkErrors::plain(
                stage,
                format!("composed component failed validation: {error}"),
            )
        })?;

    let imports = component_imports(&output)?;
    for import in &imports {
        if import == psrs_runtime::MODULE_NAME {
            return Err(LinkErrors::plain(
                stage,
                "the private runtime import was not closed inside the component",
            ));
        }
        // Interface imports are named by canonical id and must be permitted by
        // the world; type and function imports belong to those interfaces.
        if !plan.external_world().contains(import) {
            return Err(LinkErrors::one(
                stage,
                import.clone(),
                "component imports an interface outside the permitted external world",
            ));
        }
    }
    if imports != plan.external_world() {
        return Err(LinkErrors::plain(
            stage,
            format!(
                "component import closure differs from the checked plan: expected {:?}, actual {imports:?}",
                plan.external_world()
            ),
        ));
    }
    Ok(LinkedArtifact {
        bytes: output,
        external_world: imports,
    })
}

pub(crate) fn component_imports(bytes: &[u8]) -> Result<Vec<String>, LinkErrors> {
    let mut imports = Vec::new();
    let mut depth = 0_usize;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|error| {
            LinkErrors::plain(
                LinkStage::Compose,
                format!("failed to parse component: {error}"),
            )
        })? {
            Payload::ComponentSection { .. } | Payload::ModuleSection { .. } => depth += 1,
            Payload::End(_) => depth = depth.saturating_sub(1),
            Payload::ComponentImportSection(reader) if depth == 0 => {
                // Only the outermost component's import section lists
                // unresolved host imports; nested shims have their own.
                for import in reader {
                    let import = import.map_err(|error| {
                        LinkErrors::plain(
                            LinkStage::Compose,
                            format!("failed to read component import: {error}"),
                        )
                    })?;
                    imports.push(import.name.0.to_string());
                }
            }
            _ => {}
        }
    }
    imports.sort();
    imports.dedup();
    Ok(imports)
}
