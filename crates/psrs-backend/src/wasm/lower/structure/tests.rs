use super::super::lower_function;
use crate::mir::{
    BasicBlock, BlockId, Function as MirFunction, Instruction, NumericOp, Terminator,
};
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use crate::wasm::{
    self, Export, ExportIndex, ExportKind, FuncType, Function, FunctionIndex, Module, TypeIndex,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use std::collections::HashMap;
use wasm_encoder::{Instruction as WasmInstruction, ValType};

pub(super) fn span() -> TextRange {
    TextRange::new(0, 1)
}

pub(super) fn lower_and_validate(source: &MirFunction) -> (Function, Vec<u8>) {
    crate::mir::verify_module(&crate::mir::Module {
        name: source.name.clone(),
        types: Vec::new(),
        strings: Vec::new(),
        imports: Vec::new(),
        functions: vec![source.clone()],
        entry: Some(source.symbol),
        span: span(),
    })
    .expect("the loop fixture should satisfy MIR invariants");
    let lowered = lower_function(
        source,
        TypeIndex(0),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    )
    .unwrap();
    let module = Module {
        name: source.name.clone(),
        imports: Vec::new(),
        types: vec![FuncType {
            parameters: vec![ValType::I32],
            results: vec![ValType::I32],
        }],
        type_defs: Vec::new(),
        functions: vec![lowered.clone()],
        memories: Vec::new(),
        data: Vec::new(),
        exports: vec![Export {
            name: source.name.clone(),
            kind: ExportKind::Function,
            index: ExportIndex::Function(FunctionIndex(0)),
        }],
        entry: None,
        realloc: None,
        globals: Vec::new(),
        helpers: Vec::new(),
        span: span(),
    };
    wasm::verify::verify_module(&module).unwrap();
    let bytes = wasm::encode_module(&module).unwrap();
    crate::validator()
        .validate_all(&bytes)
        .expect("structured natural loops should produce valid Wasm");
    (lowered, bytes)
}

pub(super) fn count_loops(body: &[wasm::Op]) -> usize {
    body.iter()
        .map(|op| match op {
            wasm::Op::Loop { body, .. } => 1 + count_loops(body),
            wasm::Op::Block { body, .. } => count_loops(body),
            wasm::Op::If {
                then_body,
                else_body,
                ..
            } => count_loops(then_body) + count_loops(else_body),
            wasm::Op::Leaf(_) => 0,
        })
        .sum()
}

fn contains_branch_to_depth(body: &[wasm::Op], expected: u32) -> bool {
    body.iter().any(|op| match op {
        wasm::Op::Leaf(WasmInstruction::Br(depth) | WasmInstruction::BrIf(depth)) => {
            *depth == expected
        }
        wasm::Op::Leaf(WasmInstruction::BrTable(targets, default)) => {
            *default == expected || targets.contains(&expected)
        }
        wasm::Op::If {
            then_body,
            else_body,
            ..
        } => {
            contains_branch_to_depth(then_body, expected)
                || contains_branch_to_depth(else_body, expected)
        }
        wasm::Op::Block { body, .. } | wasm::Op::Loop { body, .. } => {
            contains_branch_to_depth(body, expected)
        }
        wasm::Op::Leaf(_) => false,
    })
}

pub(super) fn function(
    name: &str,
    values: Vec<ValueDecl>,
    blocks: Vec<BasicBlock>,
    result: ValueId,
) -> MirFunction {
    MirFunction {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: name.into(),
        parameters: vec![ValueId(0)],
        values,
        entry: BlockId(0),
        blocks,
        result,
        result_type: ValueType::I32,
        span: span(),
    }
}

pub(super) fn block(
    id: u32,
    parameters: Vec<ValueId>,
    instructions: Vec<Instruction>,
    terminator: Terminator,
) -> BasicBlock {
    BasicBlock {
        id: BlockId(id),
        parameters,
        instructions,
        terminator: Some(terminator),
    }
}

#[test]
fn lowers_a_natural_loop_with_a_preheader_and_loop_carried_values() {
    let values = [
        (0, ValueType::I32),
        (1, ValueType::I32),
        (2, ValueType::I32),
        (3, ValueType::I32),
        (4, ValueType::Boolean),
        (5, ValueType::I32),
        (6, ValueType::I32),
        (7, ValueType::I32),
        (8, ValueType::I32),
    ]
    .into_iter()
    .map(|(id, ty)| ValueDecl {
        id: ValueId(id),
        ty,
    })
    .collect();
    let mir = function(
        "natural_loop",
        values,
        vec![
            block(
                0,
                Vec::new(),
                vec![
                    Instruction::Constant {
                        destination: ValueId(2),
                        value: 0,
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: ValueId(8),
                        value: 1,
                        span: span(),
                    },
                ],
                Terminator::Jump {
                    target: BlockId(1),
                    arguments: vec![ValueId(0), ValueId(2)],
                    span: span(),
                },
            ),
            block(
                1,
                vec![ValueId(1), ValueId(3)],
                vec![Instruction::Primitive {
                    destination: ValueId(4),
                    op: NumericOp::I32Eq,
                    left: ValueId(1),
                    right: ValueId(2),
                    span: span(),
                }],
                Terminator::Branch {
                    condition: ValueId(4),
                    then_block: BlockId(3),
                    else_block: BlockId(2),
                    span: span(),
                },
            ),
            block(
                2,
                Vec::new(),
                vec![
                    Instruction::Primitive {
                        destination: ValueId(5),
                        op: NumericOp::I32Sub,
                        left: ValueId(1),
                        right: ValueId(8),
                        span: span(),
                    },
                    Instruction::Primitive {
                        destination: ValueId(6),
                        op: NumericOp::I32Add,
                        left: ValueId(3),
                        right: ValueId(1),
                        span: span(),
                    },
                ],
                Terminator::Jump {
                    target: BlockId(1),
                    arguments: vec![ValueId(5), ValueId(6)],
                    span: span(),
                },
            ),
            block(
                3,
                Vec::new(),
                Vec::new(),
                Terminator::Jump {
                    target: BlockId(4),
                    arguments: vec![ValueId(3)],
                    span: span(),
                },
            ),
            block(
                4,
                vec![ValueId(7)],
                Vec::new(),
                Terminator::Return {
                    value: ValueId(7),
                    span: span(),
                },
            ),
        ],
        ValueId(7),
    );

    let (lowered, bytes) = lower_and_validate(&mir);
    assert_eq!(count_loops(&lowered.body), 1);
    assert_eq!(
        lowered.locals.len(),
        mir.values.len() - mir.parameters.len(),
        "reducible CFGs should not allocate a dispatcher state local"
    );
    assert!(contains_branch_to_depth(&lowered.body, 0));

    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping natural-loop execution: wasmtime is not installed");
        return;
    }
    let path = std::env::temp_dir().join(format!("psrs-natural-loop-{}.wasm", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg("--invoke")
        .arg("natural_loop")
        .arg(&path)
        .arg("5")
        .output()
        .unwrap();
    let _ = std::fs::remove_file(path);
    assert!(output.status.success(), "wasmtime failed: {output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "15",
        "natural-loop result was incorrect"
    );
}

#[test]
fn lowers_nested_loops_with_multiple_exit_targets() {
    let values = vec![
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
    ];
    let mir = function(
        "nested_loops",
        values,
        vec![
            block(
                0,
                Vec::new(),
                vec![Instruction::Constant {
                    destination: ValueId(1),
                    value: 7,
                    span: span(),
                }],
                Terminator::Branch {
                    condition: ValueId(0),
                    then_block: BlockId(6),
                    else_block: BlockId(1),
                    span: span(),
                },
            ),
            block(
                1,
                Vec::new(),
                Vec::new(),
                Terminator::Branch {
                    condition: ValueId(0),
                    then_block: BlockId(3),
                    else_block: BlockId(2),
                    span: span(),
                },
            ),
            block(
                2,
                Vec::new(),
                Vec::new(),
                Terminator::Jump {
                    target: BlockId(1),
                    arguments: Vec::new(),
                    span: span(),
                },
            ),
            block(
                3,
                Vec::new(),
                Vec::new(),
                Terminator::Branch {
                    condition: ValueId(0),
                    then_block: BlockId(4),
                    else_block: BlockId(5),
                    span: span(),
                },
            ),
            block(
                4,
                Vec::new(),
                Vec::new(),
                Terminator::Jump {
                    target: BlockId(0),
                    arguments: Vec::new(),
                    span: span(),
                },
            ),
            block(
                5,
                Vec::new(),
                Vec::new(),
                Terminator::Return {
                    value: ValueId(1),
                    span: span(),
                },
            ),
            block(
                6,
                Vec::new(),
                Vec::new(),
                Terminator::Return {
                    value: ValueId(1),
                    span: span(),
                },
            ),
            block(
                7,
                vec![ValueId(2)],
                Vec::new(),
                Terminator::Return {
                    value: ValueId(2),
                    span: span(),
                },
            ),
        ],
        ValueId(1),
    );

    let (lowered, _) = lower_and_validate(&mir);
    assert_eq!(count_loops(&lowered.body), 2);
}
