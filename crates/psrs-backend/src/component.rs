//! Component Model encoding. A core module produced by the Wasm lowering is
//! componentized with `wit-component`, using WIT metadata that describes the
//! world the module implements. See
//! `docs/decision/DEC-06-runtime-interface-via-wit.md`.

use wit_component::{ComponentEncoder, StringEncoding, embed_component_metadata};
use wit_parser::{Resolve, WorldId};

/// Vendored WASI 0.2.12 WIT, matching the `wasi:cli/run@0.2.12` export that the
/// pinned `wasmtime` baseline expects. Pushed into the `Resolve` in dependency
/// order before the application world.
const WASI_DEPS: &[(&str, &str)] = &[
    ("wasi/io.wit", include_str!("../wit/deps/io.wit")),
    ("wasi/clocks.wit", include_str!("../wit/deps/clocks.wit")),
    ("wasi/random.wit", include_str!("../wit/deps/random.wit")),
    (
        "wasi/filesystem.wit",
        include_str!("../wit/deps/filesystem.wit"),
    ),
    ("wasi/sockets.wit", include_str!("../wit/deps/sockets.wit")),
    ("wasi/cli.wit", include_str!("../wit/deps/cli.wit")),
];

/// The application world: a WASI command that only exports `wasi:cli/run`.
const APP_WIT: &str = include_str!("../wit/psrs-app.wit");

/// Resolves the `psrs:app` command world against the vendored WASI WIT.
pub fn command_world() -> Result<(Resolve, WorldId), String> {
    let mut resolve = Resolve::default();
    for (path, contents) in WASI_DEPS {
        resolve
            .push_str(path, contents)
            .map_err(|error| format!("invalid vendored WIT `{path}`: {error}"))?;
    }
    let package = resolve
        .push_str("psrs-app.wit", APP_WIT)
        .map_err(|error| format!("invalid application WIT: {error}"))?;
    let world = resolve.packages[package]
        .worlds
        .get("command")
        .copied()
        .ok_or_else(|| "application WIT is missing the `command` world".to_string())?;
    Ok((resolve, world))
}

/// Lifts a core module into a component for `world`. The core module must
/// implement the world at the canonical ABI level.
pub fn componentize(core: &[u8], resolve: &Resolve, world: WorldId) -> Result<Vec<u8>, String> {
    let mut bytes = core.to_vec();
    embed_component_metadata(&mut bytes, resolve, world, StringEncoding::UTF8)
        .map_err(|error| format!("failed to embed component metadata: {error}"))?;
    ComponentEncoder::default()
        .module(&bytes)
        .map_err(|error| format!("failed to read the core module: {error}"))?
        .validate(true)
        .encode()
        .map_err(|error| format!("failed to encode the component: {error:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::{Export, ExportKind, FuncType, Function, Module, Op};
    use psrs_hir::{ModuleId, SymbolId};
    use psrs_span::TextRange;
    use wasm_encoder::{Instruction, ValType};

    /// The core name `wit-component` expects for an exported interface function
    /// under the legacy mangling it uses for a decoded world:
    /// `{interface-id}#{func}`.
    const RUN_CORE_NAME: &str = "wasi:cli/run@0.2.12#run";

    fn core_module_exporting_run() -> Vec<u8> {
        let span = TextRange::new(0, 1);
        let module = Module {
            name: "App".into(),
            imports: Vec::new(),
            types: vec![FuncType {
                parameters: Vec::new(),
                results: vec![ValType::I32],
            }],
            type_defs: Vec::new(),
            functions: vec![Function {
                symbol: SymbolId::new(ModuleId(0), 0),
                name: RUN_CORE_NAME.into(),
                type_index: 0,
                parameters: Vec::new(),
                locals: Vec::new(),
                body: vec![Op::Leaf(Instruction::I32Const(0))],
                span,
            }],
            runtime_functions: Vec::new(),
            memories: Vec::new(),
            data: Vec::new(),
            exports: vec![Export {
                name: RUN_CORE_NAME.into(),
                kind: ExportKind::Function,
                index: 0,
            }],
            entry: None,
            span,
        };
        crate::wasm::encode_module(&module).expect("encoding the core module")
    }

    #[test]
    fn componentizes_a_command_exporting_run() {
        let (resolve, world) = command_world().expect("WASI and application WIT should load");
        let core = core_module_exporting_run();
        let component = componentize(&core, &resolve, world).expect("componentizing");
        wasmparser::Validator::new()
            .validate_all(&component)
            .expect("the component should validate");
        let text = wasmprinter::print_bytes(&component).expect("printing the component");
        assert!(
            text.contains("(component") && text.contains("wasi:cli/run@0.2.12"),
            "the component should export the WASI run interface: {text}"
        );
    }

    #[test]
    fn runs_the_command_when_wasmtime_is_available() {
        if std::process::Command::new("wasmtime")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping: wasmtime is not installed");
            return;
        }
        let (resolve, world) = command_world().expect("WASI and application WIT should load");
        let core = core_module_exporting_run();
        let component = componentize(&core, &resolve, world).expect("componentizing");
        let path = std::env::temp_dir().join(format!("psrs-command-{}.wasm", std::process::id()));
        std::fs::write(&path, &component).unwrap();
        let output = std::process::Command::new("wasmtime")
            .arg("run")
            .arg(&path)
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(
            output.status.success(),
            "wasmtime failed to run the component: {output:?}"
        );
    }
}
