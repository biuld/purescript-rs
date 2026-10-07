//! Resolves WASI import signatures from the vendored WIT. Imports are named by
//! their WIT interface and function, so the set of WASI functions is not
//! hard-coded in the compiler. See
//! `docs/decision/DEC-06-runtime-interface-via-wit.md`.

use crate::TargetCapabilities;
use crate::types::ValueType;
use psrs_hir::{ModuleId, SymbolId};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use wit_parser::Resolve;
use wit_parser::abi::AbiVariant;

pub(crate) mod canonical;
mod definitions;
mod handles;
pub(crate) mod layout;
pub(crate) mod link;
#[cfg(test)]
mod tests;
mod validation;

use canonical::{
    CanonicalType, FnAbi, Ownership, function_abi, function_abi_from_types,
    resolve as resolve_canonical,
};
pub(crate) use definitions::load_wit;
use definitions::supported_interfaces;
pub use handles::{HandleMode, HandleResource};
#[cfg(test)]
pub(crate) use link::intern_source_type;
use validation::unsupported_shape;
pub(crate) use validation::wasi_interface_enabled;

/// Maps a resolved WIT core value type to the backend's value type.
fn value_type(ty: wit_parser::abi::WasmType) -> Result<ValueType, String> {
    Ok(match ty {
        wit_parser::abi::WasmType::I32
        | wit_parser::abi::WasmType::Pointer
        | wit_parser::abi::WasmType::Length => ValueType::I32,
        wit_parser::abi::WasmType::I64 | wit_parser::abi::WasmType::PointerOrI64 => ValueType::I64,
        wit_parser::abi::WasmType::F32 => ValueType::F32,
        wit_parser::abi::WasmType::F64 => ValueType::F64,
    })
}

/// The core export name `wit-component` expects for the exported interface
/// function `wasi:cli/run.run` under its legacy mangling.
pub const RUN_CORE_EXPORT: &str = "wasi:cli/run@0.2.12#run";

/// Scratch linear-memory address passed as the return pointer to WASI calls
/// whose result does not fit in a single canonical result.
pub const PRINT_SCRATCH: i32 = 0;

/// The size of the reserved scratch region at the start of linear memory. It
/// must hold the return pointer area of every canonical ABI call the backend
/// emits. String data begins after it, so data segments never overwrite it.
pub const SCRATCH_SIZE: u32 = 16;

/// The first linear-memory offset after the scratch region.
pub const SCRATCH_END: u32 = PRINT_SCRATCH as u32 + SCRATCH_SIZE;

/// The start of the allocator's heap-state segment: two pointer-width words
/// holding the free-list head and the bump break. It follows the scratch region.
pub const HEAP_STATE: u32 = SCRATCH_END;

/// The byte size of the heap-state segment: a free-list head word and a bump
/// break word, each one wasm32 pointer word.
pub const HEAP_STATE_SIZE: u32 = 2 * WORD_SIZE;

/// The first address the canonical allocator may hand out. It is aligned to the
/// block granularity so every block header stays aligned.
pub const HEAP_START: u32 = (HEAP_STATE + HEAP_STATE_SIZE).next_multiple_of(MIN_BLOCK);

/// A wasm32 address word.
pub const WORD_SIZE: u32 = 4;

/// The per-block metadata header: a block-size word and a length/next word.
pub const HEADER_SIZE: u32 = 8;

/// The block granularity. It matches the maximum canonical ABI field alignment
/// (`i64`/`f64`), so block headers and payloads stay aligned.
pub const MIN_BLOCK: u32 = 8;

/// Reserved MIR symbol for the allocator synthesized after ABI memory layout
/// is known. Calls to this symbol become calls to the local `cabi_realloc`
/// function during Wasm lowering; it is never emitted as a core import.
pub(crate) const REALLOC_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 1);

/// Reserved symbols for the string boundary helpers at the canonical ABI.
/// P10 synthesizes them as ordinary local Wasm functions; like
/// `REALLOC_SYMBOL` they are never emitted as core imports.
pub(crate) const STRING_TO_BYTES_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 2);
pub(crate) const BYTES_TO_STRING_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 3);

/// Reserved MIR symbol for the synthesized `validate_step` helper. Like the
/// other boundary symbols it is never a core import.
pub(crate) const VALIDATE_STEP_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 4);

/// Intrinsic symbols the MIR lowering reserves for the canonical ABI and the
/// string boundary. Any other intrinsic-symbol allocator (for example the
/// aggregate conversion helpers, which allocate downward from `u32::MAX`) must
/// skip these.
pub(crate) const NUMBER_TO_STRING_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 5);

