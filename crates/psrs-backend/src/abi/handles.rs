//! Own and borrow handles. A handle is one canonical `i32` table index.
//! `own<T>` carries a drop obligation discharged by `[resource-drop]<T>`;
//! `borrow<T>` is a call-scoped index released by the same intrinsic when the
//! call that created it returns. See
//! `docs/design/backend/wasm/canonical-abi-and-wit.md`.

use super::validation::flattened_parameter_count;
use super::{WasiImport, WasiParamKind, WasiResultKind};
use psrs_hir::{ModuleId, SymbolId};
use wit_parser::{Handle, Resolve, Type, TypeDefKind, TypeId, TypeOwner};

/// Sentinel stored by classification until the registry interns the drop import.
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
    /// registry binds it.
    pub drop_symbol: SymbolId,
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

/// The handle whose single flat slot is `flat_index`, if one covers that slot.
pub(crate) fn handle_at_flat_index(
    import: &WasiImport,
    flat_index: usize,
) -> Option<&HandleResource> {
    let mut cursor = 0;
    for kind in &import.param_kinds {
        if let Some(found) = cover(kind, flat_index, &mut cursor) {
            return Some(found);
        }
    }
    None
}

fn cover<'a>(
    kind: &'a WasiParamKind,
    target: usize,
    cursor: &mut usize,
) -> Option<&'a HandleResource> {
    match kind {
        WasiParamKind::Handle(handle) => {
            let here = *cursor;
            *cursor += 1;
            (here == target).then_some(handle)
        }
        WasiParamKind::Record { fields } => {
            for field in fields {
                if let Some(found) = cover(&field.kind, target, cursor) {
                    return Some(found);
                }
            }
            None
        }
        other => {
            *cursor += flattened_parameter_count(other);
            None
        }
    }
}

pub(super) fn bind_param(kind: &mut WasiParamKind, bind: &mut impl FnMut(&mut HandleResource)) {
    match kind {
        WasiParamKind::Handle(handle) => bind(handle),
        WasiParamKind::Record { fields } => {
            for field in fields {
                bind_param(&mut field.kind, bind);
            }
        }
        _ => {}
    }
}

pub(super) fn bind_result(kind: &mut WasiResultKind, bind: &mut impl FnMut(&mut HandleResource)) {
    if let WasiResultKind::Handle(handle) = kind {
        bind(handle);
    }
}

impl super::WasiRegistry {
    /// Interns `[resource-drop]<T>` for every handle this signature mentions.
    pub(super) fn bind_handle_drops(
        &mut self,
        params: &mut [WasiParamKind],
        result: &mut WasiResultKind,
    ) {
        let mut bind = |handle: &mut HandleResource| {
            handle.drop_symbol = self.intern_resource_drop(&handle.interface, &handle.name);
        };
        for kind in params.iter_mut() {
            bind_param(kind, &mut bind);
        }
        bind_result(result, &mut bind);
    }

    /// The guest calls this intrinsic to remove a handle from its table.
    /// wit-component lowers the import to `canon resource.drop`.
    fn intern_resource_drop(&mut self, interface: &str, resource: &str) -> SymbolId {
        let field = drop_import_field(resource);
        let key = (interface.to_string(), field.clone());
        if let Some(index) = self.keys.get(&key) {
            return self.imports[*index].symbol;
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
            param_kinds: vec![WasiParamKind::Integer32],
            result: None,
            result_kind: WasiResultKind::None,
            unsupported: None,
            retptr: false,
            flat_slots: vec![super::FlatSlot::Int32],
        });
        self.keys.insert(key, self.imports.len() - 1);
        symbol
    }
}
