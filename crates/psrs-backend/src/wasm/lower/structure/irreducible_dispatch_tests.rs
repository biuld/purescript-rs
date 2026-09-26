use super::super::lower_function;
use crate::TargetCapabilities;
use crate::mir::{
    BasicBlock, BlockId, Function, Instruction, Module as MirModule, NumericOp, Terminator,
};
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use crate::wasm::{
    self, Export, ExportIndex, ExportKind, FuncType, Function as WasmFunction, FunctionIndex,
    Module as WasmModule, Op, TypeIndex,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::Instruction as WasmInstruction;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn block(
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

fn lower_and_validate(source: &Function) -> (WasmFunction, Vec<u8>) {
    crate::mir::verify_module(&MirModule {
        name: source.name.clone(),
        types: Vec::new(),
        strings: Vec::new(),
        imports: Vec::new(),
        functions: vec![source.clone()],
        entry: Some(source.symbol),
        span: span(),
    })
    .expect("the irreducible fixture should satisfy MIR invariants");
    let lowered = lower_function(
        source,
        TypeIndex(0),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
    )
    .expect("the irreducible CFG should structure through the dispatcher");
    let module = WasmModule {
        name: source.name.clone(),
        imports: Vec::new(),
        types: vec![FuncType {
            parameters: vec![wasm_encoder::ValType::I32],
            results: vec![wasm_encoder::ValType::I32],
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
    wasm::verify::verify_module(&module)
        .expect("dispatcher locals and branch depths should verify");
    let bytes = wasm::encode_module(&module).expect("the dispatcher should encode");
    crate::validator()
        .validate_all(&bytes)
        .expect("the irreducible dispatcher should produce valid Wasm");
    let core_only = TargetCapabilities {
        mutable_globals: false,
        sign_extension: false,
        nontrapping_float_to_int: false,
        multi_value: false,
        bulk_memory: false,
        reference_types: false,
        function_references: false,
        gc: false,
        simd: false,
        relaxed_simd: false,
        tail_call: false,
        multi_memory: false,
        memory64: false,
        exceptions: false,
        extended_const: false,
        wide_arithmetic: false,
        threads: false,
        component_model: false,
        component_async: false,
        component_map: false,
        ..TargetCapabilities::default()
    };
    crate::validator_for(core_only)
        .validate_all(&bytes)
        .expect("the dispatcher should validate with proposal features disabled");
    (lowered, bytes)
}

fn irreducible_function() -> Function {
    let values = [
        (0, ValueType::I32),
        (1, ValueType::I32),
        (2, ValueType::I32),
        (3, ValueType::I32),
        (4, ValueType::I32),
        (5, ValueType::I32),
        (6, ValueType::I32),
        (7, ValueType::Boolean),
        (8, ValueType::I32),
        (9, ValueType::I32),
        (10, ValueType::I32),
        (11, ValueType::Boolean),
        (12, ValueType::I32),
        (13, ValueType::I32),
        (14, ValueType::I32),
        (15, ValueType::I32),
    ]
    .into_iter()
    .map(|(id, ty)| ValueDecl {
        id: ValueId(id),
        ty,
    })
    .collect();
    let switch_span = TextRange::new(30, 32);
    let first_branch_span = TextRange::new(10, 12);
    let second_branch_span = TextRange::new(20, 22);
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "irreducible_dispatch".into(),
        parameters: vec![ValueId(0)],
        values,
        entry: BlockId(0),
        blocks: vec![
            block(
                0,
                Vec::new(),
                Vec::new(),
                Terminator::Switch {
                    value: ValueId(0),
                    cases: vec![(-7, BlockId(1)), (42, BlockId(2))],
                    default: BlockId(9),
                    span: switch_span,
                },
            ),
            block(
                1,
                Vec::new(),
                vec![Instruction::Constant {
                    destination: ValueId(1),
                    value: 2,
                    span: span(),
                }],
                Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(1)],
                    span: span(),
                },
            ),
            block(
                2,
                Vec::new(),
                vec![Instruction::Constant {
                    destination: ValueId(2),
                    value: 3,
                    span: span(),
                }],
                Terminator::Jump {
                    target: BlockId(4),
                    arguments: vec![ValueId(2), ValueId(2)],
                    span: span(),
                },
            ),
            block(
                3,
                vec![ValueId(3)],
                vec![
                    Instruction::Constant {
                        destination: ValueId(4),
                        value: -7,
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: ValueId(5),
                        value: 1,
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: ValueId(6),
                        value: 2,
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: ValueId(14),
                        value: 8,
                        span: span(),
                    },
                    Instruction::Primitive {
                        destination: ValueId(7),
                        op: NumericOp::I32Eq,
                        left: ValueId(0),
                        right: ValueId(4),
                        span: span(),
                    },
                ],
                Terminator::Branch {
                    condition: ValueId(7),
                    then_block: BlockId(5),
                    else_block: BlockId(6),
                    span: first_branch_span,
                },
            ),
            block(
                4,
                vec![ValueId(10), ValueId(15)],
                vec![
                    Instruction::Constant {
                        destination: ValueId(12),
                        value: 5,
                        span: span(),
                    },
                    Instruction::Primitive {
                        destination: ValueId(11),
                        op: NumericOp::I32Eq,
                        left: ValueId(15),
                        right: ValueId(12),
                        span: span(),
                    },
                ],
                Terminator::Branch {
                    condition: ValueId(11),
                    then_block: BlockId(7),
                    else_block: BlockId(8),
                    span: second_branch_span,
                },
            ),
            block(
                5,
                Vec::new(),
                vec![Instruction::Primitive {
                    destination: ValueId(8),
                    op: NumericOp::I32Add,
                    left: ValueId(3),
                    right: ValueId(5),
                    span: span(),
                }],
                Terminator::Jump {
                    target: BlockId(4),
                    arguments: vec![ValueId(14), ValueId(8)],
                    span: span(),
                },
            ),
            block(
                6,
                Vec::new(),
                vec![Instruction::Primitive {
                    destination: ValueId(9),
                    op: NumericOp::I32Add,
                    left: ValueId(3),
                    right: ValueId(6),
                    span: span(),
                }],
                Terminator::Jump {
                    target: BlockId(4),
                    arguments: vec![ValueId(14), ValueId(9)],
                    span: span(),
                },
            ),
            block(
                7,
                Vec::new(),
                Vec::new(),
                Terminator::Return {
                    value: ValueId(15),
                    span: span(),
                },
            ),
            block(
                8,
                Vec::new(),
                Vec::new(),
                Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(15)],
                    span: span(),
                },
            ),
            block(
                9,
                Vec::new(),
                vec![Instruction::Constant {
                    destination: ValueId(13),
                    value: 99,
                    span: span(),
                }],
                Terminator::Return {
                    value: ValueId(13),
                    span: span(),
                },
            ),
        ],
        result: ValueId(0),
        result_type: ValueType::I32,
        span: span(),
    }
}