pub(crate) const NUMBER_FROM_DECIMAL_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 6);

pub(crate) const NUMBER_ACOS_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 7);

pub(crate) const NUMBER_ASIN_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 8);

pub(crate) const NUMBER_ATAN_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 9);

pub(crate) const RESERVED_ABI_SYMBOLS: [SymbolId; 9] = [
    REALLOC_SYMBOL,
    STRING_TO_BYTES_SYMBOL,
    BYTES_TO_STRING_SYMBOL,
    VALIDATE_STEP_SYMBOL,
    NUMBER_TO_STRING_SYMBOL,
    NUMBER_FROM_DECIMAL_SYMBOL,
    NUMBER_ACOS_SYMBOL,
    NUMBER_ASIN_SYMBOL,
    NUMBER_ATAN_SYMBOL,
];

/// WASI interfaces and functions the backend itself references. The standard
/// library names its own imports in source.
pub mod names {
    pub const STDOUT: &str = "wasi:cli/stdout";
    pub const STDERR: &str = "wasi:cli/stderr";
    pub const STREAMS: &str = "wasi:io/streams";
    pub const EXIT: &str = "wasi:cli/exit";
    pub const GET_STDOUT: &str = "get-stdout";
    pub const GET_STDERR: &str = "get-stderr";
    pub const WRITE_STDOUT: &str = "[method]output-stream.blocking-write-and-flush";
    pub const EXIT_WITH_CODE: &str = "exit-with-code";
}

/// A resolved WASI import: a core Wasm import with its canonical ABI signature
/// and the symbol the low-level IRs use to reference it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WasiImport {
    pub symbol: SymbolId,
    /// The core import module: the interface's canonical id, for example
    /// `wasi:cli/stdout@0.2.12`.
    pub module: String,
    /// The core import field, for example `get-stdout`.
    pub name: String,
    /// The core wasm signature parameters, including a return pointer when the
    /// result is passed indirectly.
    pub parameters: Vec<ValueType>,
    /// The core wasm result, if the signature has one.
    pub result: Option<ValueType>,
    /// The resolved WIT parameters, in declaration order (including a method's
    /// receiver). Empty when the shape has no source ABI mapping.
    pub(crate) params: Vec<CanonicalType>,
    /// The resolved WIT result, if any.
    pub(crate) canonical_result: Option<CanonicalType>,
    /// The canonical ABI decisions for this function: flattened parameters and
    /// results, indirect-parameter and return-pointer flags, and the return
    /// area of an indirect result.
    pub(crate) abi: FnAbi,
    /// A diagnostic explaining why this import is outside the currently
    /// supported canonical-ABI subset, if any. Keeping this on the resolved
    /// descriptor lets MIR reject it before emitting a semantically lossy
    /// call.
    pub unsupported: Option<String>,
}

impl WasiImport {
    /// The canonical handle flattened into `flat_index`, when that slot is one.
    pub(crate) fn handle_at_flat_index(&self, flat_index: usize) -> Option<&CanonicalType> {
        handles::handle_at_flat_index(self, flat_index)
    }

    pub(crate) fn has_indirect_parameters(&self) -> bool {
        self.abi.indirect_params
    }
}

/// Resolves WASI imports against the vendored WIT, interning each distinct
/// `(module, name)` to a stable [`SymbolId`].
pub struct WasiRegistry {
    resolve: Arc<Resolve>,
    imports: Vec<WasiImport>,
    keys: HashMap<(String, String), usize>,
    target: TargetCapabilities,
    /// Canonical ids of the interfaces the default world permits.
    supported: HashSet<String>,
}

impl WasiRegistry {
    /// Symbol indices for WASI imports live above the intrinsic and runtime
    /// ranges in the reserved intrinsic module.
    const SYMBOL_BASE: u32 = 1 << 20;

    pub(crate) fn from_resolve(resolve: Resolve, target: TargetCapabilities) -> Self {
        Self::from_shared_resolve(Arc::new(resolve), target)
    }

    /// Builds a registry over the linker's resolved-world context. The same
    /// parsed definitions are shared, so interface identities are not
    /// regenerated.
    pub(crate) fn from_shared_resolve(resolve: Arc<Resolve>, target: TargetCapabilities) -> Self {
        let supported = supported_interfaces(&resolve);
        Self {
            resolve,
            imports: Vec::new(),
            keys: HashMap::new(),
            target,
            supported,
        }
    }

    pub fn load() -> Result<Self, String> {
        Self::load_with_capabilities(TargetCapabilities::default())
    }

