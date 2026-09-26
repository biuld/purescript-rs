//! `post-return` for an export whose canonical result is `own<T>`.
//!
//! `wasi:cli/run` returns a scalar and has no post-return. An export that
//! returns an owned handle does: the host calls `cabi_post_<name>` with the
//! flat result, and that function releases the handle with `resource.drop`.
//! See `docs/design/backend/wasm/canonical-abi-and-wit.md`.

use super::super::{
    Export, ExportIndex, ExportKind, FuncType, Function, FunctionIndex, Op, TypeIndex,
};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};

/// One guest export whose flat result is a single owned handle.
pub(crate) struct OwnedHandleExport {
    pub core_name: String,
    pub drop_import: FunctionIndex,
    pub symbol: SymbolId,
}

/// Post-returns for exports that return `own<T>`. `wasi:cli/run` is not in
/// this list: its result is a scalar.
pub(crate) fn append_owned_handle_post_returns(
    exports: &[OwnedHandleExport],
    type_index: TypeIndex,
    first_function: FunctionIndex,
    span: TextRange,
) -> Vec<(FuncType, Function, Export)> {
    exports
        .iter()
        .enumerate()
        .map(|(offset, export)| {
            synthesize_owned_handle_post_return(
                &export.core_name,
                type_index,
                FunctionIndex(first_function.0 + offset as u32),
                export.drop_import,
                export.symbol,
                span,
            )
        })
        .collect()
}

/// Legacy core name of the post-return export for `core_export`.
pub(crate) fn post_return_name(core_export: &str) -> String {
    format!("cabi_post_{core_export}")
}

/// A `(i32) -> ()` function that drops the owned handle the export returned.
pub(crate) fn synthesize_owned_handle_post_return(
    core_export: &str,
    type_index: TypeIndex,
    function_index: FunctionIndex,
    drop_import: FunctionIndex,
    symbol: SymbolId,
    span: TextRange,
) -> (FuncType, Function, Export) {
    let signature = FuncType {
        parameters: vec![ValType::I32],
        results: Vec::new(),
    };
    let function = Function {
        symbol,
        name: post_return_name(core_export),
        type_index,
        parameters: vec![ValType::I32],
        locals: Vec::new(),
        body: vec![
            Op::Leaf(Instruction::LocalGet(0)),
            Op::Leaf(Instruction::Call(drop_import.0)),
        ],
        span,
    };
    let export = Export {
        name: post_return_name(core_export),
        kind: ExportKind::Function,
        index: ExportIndex::Function(function_index),
    };
    (signature, function, export)
}

#[cfg(test)]
mod tests {
    use super::super::super::{
        Export, ExportIndex, ExportKind, FuncType, Function, FunctionIndex, Import, Memory,
        MemoryIndex, Module, Op, TypeIndex,
    };
    use super::{post_return_name, synthesize_owned_handle_post_return};
    use crate::component::componentize;
    use psrs_hir::{ModuleId, SymbolId};
    use psrs_span::TextRange;
    use wasm_encoder::{Instruction, ValType};
    use wit_parser::{Resolve, WorldItem};

    fn span() -> TextRange {
        TextRange::new(0, 1)
    }

    #[test]
    fn post_return_drops_an_owned_export_handle() {
        let wit = r#"
            package fixture:handles@0.1.0;
            interface types {
                resource thing;
            }
            world guest {
                import types;
                use types.{thing};
                export take: func() -> thing;
            }
        "#;
        let mut resolve = Resolve::default();
        let package = resolve
            .push_str("handles.wit", wit)
            .expect("the handle fixture should resolve");
        let world = resolve.packages[package]
            .worlds
            .get("guest")
            .copied()
            .expect("guest world");
        let export = match &resolve.worlds[world].exports.values().next() {
            Some(WorldItem::Function(function)) => function.name.clone(),
            other => panic!("expected one function export, found {other:?}"),
        };
        assert_eq!(export, "take");
        assert_eq!(post_return_name(&export), "cabi_post_take");

        let drop_type = TypeIndex(0);
        let take_type = TypeIndex(1);
        let post_type = TypeIndex(0);
        let (post_signature, post_function, post_export) = synthesize_owned_handle_post_return(
            &export,
            post_type,
            FunctionIndex(2),
            FunctionIndex(0),
            SymbolId::new(ModuleId(0), 1),
            span(),
        );
        assert_eq!(post_signature.parameters, vec![ValType::I32]);
        assert!(post_signature.results.is_empty());
        assert_eq!(post_export.name, "cabi_post_take");

        let module = Module {
            name: "Handles".into(),
            imports: vec![Import {
                module: "fixture:handles/types@0.1.0".into(),
                name: "[resource-drop]thing".into(),
                type_index: drop_type,
            }],
            types: vec![
                FuncType {
                    parameters: vec![ValType::I32],
                    results: Vec::new(),
                },
                FuncType {
                    parameters: Vec::new(),
                    results: vec![ValType::I32],
                },
            ],
            type_defs: Vec::new(),
            functions: vec![
                Function {
                    symbol: SymbolId::new(ModuleId(0), 0),
                    name: export.clone(),
                    type_index: take_type,
                    parameters: Vec::new(),
                    locals: Vec::new(),
                    body: vec![Op::Leaf(Instruction::I32Const(1))],
                    span: span(),
                },
                post_function,
            ],
            memories: vec![Memory {
                id: crate::types::MemoryId(0),
                index: MemoryIndex(0),
                minimum: 1,
                maximum: None,
            }],
            data: Vec::new(),
            exports: vec![
                Export {
                    name: export,
                    kind: ExportKind::Function,
                    index: ExportIndex::Function(FunctionIndex(1)),
                },
                post_export,
                Export {
                    name: "memory".into(),
                    kind: ExportKind::Memory,
                    index: ExportIndex::Memory(MemoryIndex(0)),
                },
            ],
            entry: None,
            realloc: None,
            globals: Vec::new(),
            helpers: Vec::new(),
            span: span(),
        };
        let core = crate::wasm::encode_module(&module).expect("encoding the core module");
        let component = componentize(&core, &resolve, world).expect("componentizing post-return");
        crate::validator()
            .validate_all(&component)
            .expect("the component should validate");
        let text = wasmprinter::print_bytes(&component).expect("printing the component");
        assert!(
            text.contains("resource.drop"),
            "post-return should lower to canon resource.drop: {text}"
        );
        assert!(
            text.contains("cabi_post_take") || text.contains("post-return"),
            "the export should have a post-return: {text}"
        );
    }
}
