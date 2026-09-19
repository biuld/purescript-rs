//! Resolves WASI import signatures from the vendored WIT. Imports are named by
//! their WIT interface and function, so the set of WASI functions is not
//! hard-coded in the compiler. See
//! `docs/decision/DEC-06-runtime-interface-via-wit.md`.

use crate::types::ValueType;
use psrs_hir::{BuiltinType, ModuleId, SymbolId, Type as HirType, TypeKind as HirTypeKind};
use std::collections::HashMap;
use wit_parser::abi::{AbiVariant, WasmType};
use wit_parser::{Resolve, Type as WitType, TypeDefKind};

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

/// The shape of one WIT-level parameter, which decides how a declared argument
/// maps to canonical parameters. Several shapes flatten to the same canonical
/// types, so the shape is kept for the lowering to adapt arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasiParamKind {
    /// A scalar flattened to one canonical `i32` parameter.
    Scalar,
    /// A 64-bit scalar flattened to one canonical `i64`. `signed` selects
    /// sign- or zero-extension when an `Int` argument is widened to it.
    Scalar64 { signed: bool },
    /// A resource handle flattened to one canonical `i32` handle.
    Handle,
    /// A string or list flattened to a `(pointer, length)` pair.
    List,
}

/// How a WIT import's result is represented, which decides how the lowering
/// consumes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasiResultKind {
    /// No result.
    None,
    /// A scalar returned directly in a register.
    Scalar,
    /// A `list`/`string` returned indirectly through a return pointer as a
    /// `(pointer, length)` pair.
    List,
    /// A result returned indirectly but not modeled (for example a `result` or
    /// a record); the lowering rejects it. A WIT `result` with a source `Unit`
    /// declaration is represented separately because write-like operations
    /// intentionally discard their error value.
    Result,
    /// A record, tuple, or other aggregate result that cannot be discarded by
    /// the current source-level ABI.
    Discarded,
}

/// The small source-level type vocabulary understood by the current WIT ABI
/// adapter. It is produced while crossing the Core boundary so CC/MIR do not
/// retain HIR type nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceType {
    Int,
    Boolean,
    String,
    Unit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSignature {
    pub parameters: Vec<SourceType>,
    pub result: SourceType,
    pub span: psrs_span::TextRange,
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
    pub parameters: Vec<ValueType>,
    /// The WIT-level shape of each declared parameter, aligned with the
    /// interface's parameters (including a method's receiver).
    pub param_kinds: Vec<WasiParamKind>,
    pub result: Option<ValueType>,
    pub result_kind: WasiResultKind,
    /// A diagnostic explaining why this import is outside the currently
    /// supported canonical-ABI subset, if any. Keeping this on the resolved
    /// descriptor lets MIR reject it before emitting a semantically lossy
    /// call.
    pub unsupported: Option<String>,
    /// Whether the import takes a return pointer for a value that does not fit
    /// in a single canonical result.
    pub retptr: bool,
}

/// Resolves WASI imports against the vendored WIT, interning each distinct
/// `(module, name)` to a stable [`SymbolId`].
pub struct WasiRegistry {
    resolve: Resolve,
    imports: Vec<WasiImport>,
    keys: HashMap<(String, String), usize>,
}

impl WasiRegistry {
    /// Symbol indices for WASI imports live above the intrinsic and runtime
    /// ranges in the reserved intrinsic module.
    const SYMBOL_BASE: u32 = 1 << 20;

