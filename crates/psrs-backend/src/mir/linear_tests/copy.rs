use super::{run_linear_mir, span};
use crate::mir::{BasicBlock, BlockId, Function, Instruction, Module, Terminator};
use crate::types::{FunctionId, MemoryId, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};

#[test]
fn lowers_overlapping_linear_memory_copies_under_core_mvp() {
    let pointer = ValueId(0);
    let initial = ValueId(1);
    let byte_count = ValueId(2);
    let result = ValueId(3);
    let span = span();
    let module = Module {
        name: "LinearCopyOverlap".into(),
        types: Vec::new(),
        imports: Vec::new(),
        functions: vec![Function {
            id: FunctionId(0),
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: Vec::new(),
            values: [pointer, initial, byte_count, result]
                .into_iter()
                .map(|id| ValueDecl {
                    id,
                    ty: ValueType::I32,
                })
                .collect(),
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::LinearAlloc {
                        destination: pointer,
                        bytes: 8,
                        alignment: 4,
                        span,
                    },
                    Instruction::Constant {
                        destination: initial,
                        value: 0x0807_0605,
                        span,
                    },
                    Instruction::Constant {
                        destination: byte_count,
                        value: 4,
                        span,
                    },
                    Instruction::LinearStore {
                        address: pointer,
                        value: initial,
                        memory: MemoryId(0),
                        offset: 0,
                        object_bytes: 8,
                        alignment: 4,
                        ty: ValueType::I32,
                        span,
                    },
                    Instruction::LinearMemoryCopy {
                        destination: pointer,
                        source: pointer,
                        bytes: byte_count,
                        destination_offset: 2,
                        source_offset: 0,
                        span,
                    },
                    Instruction::LinearMemoryCopy {
                        destination: pointer,
                        source: pointer,
                        bytes: byte_count,
                        destination_offset: 0,
                        source_offset: 2,
                        span,
                    },
                    Instruction::LinearLoad {
                        destination: result,
                        address: pointer,
                        memory: MemoryId(0),
                        offset: 0,
                        object_bytes: 8,
                        alignment: 4,
                        ty: ValueType::I32,
                        span,
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: result,
                    span,
                }),
            }],
            result,
            result_type: ValueType::I32,
            span,
        }],
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        span,
    };

    run_linear_mir(module, "134678021", crate::TargetCapabilities::wasm_mvp());
}