fn has_if_span(body: &[Op], span: TextRange) -> bool {
    body.iter().any(|op| match op {
        Op::If {
            then_body,
            else_body,
            span: op_span,
            ..
        } => *op_span == span || has_if_span(then_body, span) || has_if_span(else_body, span),
        Op::Block { body, .. } | Op::Loop { body, .. } => has_if_span(body, span),
        Op::Leaf(_) => false,
    })
}

#[test]
fn structures_and_executes_irreducible_cfg_with_block_parameters_and_sparse_switch() {
    let function = irreducible_function();
    let mir = MirModule {
        name: function.name.clone(),
        types: Vec::new(),
        strings: Vec::new(),
        imports: Vec::new(),
        functions: vec![function.clone()],
        entry: Some(function.symbol),
        span: span(),
    };
    crate::mir::verify_module(&mir).expect("the irreducible CFG should satisfy MIR invariants");

    let (lowered, bytes) = lower_and_validate(&function);
    assert_eq!(
        lowered.locals.len(),
        function.values.len() - function.parameters.len() + 1
    );
    assert!(has_if_span(&lowered.body, TextRange::new(10, 12)));
    assert!(has_if_span(&lowered.body, TextRange::new(20, 22)));
    assert!(has_if_span(&lowered.body, TextRange::new(30, 32)));

    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping irreducible-CFG execution: wasmtime is not installed");
        return;
    }
    let path = std::env::temp_dir().join(format!(
        "psrs-irreducible-dispatch-{}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, bytes).unwrap();
    for (selector, expected) in [("-7", "5"), ("42", "5"), ("13", "99")] {
        let output = std::process::Command::new("wasmtime")
            .arg("run")
            .arg("--invoke")
            .arg("irreducible_dispatch")
            .arg(&path)
            .arg(selector)
            .output()
            .unwrap();
        assert!(output.status.success(), "wasmtime failed: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            expected,
            "wrong result for sparse switch selector {selector}"
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn dispatcher_uses_a_br_table_and_explicit_trap_for_invalid_state() {
    let function = irreducible_function();
    let (lowered, _) = lower_and_validate(&function);
    fn contains_dispatch(body: &[Op]) -> bool {
        body.iter().any(|op| match op {
            Op::Leaf(WasmInstruction::BrTable(targets, default)) => {
                !targets.is_empty() && *default == targets.len() as u32
            }
            Op::If {
                then_body,
                else_body,
                ..
            } => contains_dispatch(then_body) || contains_dispatch(else_body),
            Op::Block { body, .. } | Op::Loop { body, .. } => contains_dispatch(body),
            Op::Leaf(_) => false,
        })
    }
    assert!(contains_dispatch(&lowered.body));
    let loop_body = lowered.body.iter().find_map(|op| match op {
        Op::Loop { body, .. } => Some(body),
        _ => None,
    });
    assert!(matches!(
        loop_body,
        Some(body) if matches!(
            body.get(1),
            Some(Op::Leaf(WasmInstruction::Unreachable))
        )
    ));
}
