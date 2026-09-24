use super::*;
use crate::mir::{BasicBlock, BlockId, Module, Terminator};
use crate::types::{FunctionId, MemoryId, ValueDecl, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

mod dynamic;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn module_with_access(allocation: Allocation, offset: i32, object_bytes: u32) -> Module {
    let symbol = SymbolId::new(ModuleId(0), 0);
    let (allocation, mut instructions) = match allocation {
        Allocation::Fixed(bytes) => (
            Instruction::LinearAlloc {
                destination: ValueId(0),
                bytes,
                alignment: 4,
                span: span(),
            },
            Vec::new(),
        ),
        Allocation::Dynamic(bytes) => {
            let prefix = vec![
                Instruction::Constant {
                    destination: ValueId(1),
                    value: (bytes / 4) as i32,
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(2),
                    value: 4,
                    span: span(),
                },
                Instruction::Primitive {
                    destination: ValueId(5),
                    op: NumericOp::I32Mul,
                    left: ValueId(1),
                    right: ValueId(2),
                    span: span(),
                },
            ];
            (
                Instruction::LinearAllocDynamic {
                    destination: ValueId(0),
                    bytes: ValueId(5),
                    alignment: 4,
                    span: span(),
                },
                prefix,
            )
        }
    };
    let fixed = matches!(&allocation, Instruction::LinearAlloc { .. });
    instructions.push(allocation);
    let offset_id = if fixed { ValueId(1) } else { ValueId(3) };
    let address_id = if fixed { ValueId(2) } else { ValueId(4) };
    let value_id = if fixed { ValueId(3) } else { ValueId(6) };
    instructions.extend([
        Instruction::Constant {
            destination: offset_id,
            value: offset,
            span: span(),
        },
        Instruction::Primitive {
            destination: address_id,
            op: NumericOp::I32Add,
            left: ValueId(0),
            right: offset_id,
            span: span(),
        },
        Instruction::Constant {
            destination: value_id,
            value: 1,
            span: span(),
        },
        Instruction::LinearStore {
            address: address_id,
            value: value_id,
            memory: MemoryId(0),
            offset: 0,
            object_bytes,
            alignment: 4,
            ty: ValueType::I32,
            span: span(),
        },
    ]);
    let value_count = if fixed { 4 } else { 7 };
    Module {
        name: "LinearBoundsTest".into(),
        types: Vec::new(),
        imports: Vec::new(),
        functions: vec![crate::mir::Function {
            id: FunctionId(0),
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: (0..value_count)
                .map(|index| ValueDecl {
                    id: ValueId(index),
                    ty: ValueType::I32,
                })
                .collect(),
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions,
                terminator: Some(Terminator::Return {
                    value: value_id,
                    span: span(),
                }),
            }],
            result: value_id,
            result_type: ValueType::I32,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    }
}

fn module_with_joined_allocations(object_bytes: u32) -> Module {
    let symbol = SymbolId::new(ModuleId(0), 0);
    Module {
        name: "LinearBoundsJoinTest".into(),
        types: Vec::new(),
        imports: Vec::new(),
        functions: vec![crate::mir::Function {
            id: FunctionId(0),
            symbol,
            name: "main".into(),
            parameters: vec![ValueId(0)],
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: ValueType::Boolean,
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(2),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(3),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(4),
                    ty: ValueType::I32,
                },
            ],
            entry: crate::mir::BlockId(0),
            blocks: vec![
                crate::mir::BasicBlock {
                    id: crate::mir::BlockId(0),
                    parameters: Vec::new(),
                    instructions: Vec::new(),
                    terminator: Some(Terminator::Branch {
                        condition: ValueId(0),
                        then_block: crate::mir::BlockId(1),
                        else_block: crate::mir::BlockId(2),
                        merge_block: crate::mir::BlockId(3),
                        span: span(),
                    }),
                },
                crate::mir::BasicBlock {
                    id: crate::mir::BlockId(1),
                    parameters: Vec::new(),
                    instructions: vec![Instruction::LinearAlloc {
                        destination: ValueId(1),
                        bytes: 8,
                        alignment: 4,
                        span: span(),
                    }],
                    terminator: Some(Terminator::Jump {
                        target: crate::mir::BlockId(3),
                        arguments: vec![ValueId(1)],
                        span: span(),
                    }),
                },
                crate::mir::BasicBlock {
                    id: crate::mir::BlockId(2),
                    parameters: Vec::new(),
                    instructions: vec![Instruction::LinearAlloc {
                        destination: ValueId(2),
                        bytes: 16,
                        alignment: 4,
                        span: span(),
                    }],
                    terminator: Some(Terminator::Jump {
                        target: crate::mir::BlockId(3),
                        arguments: vec![ValueId(2)],
                        span: span(),
                    }),
                },
                crate::mir::BasicBlock {
                    id: crate::mir::BlockId(3),
                    parameters: vec![ValueId(3)],
                    instructions: vec![
                        Instruction::Constant {
                            destination: ValueId(4),
                            value: 1,
                            span: span(),
                        },
                        Instruction::LinearStore {
                            address: ValueId(3),
                            value: ValueId(4),
                            memory: MemoryId(0),
                            offset: 0,
                            object_bytes,
                            alignment: 4,
                            ty: ValueType::I32,
                            span: span(),
                        },
                    ],
                    terminator: Some(Terminator::Return {
                        value: ValueId(4),
                        span: span(),
                    }),
                },
            ],
            result: ValueId(4),
            result_type: ValueType::I32,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    }
}

fn module_with_guarded_constant_offset(
    offset: i32,
    allocation_bytes: u32,
    object_bytes: u32,
) -> Module {
    let symbol = SymbolId::new(ModuleId(0), 0);
    let span = span();
    Module {
        name: "GuardedLinearBoundsTest".into(),
        types: Vec::new(),
        imports: Vec::new(),
        functions: vec![crate::mir::Function {
            id: FunctionId(0),
            symbol,
            name: "main".into(),
            parameters: vec![ValueId(0)],
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: ValueType::Boolean,
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(2),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(3),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(4),
                    ty: ValueType::I32,
                },
            ],
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::LinearAlloc {
                        destination: ValueId(1),
                        bytes: allocation_bytes,
                        alignment: 4,
                        span,
                    },
                    Instruction::TrapIf {
                        condition: ValueId(0),
                        span,
                    },
                    Instruction::Constant {
                        destination: ValueId(2),
                        value: offset,
                        span,
                    },
                    Instruction::Primitive {
                        destination: ValueId(3),
                        op: NumericOp::I32Add,
                        left: ValueId(1),
                        right: ValueId(2),
                        span,
                    },
                    Instruction::Constant {
                        destination: ValueId(4),
                        value: 1,
                        span,
                    },
                    Instruction::LinearStore {
                        address: ValueId(3),
                        value: ValueId(4),
                        memory: MemoryId(0),
                        offset: 0,
                        object_bytes,
                        alignment: 4,
                        ty: ValueType::I32,
                        span,
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: ValueId(4),
                    span,
                }),
            }],
            result: ValueId(4),
            result_type: ValueType::I32,
            span,
        }],
        entry: Some(symbol),
        span,
    }
}

