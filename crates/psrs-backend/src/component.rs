//! Component Model encoding. A core module produced by the Wasm lowering is
//! componentized with `wit-component`, using WIT metadata that describes the
//! world the module implements. See
//! `docs/decision/DEC-06-runtime-interface-via-wit.md`.

use wit_component::{ComponentEncoder, StringEncoding, embed_component_metadata};
use wit_parser::{Resolve, WorldId};

/// The minimal application world used before the WASI command world is linked.
const APP_WIT: &str = include_str!("../wit/psrs-app.wit");

/// Parses the `psrs:app` world and returns its resolved world ID.
pub fn app_world() -> Result<(Resolve, WorldId), String> {
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str("psrs-app.wit", APP_WIT)
        .map_err(|error| format!("invalid application WIT: {error}"))?;
    let world = resolve.packages[package]
        .worlds
        .get("app")
        .copied()
        .ok_or_else(|| "application WIT is missing the `app` world".to_string())?;
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
        .map_err(|error| format!("failed to encode the component: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::{Export, ExportKind, FuncType, Function, Module, Op};
    use psrs_hir::{ModuleId, SymbolId};
    use psrs_span::TextRange;
    use wasm_encoder::{Instruction, ValType};

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
                name: "run".into(),
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
                name: "run".into(),
                kind: ExportKind::Function,
                index: 0,
            }],
            entry: None,
            span,
        };
        crate::wasm::encode_module(&module).expect("encoding the core module")
    }

    #[test]
    fn componentizes_a_core_module_for_the_application_world() {
        let (resolve, world) = app_world().expect("application WIT should load");
        let core = core_module_exporting_run();
        let component = componentize(&core, &resolve, world).expect("componentizing");
        wasmparser::Validator::new()
            .validate_all(&component)
            .expect("the component should validate");
        let text = wasmprinter::print_bytes(&component).expect("printing the component");
        assert!(
            text.contains("(component") && text.contains("\"run\""),
            "the component should export `run`: {text}"
        );
    }
}
