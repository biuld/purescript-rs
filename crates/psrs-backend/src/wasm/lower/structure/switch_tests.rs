use super::tests::{block, function, lower_and_validate, span};
use crate::mir::{BlockId, Instruction, Terminator};
use crate::types::{ValueDecl, ValueId, ValueType};
use crate::wasm;

use wasm_encoder::Instruction as WasmInstruction;

#[test]
fn structures_sparse_negative_switch_tags_and_executes_the_default() {
    // A reducible (acyclic) switch maps sparse signed tags to a dense
    // `br_table` index and routes the default to its own arm. The join has one
    // value, so the arms pass their results through block arguments.
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: ValueType::I32,
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
    ];
    let constant = |id: u32, value: i32| Instruction::Constant {
        destination: ValueId(id),
        value,
        span: span(),
    };
    let mir = function(
        "sparse_switch",
        values,
        vec![
            block(
                0,
                Vec::new(),
                Vec::new(),
                Terminator::Switch {
                    value: ValueId(0),
                    cases: vec![(-7, BlockId(1)), (100, BlockId(2))],
                    default: BlockId(3),
                    span: span(),
                },
            ),
            block(
                1,
                Vec::new(),
                vec![constant(1, 11)],
                Terminator::Jump {
                    target: BlockId(4),
                    arguments: vec![ValueId(1)],
                    span: span(),
                },
            ),
            block(
                2,
                Vec::new(),
                vec![constant(2, 22)],
                Terminator::Jump {
                    target: BlockId(4),
                    arguments: vec![ValueId(2)],
                    span: span(),
                },
            ),
            block(
                3,
                Vec::new(),
                vec![constant(3, 33)],
                Terminator::Jump {
                    target: BlockId(4),
                    arguments: vec![ValueId(3)],
                    span: span(),
                },
            ),
            block(
                4,
                vec![ValueId(4)],
                Vec::new(),
                Terminator::Return {
                    value: ValueId(4),
                    span: span(),
                },
            ),
        ],
        ValueId(4),
    );

    let (lowered, bytes) = lower_and_validate(&mir);
    fn contains_br_table(body: &[wasm::Op]) -> bool {
        body.iter().any(|op| match op {
            wasm::Op::Leaf(WasmInstruction::BrTable(_, _)) => true,
            wasm::Op::If {
                then_body,
                else_body,
                ..
            } => contains_br_table(then_body) || contains_br_table(else_body),
            wasm::Op::Block { body, .. } | wasm::Op::Loop { body, .. } => contains_br_table(body),
            wasm::Op::Leaf(_) => false,
        })
    }
    assert!(
        contains_br_table(&lowered.body),
        "a multi-tag switch must lower to br_table"
    );

    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        if std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1") {
            panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
        }
        eprintln!("skipping sparse-switch execution: wasmtime is not installed");
        return;
    }
    let path = std::env::temp_dir().join(format!("psrs-sparse-switch-{}.wasm", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    for (selector, expected) in [("-7", "11"), ("100", "22"), ("5", "33")] {
        let output = std::process::Command::new("wasmtime")
            .arg("run")
            .arg("--invoke")
            .arg("sparse_switch")
            .arg(&path)
            .arg(selector)
            .output()
            .unwrap();
        assert!(output.status.success(), "wasmtime failed: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            expected,
            "wrong result for selector {selector}"
        );
    }
    let _ = std::fs::remove_file(path);
}
