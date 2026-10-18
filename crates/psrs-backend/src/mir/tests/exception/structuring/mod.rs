//! EH-10: the structurer's try unit. Fixtures are hand-built direct MIR
//! (I32 values only) verified by `verify_module`, lowered with an explicitly
//! exceptions-enabled target, inspected as WAT, and executed under wasmtime.
//! This module holds the shared fixtures and the try-shape tests; the
//! loop-interaction tests live in `loops`, the dispatcher-local check in
//! `dispatcher_local`.

mod dispatcher_local;
mod loops;

use super::*;
use crate::TargetCapabilities;
use crate::mir::NumericOp;
use crate::mir::verify_module_with_capabilities;

pub(super) fn enabled_target() -> TargetCapabilities {
    TargetCapabilities {
        exceptions: true,
        ..TargetCapabilities::default()
    }
}

pub(super) fn function(
    blocks: Vec<BasicBlock>,
    values: &[(u32, ValueType)],
    parameters: Vec<u32>,
    result: u32,
) -> Function {
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "try_fixture".into(),
        parameters: parameters.into_iter().map(ValueId).collect(),
        values: declarations(values),
        entry: BlockId(0),
        blocks,
        result: ValueId(result),
        result_type: ValueType::I32,
        frame_weight: 0,
        span: at(900),
    }
}

pub(super) fn constant(destination: u32, value: i32, span: TextRange) -> Instruction {
    Instruction::Constant {
        destination: ValueId(destination),
        value,
        span,
    }
}

pub(super) fn primitive(
    destination: u32,
    op: NumericOp,
    left: u32,
    right: u32,
    span: TextRange,
) -> Instruction {
    Instruction::Primitive {
        destination: ValueId(destination),
        op,
        left: ValueId(left),
        right: ValueId(right),
        span,
    }
}

pub(super) fn jump(target: u32, arguments: Vec<u32>, span: TextRange) -> Terminator {
    Terminator::Jump {
        target: BlockId(target),
        arguments: arguments.into_iter().map(ValueId).collect(),
        span,
    }
}

pub(super) fn run(mir: &crate::mir::Module) -> Option<std::process::Output> {
    if !super::execution::require_engine() {
        return None;
    }
    super::execution::run_output(mir, enabled_target())
}

/// The structured WAT of the function, lowered over the one-function module.
pub(super) fn wat(function: Function, tags: Vec<Tag>) -> String {
    let mir = module(function, tags);
    verify_module_with_capabilities(&mir, enabled_target()).expect("the fixture verifies");
    let mut wasi = crate::abi::WasiRegistry::load_with_capabilities(enabled_target())
        .expect("the vendored WASI WIT should load");
    let context = crate::linking::default_context().expect("default target context");
    let link = crate::linking::plan_for_module(&context, &mir, &mut wasi, enabled_target())
        .expect("checked target plan");
    let wasm = crate::wasm::lower_module_with_plan(&mir, &mut wasi, enabled_target(), &link)
        .expect("the MIR should lower to Wasm");
    let binary = crate::wasm::encode_module(&wasm).expect("encoding Wasm");
    wasmprinter::print_bytes(&binary).expect("the tagged module should print")
}

// ------------------------------------------------------------- nested tries

pub(super) fn nested_blocks() -> Vec<BasicBlock> {
    vec![
        block(
            0,
            vec![],
            vec![],
            Terminator::Try {
                protected: BlockId(1),
                catches: Vec::new(),
                catch_all: Some(BlockId(8)),
                span: at(10),
            },
        ),
        block(
            1,
            vec![],
            vec![],
            Terminator::Try {
                protected: BlockId(2),
                catches: Vec::new(),
                catch_all: Some(BlockId(3)),
                span: at(11),
            },
        ),
        block(
            2,
            vec![],
            vec![],
            Terminator::Throw {
                tag: TagId(0),
                arguments: Vec::new(),
                handler: Some(BlockId(3)),
                span: at(12),
            },
        ),
        block(
            3,
            vec![],
            vec![constant(1, 10, at(13))],
            jump(9, vec![1], at(13)),
        ),
        block(
            8,
            vec![],
            vec![constant(2, 20, at(14))],
            jump(9, vec![2], at(14)),
        ),
        block(
            9,
            vec![0],
            vec![],
            Terminator::Return {
                value: ValueId(0),
                span: at(15),
            },
        ),
    ]
}

pub(super) fn nested_values() -> Vec<(u32, ValueType)> {
    vec![
        (0, ValueType::I32),
        (1, ValueType::I32),
        (2, ValueType::I32),
    ]
}

/// Outer try over an inner try; the throw is caught by the INNER handler, so
/// the inner unwind block encloses the `try_table` inside the outer one.
#[test]
fn structures_nested_tries_with_the_inner_try_inside_the_outer() {
    let wat = wat(
        function(nested_blocks(), &nested_values(), vec![], 0),
        vec![tag(0, Vec::new())],
    );
    // Two try regions in the function plus the budget slice's synthesized
    // `run` entry `try_table (catch_all)` — three in the module.
    assert_eq!(wat.matches("try_table").count(), 3, "{wat}");
    let outer = wat.find("try_table").expect("the outer try_table");
    let inner = wat[outer + 1..]
        .find("try_table")
        .expect("the inner try_table")
        + outer
        + 1;
    // The budget prologue's own `throw` (the stack-exhaustion tag) sits at the
    // function's start; the channel throw this test inspects comes after the
    // inner try_table.
    let throw = wat[inner..].find("throw").expect("the channel throw") + inner;
    assert!(outer < inner && inner < throw, "{wat}");

    let mir = module(
        function(nested_blocks(), &nested_values(), vec![], 0),
        vec![tag(0, Vec::new())],
    );
    let Some(output) = run(&mir) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(10),
        "the inner handler must catch the throw: {output:?}"
    );
}

// ------------------------------------------------------------- named catch

/// A named `catch` clause delivers the tag's payload through the synthesized
/// unwind block into the handler's parameter local.
#[test]
fn delivers_a_named_catch_payload_to_the_handler_parameter() {
    let blocks = vec![
        block(
            0,
            vec![],
            vec![],
            Terminator::Try {
                protected: BlockId(1),
                catches: vec![(TagId(0), BlockId(2))],
                catch_all: None,
                span: at(20),
            },
        ),
        block(
            1,
            vec![],
            vec![constant(1, 7, at(21))],
            Terminator::Throw {
                tag: TagId(0),
                arguments: vec![ValueId(1)],
                handler: Some(BlockId(2)),
                span: at(21),
            },
        ),
        block(2, vec![2], vec![], jump(9, vec![2], at(22))),
        block(
            9,
            vec![0],
            vec![],
            Terminator::Return {
                value: ValueId(0),
                span: at(23),
            },
        ),
    ];
    let values = vec![
        (0, ValueType::I32),
        (1, ValueType::I32),
        (2, ValueType::I32),
    ];
    let wat = wat(
        function(blocks.clone(), &values, vec![], 0),
        vec![tag(0, vec![ValueType::I32])],
    );
    assert!(wat.contains("try_table") && wat.contains("throw"), "{wat}");

    let mir = module(
        function(blocks, &values, vec![], 0),
        vec![tag(0, vec![ValueType::I32])],
    );
    let Some(output) = run(&mir) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(7),
        "the handler parameter must receive the thrown payload: {output:?}"
    );
}
