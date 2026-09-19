//! The compiler runtime ABI, derived from the WIT package in `wit/` and lowered
//! to the Component Model canonical ABI. See
//! `docs/decision/DEC-06-runtime-interface-via-wit.md`.

use crate::types::ValueType;
use psrs_hir::{RuntimeFunction, SymbolId};
use wit_parser::Resolve;
use wit_parser::abi::{AbiVariant, WasmType};

/// The WIT package embedded in the compiler. Kept in sync with the runtime
/// operations the frontend exposes.
const RUNTIME_WIT: &str = include_str!("../wit/psrs-runtime.wit");

/// The runtime operations and the WIT function each maps to.
const RUNTIME_FUNCTIONS: &[(RuntimeFunction, &str)] = &[(RuntimeFunction::ConsoleLog, "log")];

/// A runtime operation as a core Wasm import, with its canonical ABI signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeImport {
    pub symbol: SymbolId,
    /// The core import module: the interface's fully-qualified WIT name. This is
    /// the naming `wit-component` expects for a core import of the interface.
    pub module: String,
    pub name: String,
    pub parameters: Vec<ValueType>,
    pub result: Option<ValueType>,
}

/// The runtime imports the backend may emit, keyed by runtime operation.
#[derive(Clone, Debug, Default)]
pub struct RuntimeAbi {
    imports: Vec<RuntimeImport>,
}

impl RuntimeAbi {
    /// Parses the embedded WIT package and lowers each runtime operation to its
    /// canonical ABI import signature.
    pub fn load() -> Result<Self, String> {
        let mut resolve = Resolve::default();
        let package_id = resolve
            .push_str("psrs-runtime.wit", RUNTIME_WIT)
            .map_err(|error| format!("invalid runtime WIT: {error}"))?;
        let interface_id = resolve.packages[package_id]
            .interfaces
            .get("runtime")
            .copied()
            .ok_or_else(|| "runtime WIT is missing the `runtime` interface".to_string())?;
        let package = &resolve.packages[package_id];
        let interface = &resolve.interfaces[interface_id];
        let module = match &interface.name {
            Some(name) => format!("{}:{}/{}", package.name.namespace, package.name.name, name),
            None => format!("{}:{}", package.name.namespace, package.name.name),
        };
        let mut imports = Vec::new();
        for (function, wit_name) in RUNTIME_FUNCTIONS {
            let Some(wit_function) = interface.functions.get(*wit_name) else {
                return Err(format!("runtime WIT is missing `{wit_name}`"));
            };
            let signature = resolve.wasm_signature(AbiVariant::GuestImport, wit_function);
            let parameters = signature
                .params
                .iter()
                .copied()
                .map(value_type)
                .collect::<Result<Vec<_>, _>>()?;
            let result = match signature.results.as_slice() {
                [] => None,
                [single] => Some(value_type(*single)?),
                _ => return Err(format!("`{wit_name}` has multiple canonical results")),
            };
            imports.push(RuntimeImport {
                symbol: function.symbol(),
                module: module.clone(),
                name: (*wit_name).to_string(),
                parameters,
                result,
            });
        }
        Ok(Self { imports })
    }

    pub fn imports(&self) -> &[RuntimeImport] {
        &self.imports
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
    fn loads_the_runtime_wit_and_lowers_log_to_ptr_len() {
        let abi = RuntimeAbi::load().expect("runtime WIT should load");
        let log = abi
            .imports()
            .iter()
            .find(|import| import.symbol == RuntimeFunction::ConsoleLog.symbol())
            .expect("log should be present");
        assert_eq!(log.name, "log");
        assert_eq!(log.module, "psrs:runtime/runtime");
        assert_eq!(log.parameters, vec![ValueType::I32, ValueType::I32]);
        assert_eq!(log.result, None);
    }
}