    pub fn load() -> Result<Self, String> {
        let mut resolve = Resolve::default();
        crate::component::load_vendored_wasi(&mut resolve)?;
        Ok(Self {
            resolve,
            imports: Vec::new(),
            keys: HashMap::new(),
        })
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
        let wit_function = self.resolve.interfaces[interface_id]
            .functions
            .get(function)
            .ok_or_else(|| format!("`{interface}.{function}` is not vendored"))?;
        let module = self
            .resolve
            .id_of(interface_id)
            .ok_or_else(|| format!("`{interface}` has no canonical id"))?;
        let key = (module.clone(), function.to_string());
        if let Some(index) = self.keys.get(&key) {
            return Ok(self.imports[*index].clone());
        }
        let signature = self
            .resolve
            .wasm_signature(AbiVariant::GuestImport, wit_function);
        let param_kinds = wit_function
            .params
            .iter()
            .map(|param| param_kind(&self.resolve, &param.ty))
            .collect();
        let result_kind = match &wit_function.result {
            None => WasiResultKind::None,
            Some(ty) => result_kind(&self.resolve, ty),
        };
        let unsupported =
            unsupported_shape(&self.resolve, wit_function, &result_kind).or_else(|| {
                (!crate::component::component_interface_supported(&module)).then(|| {
                format!(
                    "WASI interface `{module}` is not in the current component capability profile"
                )
            })
            });
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
            param_kinds,
            result,
            result_kind,
            unsupported,
            retptr: signature.retptr,
        });
        self.keys.insert(key, self.imports.len() - 1);
        Ok(self.imports.last().expect("just pushed").clone())
    }

    pub fn imports(&self) -> &[WasiImport] {
        &self.imports
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
        self.imports
            .iter()
            .any(|import| import.symbol == symbol && import.result_kind == WasiResultKind::List)
    }

    /// Checks that a source-declared foreign import has a type that can be
    /// represented by the canonical ABI adapter. This is deliberately done
    /// before CC/MIR lowering: matching only arity would let an `Int` be used
    /// for a resource or a non-byte list be treated as a `String`.
    pub fn validate_signature(
        &self,
        import: &WasiImport,
        signature: &SourceSignature,
    ) -> Result<(), String> {
        if signature.parameters.len() != import.param_kinds.len() {
            return Err(format!(
                "WIT import `{}` expects {} source arguments, but its declaration has {}",
                import.name,
                import.param_kinds.len(),
                signature.parameters.len()
            ));
        }
        for (parameter, kind) in signature.parameters.iter().zip(&import.param_kinds) {
            let valid = match kind {
                WasiParamKind::Scalar => {
                    matches!(parameter, SourceType::Int | SourceType::Boolean)
                }
                WasiParamKind::Scalar64 { .. } | WasiParamKind::Handle => {
                    matches!(parameter, SourceType::Int)
                }
                WasiParamKind::List => matches!(parameter, SourceType::String),
            };
            if !valid {
                return Err(format!(
                    "WIT import `{}` has a source parameter with an incompatible type",
                    import.name
                ));
            }
        }
        let valid_result = match import.result_kind {
            WasiResultKind::None => matches!(signature.result, SourceType::Unit),
            WasiResultKind::Scalar => match import.result {
                Some(ValueType::I64) => {
                    matches!(signature.result, SourceType::Int)
                }
                Some(ValueType::I32) => {
                    matches!(signature.result, SourceType::Int)
                }
                _ => false,
            },
            WasiResultKind::List => matches!(signature.result, SourceType::String),
            WasiResultKind::Result => matches!(signature.result, SourceType::Unit),
            WasiResultKind::Discarded => false,
        };
        if !valid_result {
            return Err(format!(
                "WIT import `{}` has a source result type incompatible with its canonical result",
                import.name
            ));
        }
        Ok(())
    }
}

/// Converts a resolved HIR foreign-import type into the source-level subset
/// that may cross into CC. Unsupported polymorphic, aggregate, or higher-kinded
/// declarations remain `None` and are rejected by the ABI validation pass.
pub(crate) fn source_signature(signature: &HirType) -> Option<SourceSignature> {
    let mut parameters = Vec::new();
    let mut result = signature;
    while let HirTypeKind::Function {
        parameter,
        result: next,
    } = &result.kind
    {
        parameters.push(source_type(parameter)?);
        result = next.as_ref();
    }
    Some(SourceSignature {
        parameters,
        result: source_type(result)?,
        span: signature.span,
    })
}

fn source_type(ty: &HirType) -> Option<SourceType> {
    match ty.kind {
        HirTypeKind::Constructor(BuiltinType::Int) => Some(SourceType::Int),
        HirTypeKind::Constructor(BuiltinType::Boolean) => Some(SourceType::Boolean),
        HirTypeKind::Constructor(BuiltinType::String) => Some(SourceType::String),
        HirTypeKind::Constructor(BuiltinType::Unit) => Some(SourceType::Unit),
        _ => None,
    }
}

fn unsupported_shape(
    resolve: &Resolve,
    function: &wit_parser::Function,
    result_kind: &WasiResultKind,
) -> Option<String> {
    if function.params.iter().any(|parameter| {
        matches!(param_kind(resolve, &parameter.ty), WasiParamKind::List)
            && !list_is_bytes(resolve, &parameter.ty)
    }) {
        return Some("non-byte WIT lists are not supported by the String ABI".into());
    }
    if matches!(result_kind, WasiResultKind::List)
        && let Some(result) = &function.result
        && !list_is_bytes(resolve, result)
    {
        return Some("non-byte WIT list results are not supported by the String ABI".into());
    }
    if matches!(result_kind, WasiResultKind::Discarded) {
        return Some(
            "record, tuple, result, and other aggregate WIT results are not supported".into(),
        );
    }
    None
}

