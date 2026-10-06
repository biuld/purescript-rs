//! WIT definition loading and the immutable resolved-world context.
//!
//! One resolved context is shared by backend ABI lowering and linker provider
//! validation. It is held behind an `Arc` so the backend can borrow it without
//! parsing a second copy or regenerating interface identities.

use crate::error::{LinkErrors, LinkStage};
use psrs_runtime::{APP_WIT, DEFAULT_WORLD, WASI_WIT, WitSource, WorldIdentity};
use std::sync::Arc;
use wit_parser::{Resolve, WorldId, WorldItem};

/// One immutable resolved world shared by ABI lowering and provider validation.
pub struct ResolvedWorldContext {
    resolve: Arc<Resolve>,
    world: Option<WorldId>,
    imports: Vec<String>,
    exports: Vec<String>,
}

impl ResolvedWorldContext {
    /// Wraps an already-resolved `Resolve`/`WorldId` pair. Used by isolated
    /// target tests with custom WIT fixtures.
    pub fn from_resolve(resolve: Resolve, world: WorldId) -> Self {
        let (imports, exports) = collect_interfaces(&resolve, world);
        Self {
            resolve: Arc::new(resolve),
            world: Some(world),
            imports,
            exports,
        }
    }

    /// A context that permits every interface the resolve defines. Isolated ABI
    /// fixtures use it; it has no world and cannot be composed.
    pub fn permissive(resolve: Arc<Resolve>) -> Self {
        let mut imports = resolve
            .interfaces
            .iter()
            .filter_map(|(id, _)| resolve.id_of(id))
            .collect::<Vec<_>>();
        imports.sort();
        Self {
            resolve,
            world: None,
            imports,
            exports: Vec::new(),
        }
    }

    /// The resolved definitions. Backend canonical flattening reads through
    /// this borrow; it never loads a second copy.
    pub fn resolve(&self) -> &Resolve {
        &self.resolve
    }

    /// A shared handle to the same resolved definitions. The backend's ABI
    /// registry keeps this handle instead of cloning the `Resolve`.
    pub fn shared_resolve(&self) -> Arc<Resolve> {
        Arc::clone(&self.resolve)
    }

    pub fn world(&self) -> WorldId {
        self.world.expect("a permissive context has no world")
    }
    pub(crate) fn composition_world(&self) -> Option<WorldId> {
        self.world
    }

    /// Canonical ids of interfaces the default world imports.
    pub fn world_imports(&self) -> &[String] {
        &self.imports
    }

    /// Canonical ids of interfaces the default world exports.
    pub fn world_exports(&self) -> &[String] {
        &self.exports
    }

    /// Whether the world imports the named canonical interface.
    pub fn imports_interface(&self, canonical: &str) -> bool {
        self.imports.iter().any(|id| id == canonical)
    }
}

/// Resolves the supplied WIT sources into one world context.
///
/// The application package must declare the requested world; a mismatch is a
/// hard error rather than a silent fallback.
pub fn resolve_definitions(
    wasi: &[WitSource],
    app: WitSource,
    world: WorldIdentity,
) -> Result<ResolvedWorldContext, LinkErrors> {
    let mut resolve = Resolve::default();
    for source in wasi {
        resolve
            .push_str(source.path, source.contents)
            .map_err(|error| {
                LinkErrors::one(
                    LinkStage::Definitions,
                    source.path,
                    format!("invalid WIT source: {error}"),
                )
            })?;
    }
    let package = resolve.push_str(app.path, app.contents).map_err(|error| {
        LinkErrors::one(
            LinkStage::Definitions,
            app.path,
            format!("invalid application WIT: {error}"),
        )
    })?;
    let declared = &resolve.packages[package].name;
    if declared.namespace != world.package_namespace || declared.name != world.package_name {
        return Err(LinkErrors::one(
            LinkStage::Definitions,
            app.path,
            format!(
                "application WIT declares `{}:{}`, not `{}:{}`",
                declared.namespace, declared.name, world.package_namespace, world.package_name
            ),
        ));
    }
    let world_id = resolve.packages[package]
        .worlds
        .get(world.world)
        .copied()
        .ok_or_else(|| {
            LinkErrors::one(
                LinkStage::Definitions,
                format!("{}:{}", world.package_namespace, world.package_name),
                format!("application WIT is missing the `{}` world", world.world),
            )
        })?;
    let (imports, exports) = collect_interfaces(&resolve, world_id);
    Ok(ResolvedWorldContext {
        resolve: Arc::new(resolve),
        world: Some(world_id),
        imports,
        exports,
    })
}

/// Resolves the runtime's pinned default world.
pub fn resolve_default_definitions() -> Result<ResolvedWorldContext, LinkErrors> {
    resolve_definitions(WASI_WIT, APP_WIT, DEFAULT_WORLD)
}

fn collect_interfaces(resolve: &Resolve, world: WorldId) -> (Vec<String>, Vec<String>) {
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    for item in resolve.worlds[world].imports.values() {
        if let WorldItem::Interface { id, .. } = item
            && let Some(id) = resolve.id_of(*id)
        {
            imports.push(id);
        }
    }
    for item in resolve.worlds[world].exports.values() {
        if let WorldItem::Interface { id, .. } = item
            && let Some(id) = resolve.id_of(*id)
        {
            exports.push(id);
        }
    }
    imports.sort();
    exports.sort();
    (imports, exports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_world_resolves_and_exports_the_command() {
        let context = resolve_default_definitions().expect("the pinned WIT should resolve");
        assert!(
            context
                .world_imports()
                .iter()
                .any(|id| id == "wasi:cli/stdout@0.2.12")
        );
        assert!(
            context
                .world_exports()
                .iter()
                .any(|id| id == "wasi:cli/run@0.2.12")
        );
        assert!(context.imports_interface("wasi:io/streams@0.2.12"));
        assert!(!context.imports_interface("wasi:http/outgoing-handler@0.2.12"));
    }

    #[test]
    fn a_world_identity_mismatch_is_rejected() {
        let bogus = WorldIdentity {
            package_namespace: "other",
            package_name: "app",
            world: "command",
        };
        assert!(resolve_definitions(WASI_WIT, APP_WIT, bogus).is_err());
    }
}