enum Allocation {
    Fixed(u32),
    Dynamic(u32),
}

#[test]
fn rejects_accesses_that_exceed_a_constant_offset_allocation_origin() {
    let module = module_with_access(Allocation::Fixed(8), 4, 8);
    let errors = crate::mir::verify::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("originating allocation")),
        "{errors:?}"
    );
}

#[test]
fn rejects_accesses_after_a_constant_offset_leaves_the_allocation() {
    let module = module_with_access(Allocation::Fixed(8), 12, 4);
    let errors = crate::mir::verify::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("originating allocation")),
        "{errors:?}"
    );
}

#[test]
fn tracks_constant_offsets_after_runtime_guards() {
    let valid = module_with_guarded_constant_offset(4, 8, 4);
    crate::mir::verify::verify_module(&valid)
        .expect("a guard must not discard a safe constant pointer bound");

    let overstated = module_with_guarded_constant_offset(4, 8, 8);
    let errors = crate::mir::verify::verify_module(&overstated).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("originating allocation")),
        "{errors:?}"
    );
}

#[test]
fn tracks_constant_sizes_and_offsets_for_dynamic_allocations() {
    let module = module_with_access(Allocation::Dynamic(12), 8, 4);
    crate::mir::verify::verify_module(&module)
        .expect("a 4-byte store at offset 8 fits a statically sized 12-byte allocation");
}

#[test]
fn joins_pointer_bounds_using_the_smallest_incoming_allocation() {
    let module = module_with_joined_allocations(8);
    crate::mir::verify::verify_module(&module)
        .expect("an 8-byte extent fits both incoming allocations");

    let invalid = module_with_joined_allocations(12);
    let errors = crate::mir::verify::verify_module(&invalid).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("originating allocation")),
        "{errors:?}"
    );
}
