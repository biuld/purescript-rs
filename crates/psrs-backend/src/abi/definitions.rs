//! Pinned WIT loading and permitted-interface derivation.

use std::collections::HashSet;
use wit_parser::Resolve;

/// Pushes the runtime catalog's pinned WIT into `resolve` in dependency order.
pub(crate) fn load_wit(resolve: &mut Resolve) -> Result<(), String> {
    for source in psrs_runtime::WASI_WIT {
        resolve
            .push_str(source.path, source.contents)
            .map_err(|error| format!("invalid vendored WIT `{}`: {error}", source.path))?;
    }
    let app = psrs_runtime::APP_WIT;
    resolve
        .push_str(app.path, app.contents)
        .map_err(|error| format!("invalid application WIT: {error}"))?;
    Ok(())
}

/// The canonical ids the default world permits. A resolve that omits the
/// application world (an isolated ABI fixture) admits every interface it
/// defines; production resolves always include the world.
pub(super) fn supported_interfaces(resolve: &Resolve) -> HashSet<String> {
    let world = resolve
        .packages
        .iter()
        .find(|(_, package)| {
            package.name.namespace == psrs_runtime::DEFAULT_WORLD.package_namespace
                && package.name.name == psrs_runtime::DEFAULT_WORLD.package_name
        })
        .and_then(|(_, package)| {
            package
                .worlds
                .get(psrs_runtime::DEFAULT_WORLD.world)
                .copied()
        });
    let mut supported = HashSet::new();
    match world {
        Some(world) => {
            for item in resolve.worlds[world].imports.values() {
                if let wit_parser::WorldItem::Interface { id, .. } = item
                    && let Some(canonical) = resolve.id_of(*id)
                {
                    supported.insert(canonical);
                }
            }
        }
        None => {
            for (id, _) in resolve.interfaces.iter() {
                if let Some(canonical) = resolve.id_of(id) {
                    supported.insert(canonical);
                }
            }
        }
    }
    supported
}
