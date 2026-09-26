//! Resolves WASI import signatures from the vendored WIT. Imports are named by
//! their WIT interface and function, so the set of WASI functions is not
//! hard-coded in the compiler. See
//! `docs/decision/DEC-06-runtime-interface-via-wit.md`.

use crate::TargetCapabilities;
use crate::types::ValueType;
use psrs_hir::{ModuleId, SymbolId};
use std::collections::HashMap;
use wit_parser::Resolve;
use wit_parser::abi::AbiVariant;

mod classification;
#[cfg(test)]
mod tests;
mod validation;

pub(crate) use classification::source_signature;
use classification::{param_kind, result_kind, unsupported_shape, value_type};
use validation::{flattened_parameter_count, source_parameter_matches, wasi_interface_enabled};

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

/// Reserved MIR symbol for the allocator synthesized after ABI memory layout
/// is known. Calls to this symbol become calls to the local `cabi_realloc`
/// function during Wasm lowering; it is never emitted as a core import.
pub(crate) const REALLOC_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 1);

/// Reserved symbols for the UTF-16 <-> UTF-8 codec at the canonical ABI
/// boundary. P10 synthesizes them as ordinary local Wasm functions; like
/// `REALLOC_SYMBOL` they are never emitted as core imports.
pub(crate) const STRING_TO_BYTES_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 2);
pub(crate) const BYTES_TO_STRING_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 3);

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WasiParamKind {
    /// A WIT `s32` or `u32` flattened to one canonical `i32` parameter.
    Integer32,
    /// A WIT `s8`/`u8`/`s16`/`u16` flattened to one canonical `i32`. `bits` is
    /// the width and `signed` selects sign-extension when masking an `Int`.
    IntegerNarrow { bits: u8, signed: bool },
    /// A WIT `bool` flattened to one canonical `i32` parameter.
    Boolean,
    /// A WIT character flattened to one canonical `i32` parameter.
    Char,
    /// A 64-bit scalar flattened to one canonical `i64`. `signed` selects
    /// sign- or zero-extension when an `Int` argument is widened to it.
    Scalar64 { signed: bool },
    /// A WIT `f32` parameter; source `Number` values are narrowed in P9.
    Float32,
    /// A WIT `f64` parameter represented directly by source `Number`.
    Float64,
    /// A WIT enum with cases that must match a nullary source data type.
    Enum { cases: Vec<String> },
    /// WIT flags represented by a closed source record of Booleans and
    /// flattened to one or more canonical `i32` words.
    Flags { names: Vec<String> },
    /// A closed record whose fields each flatten directly to scalar values.
    Record { fields: Vec<WasiField> },
    /// A resource handle flattened to one canonical `i32` handle.
    Handle,
    /// A string or list flattened to a `(pointer, length)` pair.
    List,
    /// A WIT shape with no source representation in the current ABI subset.
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WasiField {
    pub name: String,
    pub kind: WasiParamKind,
}

/// How a WIT import's result is represented, which decides how the lowering
/// consumes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WasiResultKind {
    /// No result.
    None,
    /// A scalar returned directly in a register.
    Scalar,
    /// A WIT `s8`/`u8`/`s16`/`u16` result returned as a canonical `i32` whose
    /// bits are already the in-range value.
    IntegerNarrow { bits: u8, signed: bool },
    /// A WIT `bool` result represented by the source `Boolean` type.
    Boolean,
    /// A WIT enum with cases that must match a nullary source data type.
    Enum { cases: Vec<String> },
    /// A WIT character returned directly as a canonical `i32`.
    Char,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceType {
    Int,
    Boolean,
    Number,
    Char,
    /// A nullary data type whose cases correspond in order to WIT enum cases.
    Enum {
        cases: Vec<String>,
    },
    /// A closed PureScript record, kept structurally for ABI validation.
    Record {
        fields: Vec<(String, Box<SourceType>)>,
    },
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

impl WasiImport {
    pub(crate) fn has_indirect_parameters(&self) -> bool {
        let flattened = self
            .param_kinds
            .iter()
            .map(flattened_parameter_count)
            .sum::<usize>();
        flattened
            > self
                .parameters
                .len()
                .saturating_sub(usize::from(self.retptr))
    }
}

/// A P9-resolved import pairs WIT's ABI description with the exact source
/// signature needed to recover record field order after CC lowering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoundWasiImport {
    pub import: WasiImport,
    pub signature: SourceSignature,
}

/// Resolves WASI imports against the vendored WIT, interning each distinct
/// `(module, name)` to a stable [`SymbolId`].
pub struct WasiRegistry {
    resolve: Resolve,
    imports: Vec<WasiImport>,
    keys: HashMap<(String, String), usize>,
    target: TargetCapabilities,
}

impl WasiRegistry {
    /// Symbol indices for WASI imports live above the intrinsic and runtime
    /// ranges in the reserved intrinsic module.
    const SYMBOL_BASE: u32 = 1 << 20;

    pub(crate) fn from_resolve(resolve: Resolve, target: TargetCapabilities) -> Self {
        Self {
            resolve,
            imports: Vec::new(),
            keys: HashMap::new(),
            target,
        }
    }

    pub fn load() -> Result<Self, String> {
        Self::load_with_capabilities(TargetCapabilities::default())
    }

    /// Loads the vendored WIT with the service families enabled by `target`.
    pub fn load_with_capabilities(target: TargetCapabilities) -> Result<Self, String> {
        let mut resolve = Resolve::default();
        crate::component::load_vendored_wasi(&mut resolve)?;
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
        let param_kinds: Vec<WasiParamKind> = wit_function
            .params
            .iter()
            .map(|param| param_kind(&self.resolve, &param.ty))
            .collect();
        let result_kind = match &wit_function.result {
            None => WasiResultKind::None,
            Some(ty) => result_kind(&self.resolve, ty),
        };
        let unsupported = unsupported_shape(&self.resolve, wit_function, &result_kind)
            .or_else(|| {
                (!crate::component::component_interface_supported(&module)).then(|| {
                    format!(
                        "WASI interface `{module}` is not in the current component capability profile"
                    )
                })
            })
            .or_else(|| {
                let flattened = param_kinds
                    .iter()
                    .map(flattened_parameter_count)
                    .sum::<usize>();
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
            if !source_parameter_matches(parameter, kind) {
                return Err(format!(
                    "WIT import `{}` has a source parameter with an incompatible type",
                    import.name
                ));
            }
        }
        let valid_result = match &import.result_kind {
            WasiResultKind::None => matches!(&signature.result, SourceType::Unit),
            WasiResultKind::Scalar => match import.result {
                Some(ValueType::I64) => {
                    matches!(&signature.result, SourceType::Int)
                }
                Some(ValueType::I32) => matches!(&signature.result, SourceType::Int),
                Some(ValueType::F32 | ValueType::F64) => {
                    matches!(&signature.result, SourceType::Number)
                }
                _ => false,
            },
            WasiResultKind::IntegerNarrow { .. } => matches!(&signature.result, SourceType::Int),
            WasiResultKind::Boolean => matches!(&signature.result, SourceType::Boolean),
            WasiResultKind::Enum { cases } => matches!(
                &signature.result,
                SourceType::Enum { cases: source } if source == cases
            ),
            WasiResultKind::Char => matches!(&signature.result, SourceType::Char),
            WasiResultKind::List => matches!(&signature.result, SourceType::String),
            WasiResultKind::Result => matches!(&signature.result, SourceType::Unit),
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
