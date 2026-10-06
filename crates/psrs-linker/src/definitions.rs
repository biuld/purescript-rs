//! WIT definition loading and the immutable resolved-world context.
//!
//! One resolved context is shared by backend ABI lowering and linker provider
//! validation. It is held behind an `Arc` so the backend can borrow it without
//! parsing a second copy or regenerating interface identities.

use crate::error::{LinkErrors, LinkStage};
use psrs_runtime::{APP_WIT, DEFAULT_WORLD, WASI_WIT, WitSource, WorldIdentity};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use wit_parser::{
    Handle, InterfaceId, Resolve, Type, TypeDefKind, TypeId, TypeOwner, WorldId, WorldItem,
};

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

    /// The allowed residual host capabilities: the transitive WIT dependency
    /// closure of `seeds` within the resolved world.
    pub fn host_closure(&self, seeds: &BTreeSet<String>) -> Vec<String> {
        let resolve = &self.resolve;
        let mut owner = HashMap::<TypeId, InterfaceId>::new();
        for (id, ty) in resolve.types.iter() {
            if let TypeOwner::Interface(interface) = ty.owner {
                owner.insert(id, interface);
            }
        }
        let mut pending = Vec::new();
        let mut seen = HashSet::<InterfaceId>::new();
        for (id, _) in resolve.interfaces.iter() {
            if let Some(canonical) = resolve.id_of(id)
                && seeds.contains(&canonical)
                && seen.insert(id)
            {
                pending.push(id);
            }
        }
        while let Some(interface) = pending.pop() {
            let mut referenced = Vec::new();
            for ty in resolve.interfaces[interface].types.values() {
                walk_type(resolve, &Type::Id(*ty), &mut referenced);
            }
            for function in resolve.interfaces[interface].functions.values() {
                for param in &function.params {
                    walk_type(resolve, &param.ty, &mut referenced);
                }
                if let Some(ty) = &function.result {
                    walk_type(resolve, ty, &mut referenced);
                }
            }
            for ty in referenced {
                if let Some(dependency) = owner.get(&ty)
                    && seen.insert(*dependency)
                {
                    pending.push(*dependency);
                }
            }
        }
        let mut ids = seen
            .into_iter()
            .filter_map(|id| resolve.id_of(id))
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }
}

fn walk_type(resolve: &Resolve, ty: &Type, out: &mut Vec<TypeId>) {
    if let Type::Id(id) = ty {
        out.push(*id);
        walk_kind(resolve, &resolve.types[*id].kind, out);
    }
}

fn walk_kind(resolve: &Resolve, kind: &TypeDefKind, out: &mut Vec<TypeId>) {
    match kind {
        TypeDefKind::Record(record) => {
            for field in &record.fields {
                walk_type(resolve, &field.ty, out);
            }
        }
        TypeDefKind::Tuple(tuple) => {
            for ty in &tuple.types {
                walk_type(resolve, ty, out);
            }
        }
        TypeDefKind::Variant(variant) => {
            for case in &variant.cases {
                if let Some(ty) = &case.ty {
                    walk_type(resolve, ty, out);
                }
            }
        }
        TypeDefKind::Option(ty) | TypeDefKind::List(ty) => walk_type(resolve, ty, out),
        TypeDefKind::Result(result) => {
            if let Some(ty) = &result.ok {
                walk_type(resolve, ty, out);
            }
            if let Some(ty) = &result.err {
                walk_type(resolve, ty, out);
            }
        }
        TypeDefKind::Map(key, value) => {
            walk_type(resolve, key, out);
            walk_type(resolve, value, out);
        }
        TypeDefKind::FixedLengthList(ty, _) => walk_type(resolve, ty, out),
        TypeDefKind::Future(Some(ty)) | TypeDefKind::Stream(Some(ty)) => {
            walk_type(resolve, ty, out)
        }
        TypeDefKind::Type(ty) => walk_type(resolve, ty, out),
        TypeDefKind::Handle(Handle::Own(id) | Handle::Borrow(id)) => {
            walk_type(resolve, &Type::Id(*id), out);
        }
        _ => {}
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