    /// Loads the pinned WIT from the runtime catalog with the service families
    /// enabled by `target`.
    pub fn load_with_capabilities(target: TargetCapabilities) -> Result<Self, String> {
        let mut resolve = Resolve::default();
        load_wit(&mut resolve)?;
        Ok(Self::from_resolve(resolve, target))
    }

    /// Resolves `interface`/`function` (for example `wasi:cli/stdout`,
    /// `get-stdout`) and returns its import, interning it on first use.
    pub fn import(&mut self, interface: &str, function: &str) -> Result<WasiImport, String> {
        let (package, interface_name) = interface
            .split_once('/')
            .ok_or_else(|| format!("`{interface}` is not `package/interface`"))?;
        let package_id = self
            .resolve
            .packages
            .iter()
            .find_map(|(id, candidate)| {
                (format!("{}:{}", candidate.name.namespace, candidate.name.name) == package)
                    .then_some(id)
            })
            .ok_or_else(|| format!("WASI package `{package}` is not vendored"))?;
        let interface_id = self.resolve.packages[package_id]
            .interfaces
            .get(interface_name)
            .copied()
            .ok_or_else(|| format!("`{interface}` is not vendored"))?;
        let module = self
            .resolve
            .id_of(interface_id)
            .ok_or_else(|| format!("`{interface}` has no canonical id"))?;
        let key = (module.clone(), function.to_string());
        if let Some(index) = self.keys.get(&key) {
            return Ok(self.imports[*index].clone());
        }
        // A source-declared `[resource-drop]<resource>` names the canonical drop
        // import rather than a WIT function (DEC-14).
        if let Some(resource) = function.strip_prefix("[resource-drop]") {
            if resource.is_empty() {
                return Err(format!("`{interface}.{function}` is not a resource drop"));
            }
            let index = self.intern_resource_drop_index(&module, resource);
            return Ok(self.imports[index].clone());
        }
        let wit_function = self.resolve.interfaces[interface_id]
            .functions
            .get(function)
            .ok_or_else(|| format!("`{interface}.{function}` is not vendored"))?
            .clone();
        let signature = self
            .resolve
            .wasm_signature(AbiVariant::GuestImport, &wit_function);
        let resolved: Vec<Option<CanonicalType>> = wit_function
            .params
            .iter()
            .map(|param| resolve_canonical(&self.resolve, &param.ty))
            .collect();
        let mut canonical_result = wit_function
            .result
            .as_ref()
            .and_then(|ty| resolve_canonical(&self.resolve, ty));
        let result_shape: Result<Option<CanonicalType>, ()> = match &wit_function.result {
            None => Ok(None),
            Some(_) => canonical_result.clone().map(Some).ok_or(()),
        };
        // A parameter or result with no canonical form leaves the import
        // constructible but unrepresentable; the lowering rejects it on the
        // `unsupported` diagnostic before reading `params`/`canonical_result`.
        let mut params: Vec<CanonicalType> = resolved.iter().flatten().cloned().collect();
        if params.len() != resolved.len() {
            params.clear();
        }
        // Every parameter has a direct source lowering, so the declared
        // signature can be matched against the canonical flattening. A shape
        // that only the primitive-FFI path can represent is deferred.
        let surface = params.len() == resolved.len()
            && params.iter().all(canonical::parameter_has_source_abi);
        let unsupported = unsupported_shape(&resolved, &result_shape, &canonical_result)
            .or_else(|| {
                matches!(
                    &canonical_result,
                    Some(CanonicalType::Handle {
                        ownership: Ownership::Borrow,
                        ..
                    })
                )
                .then(|| {
                    "a borrow<T> result cannot outlive the call that produced it; return own<T> instead"
                        .into()
                })
            })
            .or_else(|| {
                (!self.supported.contains(&module)).then(|| {
                    format!(
                        "WASI interface `{module}` is not in the current component capability profile"
                    )
                })
            })
            .or_else(|| {
                // An `option`, tuple, or payload-bearing variant is one WIT
                // parameter and several flat slots. Signature validation
                // compares the declared primitives with the canonical flat
                // leaves instead.
                if !surface {
                    return None;
                }
                let flattened = params.iter().map(|ty| canonical::flatten(ty).len()).sum::<usize>();
                let matches = if signature.indirect_params {
                    signature.params.len() == 1 + usize::from(signature.retptr)
                        && signature.params.first()
                            == Some(&wit_parser::abi::WasmType::Pointer)
                        && (!signature.retptr
                            || signature.params.last()
                                == Some(&wit_parser::abi::WasmType::Pointer))
                } else {
                    flattened + usize::from(signature.retptr) == signature.params.len()
                };
                (!matches).then(|| {
                    "WIT parameter shapes do not match the canonical ABI signature".into()
                })
            })
            .or_else(|| {
                (!wasi_interface_enabled(self.target, &module)).then(|| {
                    format!(
                        "WASI interface `{module}` is disabled by the selected target capability profile"
                    )
                })
            });
        self.bind_handle_drops(&mut params, &mut canonical_result);
        let abi = function_abi(&self.resolve, &wit_function)
            .unwrap_or_else(|| function_abi_from_types(&params, canonical_result.as_ref()));
        let parameters = signature
            .params
            .iter()
            .copied()
            .map(value_type)
            .collect::<Result<Vec<_>, _>>()?;
        let result = match signature.results.as_slice() {
            [] => None,
            [single] => Some(value_type(*single)?),
            _ => {
                return Err(format!(
                    "`{interface}.{function}` has multiple canonical results"
                ));
            }
        };
        let symbol = SymbolId::new(
            ModuleId::INTRINSICS,
            Self::SYMBOL_BASE + self.imports.len() as u32,
        );
        self.imports.push(WasiImport {
            symbol,
            module,
            name: function.to_string(),
            parameters,
            result,
            params,
            canonical_result,
            abi,
            unsupported,
        });
        self.keys.insert(key, self.imports.len() - 1);
        Ok(self.imports.last().expect("just pushed").clone())
    }

