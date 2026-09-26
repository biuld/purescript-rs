use super::verify_static_access_extents;
use crate::mir::{BasicBlock, BlockId, Function, Instruction, Module, Terminator};
use crate::types::{FunctionId, MemoryId, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

mod address;

const ACCESS_SPAN: TextRange = TextRange::new(17, 23);

fn decl(id: u32, ty: ValueType) -> ValueDecl {
    ValueDecl {
        id: ValueId(id),
        ty,
    }
}

fn function(
    values: Vec<ValueDecl>,
    parameters: Vec<ValueId>,
    blocks: Vec<BasicBlock>,
    result: ValueId,
) -> Function {
    let result_type = values
        .iter()
        .find(|value| value.id == result)
        .expect("result value has a declaration")
        .ty;
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "extent_test".into(),
        parameters,
        values,
        entry: BlockId(0),
        blocks,
        result,
        result_type,
        span: ACCESS_SPAN,
    }
}

fn module(function: Function) -> Module {
    Module {
        name: "ExtentTest".into(),
        types: Vec::new(),
        strings: Vec::new(),
        imports: Vec::new(),
        functions: vec![function],
        entry: None,
        span: ACCESS_SPAN,
    }
}

fn block(instructions: Vec<Instruction>, terminator: Terminator) -> BasicBlock {
    BasicBlock {
        id: BlockId(0),
        parameters: Vec::new(),
        instructions,
        terminator: Some(terminator),
    }
}

fn returning(value: ValueId) -> Terminator {
    Terminator::Return {
        value,
        span: ACCESS_SPAN,
    }
}

fn verify(function: Function) -> Result<(), Vec<crate::BackendError>> {
    verify_static_access_extents(&module(function))
}

#[test]
fn allows_reads_and_writes_inside_the_scratch_region() {
    let function = function(
        vec![
            decl(0, ValueType::I32),
            decl(1, ValueType::I32),
            decl(2, ValueType::I32),
            decl(3, ValueType::I32),
        ],
        Vec::new(),
        vec![block(
            vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 0,
                    span: ACCESS_SPAN,
                },
                Instruction::Constant {
                    destination: ValueId(1),
                    value: 7,
                    span: ACCESS_SPAN,
                },
                Instruction::Load {
                    destination: ValueId(2),
                    address: ValueId(0),
                    memory: MemoryId(0),
                    offset: 12,
                    span: ACCESS_SPAN,
                },
                Instruction::Store {
                    address: ValueId(0),
                    value: ValueId(1),
                    memory: MemoryId(0),
                    offset: 12,
                    span: ACCESS_SPAN,
                },
                Instruction::Copy {
                    destination: ValueId(3),
                    value: ValueId(2),
                    span: ACCESS_SPAN,
                },
            ],
            returning(ValueId(3)),
        )],
        ValueId(3),
    );

    verify(function).expect("scratch is a readable and writable ABI region");
}

#[test]
fn allows_reads_and_writes_inside_the_heap_state_region() {
    let function = function(
        vec![
            decl(0, ValueType::I32),
            decl(1, ValueType::I32),
            decl(2, ValueType::I32),
            decl(3, ValueType::I32),
        ],
        Vec::new(),
        vec![block(
            vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: crate::abi::HEAP_STATE as i32,
                    span: ACCESS_SPAN,
                },
                Instruction::Constant {
                    destination: ValueId(1),
                    value: 7,
                    span: ACCESS_SPAN,
                },
                Instruction::Load {
                    destination: ValueId(2),
                    address: ValueId(0),
                    memory: MemoryId(0),
                    offset: 0,
                    span: ACCESS_SPAN,
                },
                Instruction::Store {
                    address: ValueId(0),
                    value: ValueId(1),
                    memory: MemoryId(0),
                    offset: 0,
                    span: ACCESS_SPAN,
                },
                Instruction::Copy {
                    destination: ValueId(3),
                    value: ValueId(2),
                    span: ACCESS_SPAN,
                },
            ],
            returning(ValueId(3)),
        )],
        ValueId(3),
    );

    verify(function).expect("heap state is a readable and writable ABI region");
}

