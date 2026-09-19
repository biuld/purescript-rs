//! Resolves WASI import signatures from the vendored WIT. Imports are named by
//! their WIT interface and function, so the set of WASI functions is not
//! hard-coded in the compiler. See
//! `docs/decision/DEC-06-runtime-interface-via-wit.md`.

use crate::types::ValueType;
use psrs_hir::{ModuleId, SymbolId};
use std::collections::HashMap;
use wit_parser::Resolve;
use wit_parser::abi::{AbiVariant, WasmType};

/// The core export name `wit-component` expects for the exported interface
/// function `wasi:cli/run.run` under its legacy mangling.
pub const RUN_CORE_EXPORT: &str = "wasi:cli/run@0.2.12#run";

/// Scratch linear-memory address passed as the return pointer to WASI calls
/// whose result does not fit in a single canonical result.
pub const PRINT_SCRATCH: i32 = 0;

/// Linear-memory address of the newline byte the standard `print` appends.
pub const NEWLINE_ADDR: u32 = 12;

/// The WASI interfaces and functions the standard library uses.
pub mod names {
    pub const STDOUT: &str = "wasi:cli/stdout";
    pub const STDERR: &str = "wasi:cli/stderr";
    pub const STREAMS: &str = "wasi:io/streams";
    pub const EXIT: &str = "wasi:cli/exit";
    pub const MONOTONIC_CLOCK: &str = "wasi:clocks/monotonic-clock";
    pub const GET_STDOUT: &str = "get-stdout";
    pub const GET_STDERR: &str = "get-stderr";
    pub const WRITE_STDOUT: &str = "[method]output-stream.blocking-write-and-flush";
    pub const EXIT_WITH_CODE: &str = "exit-with-code";
    pub const NOW: &str = "now";
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
    pub result: Option<ValueType>,
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
            retptr: signature.retptr,
        });
        self.keys.insert(key, self.imports.len() - 1);
        Ok(self.imports.last().expect("just pushed").clone())
    }

    pub fn imports(&self) -> &[WasiImport] {
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
    fn resolves_stdout_and_exit_imports() {
        let mut registry = WasiRegistry::load().expect("WASI WIT should load");
        let stdout = registry
            .import(names::STDOUT, names::GET_STDOUT)
            .expect("get-stdout should resolve");
        assert_eq!(stdout.module, "wasi:cli/stdout@0.2.12");
        assert!(stdout.parameters.is_empty());
        assert_eq!(stdout.result, Some(ValueType::I32));

        let write = registry
            .import(names::STREAMS, names::WRITE_STDOUT)
            .expect("blocking-write-and-flush should resolve");
        assert_eq!(write.module, "wasi:io/streams@0.2.12");
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
}
