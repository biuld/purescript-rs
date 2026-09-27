//! Own and borrow handles. A handle is one canonical `i32` table index.
//! `own<T>` carries a drop obligation discharged by `[resource-drop]<T>`;
//! `borrow<T>` is a call-scoped index released by the same intrinsic when the
//! call that created it returns. See
//! `docs/design/backend/wasm/canonical-abi-and-wit.md`.

use super::WasiImport;
use super::canonical::{CanonicalType, Ownership, ResourceId};
use psrs_hir::{ModuleId, SymbolId};
use wit_parser::{Handle, Resolve, Type, TypeDefKind, TypeId, TypeOwner};

/// Sentinel stored until the registry interns the drop import.
pub(crate) const UNBOUND_DROP: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 5);

/// Whether a handle index owns the resource or only borrows it for one call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleMode {
    Own,
    Borrow,
}

/// The WIT resource a handle index refers to, and the `resource.drop` import
/// that releases it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandleResource {
    pub mode: HandleMode,
    /// Canonical interface id that defines the resource, for example
    /// `wasi:io/streams@0.2.12`. The drop import's module is this id.
    pub interface: String,
    /// WIT resource name, for example `output-stream`.
    pub name: String,
    /// Interned `[resource-drop]<name>` symbol. [`UNBOUND_DROP`] until the
    /// registry binds it, and for a borrow.
    pub drop_symbol: SymbolId,
}

impl CanonicalType {
    /// The resource metadata of a canonical handle, if this is one.
    pub(crate) fn handle_resource(&self) -> Option<HandleResource> {
        let CanonicalType::Handle {
            resource,
            ownership,
        } = self
        else {
            return None;
        };
        Some(HandleResource {
            mode: match ownership {
                Ownership::Own { .. } => HandleMode::Own,
                Ownership::Borrow => HandleMode::Borrow,
            },
            interface: resource.interface.clone(),
            name: resource.name.clone(),
            drop_symbol: ownership.drop_symbol().unwrap_or(UNBOUND_DROP),
        })
    }
}

/// Core import field wit-component maps to `canon resource.drop`.
pub(crate) fn drop_import_field(resource: &str) -> String {
    format!("[resource-drop]{resource}")
}

/// Classifies a resolved `own` or `borrow` handle. The drop symbol is unbound
/// until [`super::WasiRegistry`] interns the intrinsic.
pub(super) fn classify(resolve: &Resolve, handle: &Handle) -> Option<HandleResource> {
    let (mode, root) = match *handle {
        Handle::Own(id) => (HandleMode::Own, id),
        Handle::Borrow(id) => (HandleMode::Borrow, id),
    };
    // `own<T>` is sometimes an alias of the resource, or a handle typedef whose
    // inner id is that alias. The drop import is named from the resource itself.
    let resource = resource_definition(resolve, root)?;
    let definition = &resolve.types[resource];
    let name = definition.name.clone()?;
    let TypeOwner::Interface(interface) = definition.owner else {
        return None;
    };
    let interface = resolve.id_of(interface)?;
    Some(HandleResource {
        mode,
        interface,
        name,
        drop_symbol: UNBOUND_DROP,
    })
}

fn resource_definition(resolve: &Resolve, mut id: TypeId) -> Option<TypeId> {
    for _ in 0..8 {
        match resolve.types[id].kind {
            TypeDefKind::Resource => return Some(id),
            TypeDefKind::Type(Type::Id(next)) => id = next,
            TypeDefKind::Handle(Handle::Own(next) | Handle::Borrow(next)) => id = next,
            _ => return None,
        }
    }
    None
}

/// The canonical handle whose single flat slot is `flat_index`, if one covers
/// that slot.
pub(crate) fn handle_at_flat_index(
    import: &WasiImport,
    flat_index: usize,
) -> Option<&CanonicalType> {
    super::canonical::handle_at_flat_index(&import.params, flat_index)
}

/// Sets each owned handle's drop to `bind(resource)`.
fn bind_canonical<'a>(
    ty: &'a mut CanonicalType,
    bind: &mut impl FnMut(&'a ResourceId) -> SymbolId,
) {
    match ty {
        CanonicalType::Handle {
            resource,
            ownership: Ownership::Own { drop },
        } => *drop = bind(resource),
        CanonicalType::Record(fields) => {
            for field in fields {
                bind_canonical(&mut field.ty, bind);
            }
        }
        CanonicalType::Option(payload) => bind_canonical(payload, bind),
        CanonicalType::Result { ok, err } => {
            if let Some(ok) = ok {
                bind_canonical(ok, bind);
            }
            if let Some(err) = err {
                bind_canonical(err, bind);
            }
        }
        CanonicalType::Variant(cases) => {
            for case in cases {
                if let Some(payload) = &mut case.payload {
                    bind_canonical(payload, bind);
                }
            }
        }
        _ => {}
    }
}

impl super::WasiRegistry {
    /// Interns `[resource-drop]<T>` for every owned handle this signature
    /// mentions.
    pub(super) fn bind_handle_drops(
        &mut self,
        params: &mut [CanonicalType],
        result: &mut Option<CanonicalType>,
    ) {
        let mut bind =
            |resource: &ResourceId| self.intern_resource_drop(&resource.interface, &resource.name);
        for ty in params.iter_mut() {
            bind_canonical(ty, &mut bind);
        }
        if let Some(ty) = result {
            bind_canonical(ty, &mut bind);
        }
    }

    /// The guest calls this intrinsic to remove a handle from its table.
    /// wit-component lowers the import to `canon resource.drop`.
    fn intern_resource_drop(&mut self, interface: &str, resource: &str) -> SymbolId {
        let index = self.intern_resource_drop_index(interface, resource);
        self.imports[index].symbol
    }

    /// Interns `[resource-drop]<resource>` and returns its index. Shared by the
    /// signature binding and a source-declared drop.
    pub(super) fn intern_resource_drop_index(&mut self, interface: &str, resource: &str) -> usize {
        let field = drop_import_field(resource);
        let key = (interface.to_string(), field.clone());
        if let Some(index) = self.keys.get(&key) {
            return *index;
        }
        let symbol = SymbolId::new(
            ModuleId::INTRINSICS,
            Self::SYMBOL_BASE + self.imports.len() as u32,
        );
        self.imports.push(super::WasiImport {
            symbol,
            module: interface.to_string(),
            name: field,
            parameters: vec![crate::types::ValueType::I32],
            result: None,
            params: vec![CanonicalType::Int {
                width: 32,
                signed: true,
            }],
            canonical_result: None,
            abi: super::canonical::function_abi_from_types(
                &[CanonicalType::Int {
                    width: 32,
                    signed: true,
                }],
                None,
            ),
            unsupported: None,
        });
        let index = self.imports.len() - 1;
        self.keys.insert(key, index);
        index
    }
}
