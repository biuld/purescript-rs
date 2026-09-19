//! The WASI imports the standard library may emit, with their canonical ABI
//! signatures derived from the vendored WASI WIT. The project does not define a
//! separate host ABI. See `docs/decision/DEC-06-runtime-interface-via-wit.md`.

use crate::types::ValueType;
use wit_parser::Resolve;
use wit_parser::abi::{AbiVariant, WasmType};

/// The WASI functions the standard library uses, as
/// `(package, interface, function)`.
const WASI_IMPORTS: &[(&str, &str, &str)] = &[
    ("wasi:cli", "stdout", "get-stdout"),
    (
        "wasi:io",
        "streams",
        "[method]output-stream.blocking-write-and-flush",
    ),
    ("wasi:cli", "exit", "exit-with-code"),
];

/// A WASI import as a core Wasm import with its canonical ABI signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WasiImport {
    /// The core import module: the interface's canonical id, for example
    /// `wasi:cli/stdout@0.2.12`.
    pub module: String,
    /// The core import field, for example `get-stdout`.
    pub name: String,
    pub parameters: Vec<ValueType>,
    pub result: Option<ValueType>,
}

/// Resolves the WASI imports the standard library uses.
pub fn wasi_imports() -> Result<Vec<WasiImport>, String> {
    let mut resolve = Resolve::default();
    crate::component::load_vendored_wasi(&mut resolve)?;
    WASI_IMPORTS
        .iter()
        .map(|(package, interface, function)| {
            resolve_import(&resolve, package, interface, function)
        })
        .collect()
}

fn resolve_import(
    resolve: &Resolve,
    package: &str,
    interface: &str,
    function: &str,
) -> Result<WasiImport, String> {
    let package_id = resolve
        .packages
        .iter()
        .find_map(|(id, candidate)| {
            (format!("{}:{}", candidate.name.namespace, candidate.name.name) == package)
                .then_some(id)
        })
        .ok_or_else(|| format!("WASI package `{package}` is not vendored"))?;
    let interface_id = resolve.packages[package_id]
        .interfaces
        .get(interface)
        .copied()
        .ok_or_else(|| format!("`{package}/{interface}` is not vendored"))?;
    let wit_function = resolve.interfaces[interface_id]
        .functions
        .get(function)
        .ok_or_else(|| format!("`{package}/{interface}.{function}` is not vendored"))?;
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
        _ => return Err(format!("`{function}` has multiple canonical results")),
    };
    Ok(WasiImport {
        module: resolve
            .id_of(interface_id)
            .ok_or_else(|| format!("`{package}/{interface}` has no canonical id"))?,
        name: function.to_string(),
        parameters,
        result,
    })
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
        let imports = wasi_imports().expect("WASI imports should resolve");
        let stdout = imports
            .iter()
            .find(|import| import.name == "get-stdout")
            .expect("get-stdout should be present");
        assert_eq!(stdout.module, "wasi:cli/stdout@0.2.12");
        assert!(stdout.parameters.is_empty());
        assert_eq!(stdout.result, Some(ValueType::I32));

        let exit = imports
            .iter()
            .find(|import| import.name == "exit-with-code")
            .expect("exit-with-code should be present");
        assert_eq!(exit.module, "wasi:cli/exit@0.2.12");
        assert_eq!(exit.parameters, vec![ValueType::I32]);
        assert_eq!(exit.result, None);
    }
}
