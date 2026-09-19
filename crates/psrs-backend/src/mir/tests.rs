use super::{BasicBlock, BlockId, Function, Instruction, Module, Terminator};
use crate::types::{
    CompositeType, DefinedType, FieldType, RecGroup, StorageType, ValueDecl, ValueId, ValueType,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

/// MIR owns a defined-type table; the Wasm lowering must carry it into the
/// encoded type section ahead of the function types.
#[test]
fn defined_types_flow_into_the_wasm_type_section() {
    let mir = Module {
        name: "MirTypes".into(),
        externals: Vec::new(),
        types: vec![RecGroup(vec![DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Struct(vec![FieldType {
                storage: StorageType::I32,
                mutable: false,
            }]),
        }])],
        functions: vec![Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            }],
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![Instruction::Constant {
                    destination: ValueId(0),
                    value: 42,
                    span: span(),
                }],
                terminator: Some(Terminator::Return {
                    value: ValueId(0),
                    span: span(),
                }),
            }],
            result: ValueId(0),
            result_type: ValueType::I32,
            span: span(),
        }],
        span: span(),
    };

    let wasm = crate::wasm::lower_module(&mir).expect("lowering to Wasm");
    assert_eq!(wasm.defined_type_count(), 1);
    assert_eq!(wasm.functions[0].type_index, 1);
    let binary = crate::wasm::encode_module(&wasm).expect("encoding");
    wasmparser::Validator::new()
        .validate_all(&binary)
        .expect("the encoded module should validate");
}
