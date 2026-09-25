//! Shared fixtures for the unified reducible structurer.
//!
//! Every reducible CFG, with or without loops, is lowered through
//! `region.rs`. These tests exercise diamonds, switches, shared successors,
//! early returns, traps, block parameters, and combinations with natural
//! loops, and assert that reducible input never allocates a dispatcher state
//! local.

use super::tests::span;
use crate::mir::{BlockId, Instruction, NumericOp, Terminator};
use crate::types::{ValueDecl, ValueId, ValueType};

mod acyclic;
mod loops;

fn decls(spec: &[(u32, ValueType)]) -> Vec<ValueDecl> {
    spec.iter()
        .map(|&(id, ty)| ValueDecl {
            id: ValueId(id),
            ty,
        })
        .collect()
}

fn constant(destination: u32, value: i32) -> Instruction {
    Instruction::Constant {
        destination: ValueId(destination),
        value,
        span: span(),
    }
}

fn primitive(destination: u32, op: NumericOp, left: u32, right: u32) -> Instruction {
    Instruction::Primitive {
        destination: ValueId(destination),
        op,
        left: ValueId(left),
        right: ValueId(right),
        span: span(),
    }
}

fn unreachable(destination: u32) -> Instruction {
    Instruction::Unreachable {
        destination: ValueId(destination),
        span: span(),
    }
}

fn jump(target: u32, arguments: Vec<u32>) -> Terminator {
    Terminator::Jump {
        target: BlockId(target),
        arguments: arguments.into_iter().map(ValueId).collect(),
        span: span(),
    }
}

fn branch(condition: u32, then_block: u32, else_block: u32) -> Terminator {
    Terminator::Branch {
        condition: ValueId(condition),
        then_block: BlockId(then_block),
        else_block: BlockId(else_block),
        span: span(),
    }
}

fn switch(value: u32, cases: Vec<(i32, u32)>, default: u32) -> Terminator {
    Terminator::Switch {
        value: ValueId(value),
        cases: cases
            .into_iter()
            .map(|(tag, target)| (tag, BlockId(target)))
            .collect(),
        default: BlockId(default),
        span: span(),
    }
}

fn ret(value: u32) -> Terminator {
    Terminator::Return {
        value: ValueId(value),
        span: span(),
    }
}

fn require_wasmtime(test: &str) -> bool {
    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok()
    {
        return true;
    }
    if std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1") {
        panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
    }
    eprintln!("skipping {test} execution: wasmtime is not installed");
    false
}

fn execute_cases(name: &str, bytes: &[u8], cases: &[(&str, &str)]) {
    let path = std::env::temp_dir().join(format!("psrs-{name}-{}.wasm", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    for (argument, expected) in cases {
        let output = std::process::Command::new("wasmtime")
            .arg("run")
            .arg("--invoke")
            .arg(name)
            .arg(&path)
            .arg(argument)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "wasmtime failed for {name}({argument}): {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            *expected,
            "wrong result for {name}({argument})"
        );
    }
    let _ = std::fs::remove_file(path);
}

fn execute_trap(name: &str, bytes: &[u8], argument: &str) {
    let path = std::env::temp_dir().join(format!("psrs-{name}-{}.wasm", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg("--invoke")
        .arg(name)
        .arg(&path)
        .arg(argument)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(path);
    assert!(
        !output.status.success(),
        "expected a trap for {name}({argument})"
    );
}

fn assert_no_dispatcher(name: &str, lowered: &crate::wasm::Function, mir: &crate::mir::Function) {
    assert_eq!(
        lowered.locals.len(),
        mir.values.len() - mir.parameters.len(),
        "{name}: a reducible CFG must not allocate a dispatcher state local"
    );
}