#[test]
fn rejects_accesses_that_cross_the_scratch_boundary() {
    let function = function(
        vec![decl(0, ValueType::I32), decl(1, ValueType::I32)],
        Vec::new(),
        vec![block(
            vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 0,
                    span: ACCESS_SPAN,
                },
                Instruction::Load {
                    destination: ValueId(1),
                    address: ValueId(0),
                    memory: MemoryId(0),
                    offset: 13,
                    span: ACCESS_SPAN,
                },
            ],
            returning(ValueId(1)),
        )],
        ValueId(1),
    );

    let errors = verify(function).expect_err("the access extends past SCRATCH_END");
    assert!(errors[0].message.contains("region boundary"));
}

#[test]
fn rejects_accesses_that_start_in_an_unmapped_gap() {
    let function = function(
        vec![decl(0, ValueType::I32), decl(1, ValueType::I32)],
        Vec::new(),
        vec![block(
            vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 4096,
                    span: ACCESS_SPAN,
                },
                Instruction::Load8U {
                    destination: ValueId(1),
                    address: ValueId(0),
                    memory: MemoryId(0),
                    offset: 0,
                    span: ACCESS_SPAN,
                },
            ],
            returning(ValueId(1)),
        )],
        ValueId(1),
    );

    let errors = verify(function).expect_err("the scratch region ends before this address");
    assert!(
        errors[0]
            .message
            .contains("outside the canonical ABI regions")
    );
    assert_eq!(errors[0].span, ACCESS_SPAN);
}

#[test]
fn allows_dynamic_reads_but_rejects_dynamic_stores_without_proof() {
    let load = function(
        vec![decl(0, ValueType::I32), decl(1, ValueType::I32)],
        vec![ValueId(0)],
        vec![block(
            vec![Instruction::Load {
                destination: ValueId(1),
                address: ValueId(0),
                memory: MemoryId(0),
                offset: 0,
                span: ACCESS_SPAN,
            }],
            returning(ValueId(1)),
        )],
        ValueId(1),
    );
    verify(load).expect("dynamic reads rely on Wasm's current-memory bounds check");

    let store = function(
        vec![decl(0, ValueType::I32), decl(1, ValueType::I32)],
        vec![ValueId(0), ValueId(1)],
        vec![block(
            vec![Instruction::Store {
                address: ValueId(0),
                value: ValueId(1),
                memory: MemoryId(0),
                offset: 0,
                span: ACCESS_SPAN,
            }],
            returning(ValueId(1)),
        )],
        ValueId(1),
    );
    let errors = verify(store).expect_err("dynamic stores need writable-buffer provenance");
    assert!(errors[0].message.contains("writable ABI region"));
}

#[test]
fn allows_dynamic_load8u_at_the_wasm32_fixed_extent_boundary() {
    let function = function(
        vec![decl(0, ValueType::I32), decl(1, ValueType::I32)],
        vec![ValueId(0)],
        vec![block(
            vec![Instruction::Load8U {
                destination: ValueId(1),
                address: ValueId(0),
                memory: MemoryId(0),
                offset: u32::MAX,
                span: ACCESS_SPAN,
            }],
            returning(ValueId(1)),
        )],
        ValueId(1),
    );

    verify(function).expect("offset plus the one-byte width equals 2^32");
}

#[test]
fn rejects_offsets_that_always_exceed_the_wasm32_address_space() {
    let function = function(
        vec![decl(0, ValueType::I32), decl(1, ValueType::I32)],
        vec![ValueId(0)],
        vec![block(
            vec![Instruction::Load {
                destination: ValueId(1),
                address: ValueId(0),
                memory: MemoryId(0),
                offset: u32::MAX,
                span: ACCESS_SPAN,
            }],
            returning(ValueId(1)),
        )],
        ValueId(1),
    );

    let errors = verify(function).expect_err("a 4-byte access cannot fit at this offset");
    assert!(errors[0].message.contains("offset and width"));
}

#[test]
fn rejects_known_effective_addresses_past_the_wasm32_limit() {
    let function = function(
        vec![decl(0, ValueType::I32), decl(1, ValueType::I32)],
        Vec::new(),
        vec![block(
            vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: -1,
                    span: ACCESS_SPAN,
                },
                Instruction::Load8U {
                    destination: ValueId(1),
                    address: ValueId(0),
                    memory: MemoryId(0),
                    offset: 1,
                    span: ACCESS_SPAN,
                },
            ],
            returning(ValueId(1)),
        )],
        ValueId(1),
    );

    let errors = verify(function).expect_err("the effective end is greater than 2^32");
    assert!(errors[0].message.contains("extent exceeds"));
}