    pub fn imports(&self) -> &[WasiImport] {
        &self.imports
    }

    /// A shared handle to the parsed definitions this registry resolves
    /// against. Isolated lowering fixtures wrap it in a permissive context.
    pub(crate) fn shared_resolve(&self) -> Arc<Resolve> {
        Arc::clone(&self.resolve)
    }

    /// The core import module and field for an interned import symbol. This is
    /// the only place the WIT interface and function names are resolved.
    pub fn symbol_name(&self, symbol: SymbolId) -> Option<(&str, &str)> {
        self.imports
            .iter()
            .find(|import| import.symbol == symbol)
            .map(|import| (import.module.as_str(), import.name.as_str()))
    }

    /// Whether an interned import returns a `list`/`string`, which needs the
    /// module to export `cabi_realloc`.
    pub fn has_list_result(&self, symbol: SymbolId) -> bool {
        self.imports.iter().any(|import| {
            import.symbol == symbol
                && import.canonical_result.as_ref().is_some_and(|ty| {
                    matches!(ty, CanonicalType::String | CanonicalType::List(_))
                        || ty.is_byte_list()
                })
        })
    }
}

pub(crate) fn source_field_name(wit_name: &str) -> String {
    let mut source_name = String::with_capacity(wit_name.len());
    let mut uppercase_next = false;
    for character in wit_name.chars() {
        if character == '-' {
            uppercase_next = true;
        } else if uppercase_next {
            source_name.extend(character.to_uppercase());
            uppercase_next = false;
        } else {
            source_name.push(character);
        }
    }
    source_name
}

/// Test-only builders for a canonical [`WasiImport`].
#[cfg(test)]
pub(crate) mod test_support {
    use super::canonical::{CanonicalType, CoreVal};
    use super::*;

    fn value_type(value: CoreVal) -> ValueType {
        match value {
            CoreVal::I32 => ValueType::I32,
            CoreVal::I64 => ValueType::I64,
            CoreVal::F32 => ValueType::F32,
            CoreVal::F64 => ValueType::F64,
        }
    }

    /// Builds a supported import from canonical parameters and result, deriving
    /// the core wasm signature and flattening.
    pub(crate) fn import(
        symbol: SymbolId,
        module: &str,
        name: &str,
        params: Vec<CanonicalType>,
        result: Option<CanonicalType>,
    ) -> WasiImport {
        let abi = canonical::function_abi_from_types(&params, result.as_ref());
        let mut parameters = Vec::new();
        if abi.indirect_params {
            parameters.push(ValueType::I32);
        } else {
            parameters.extend(abi.flat_params.iter().copied().map(value_type));
        }
        if abi.retptr {
            parameters.push(ValueType::I32);
        }
        let core_result = if abi.retptr {
            None
        } else {
            abi.flat_results.first().copied().map(value_type)
        };
        WasiImport {
            symbol,
            module: module.into(),
            name: name.into(),
            parameters,
            result: core_result,
            params,
            canonical_result: result,
            abi,
            unsupported: None,
        }
    }
}