fn list_is_bytes(resolve: &Resolve, ty: &WitType) -> bool {
    match ty {
        WitType::String | WitType::U8 => true,
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::List(inner) | TypeDefKind::FixedLengthList(inner, ..) => {
                list_element_is_bytes(resolve, inner)
            }
            TypeDefKind::Type(inner) => list_is_bytes(resolve, inner),
            _ => false,
        },
        _ => false,
    }
}

fn list_element_is_bytes(resolve: &Resolve, ty: &WitType) -> bool {
    match ty {
        WitType::U8 => true,
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::Type(inner) => list_element_is_bytes(resolve, inner),
            _ => false,
        },
        _ => false,
    }
}

/// Classifies a WIT-level parameter so the lowering knows how many canonical
/// parameters a declared argument produces. Aliases are followed.
fn param_kind(resolve: &Resolve, ty: &WitType) -> WasiParamKind {
    match ty {
        WitType::String => WasiParamKind::List,
        WitType::U64 => WasiParamKind::Scalar64 { signed: false },
        WitType::S64 => WasiParamKind::Scalar64 { signed: true },
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::List(_) | TypeDefKind::FixedLengthList(..) => WasiParamKind::List,
            TypeDefKind::Handle(_) => WasiParamKind::Handle,
            TypeDefKind::Type(inner) => param_kind(resolve, inner),
            _ => WasiParamKind::Scalar,
        },
        _ => WasiParamKind::Scalar,
    }
}

/// Classifies a WIT-level result so the lowering knows whether it comes back in
/// a register, through a return pointer, or is discarded. Aliases are followed.
fn result_kind(resolve: &Resolve, ty: &WitType) -> WasiResultKind {
    match ty {
        WitType::String => WasiResultKind::List,
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::List(_) | TypeDefKind::FixedLengthList(..) => WasiResultKind::List,
            TypeDefKind::Handle(_) => WasiResultKind::Scalar,
            TypeDefKind::Result(_) => WasiResultKind::Result,
            TypeDefKind::Enum(_) | TypeDefKind::Flags(_) => WasiResultKind::Scalar,
            TypeDefKind::Type(inner) => result_kind(resolve, inner),
            _ => WasiResultKind::Discarded,
        },
        _ => WasiResultKind::Scalar,
    }
}

fn value_type(ty: WasmType) -> Result<ValueType, String> {
    Ok(match ty {
        WasmType::I32 | WasmType::Pointer | WasmType::Length => ValueType::I32,
        WasmType::I64 | WasmType::PointerOrI64 => ValueType::I64,
        WasmType::F32 => ValueType::F32,
        WasmType::F64 => ValueType::F64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_stdout_and_exit_imports() {
        let mut registry = WasiRegistry::load().expect("WASI WIT should load");
        let stdout = registry
            .import(names::STDOUT, names::GET_STDOUT)
            .expect("get-stdout should resolve");
        assert_eq!(stdout.module, "wasi:cli/stdout@0.2.12");
        assert!(stdout.parameters.is_empty());
        assert!(stdout.param_kinds.is_empty());
        assert_eq!(stdout.result, Some(ValueType::I32));

        let write = registry
            .import(names::STREAMS, names::WRITE_STDOUT)
            .expect("blocking-write-and-flush should resolve");
        assert_eq!(write.module, "wasi:io/streams@0.2.12");
        assert_eq!(
            write.param_kinds,
            vec![WasiParamKind::Handle, WasiParamKind::List]
        );
        assert_eq!(write.result_kind, WasiResultKind::Result);
        assert!(write.retptr);

        let exit = registry
            .import(names::EXIT, names::EXIT_WITH_CODE)
            .expect("exit-with-code should resolve");
        assert_eq!(exit.module, "wasi:cli/exit@0.2.12");
        assert_eq!(exit.parameters, vec![ValueType::I32]);
        assert_eq!(exit.result, None);

        // Interning returns the same symbol for the same import.
        let stdout_again = registry
            .import(names::STDOUT, names::GET_STDOUT)
            .expect("get-stdout should resolve again");
        assert_eq!(stdout.symbol, stdout_again.symbol);
    }

    #[test]
    fn classifies_a_64_bit_parameter_by_its_wit_signedness() {
        let mut registry = WasiRegistry::load().expect("WASI WIT should load");
        let random = registry
            .import("wasi:random/random", "get-random-bytes")
            .expect("get-random-bytes should resolve");
        assert_eq!(
            random.param_kinds,
            vec![WasiParamKind::Scalar64 { signed: false }]
        );
    }
}
