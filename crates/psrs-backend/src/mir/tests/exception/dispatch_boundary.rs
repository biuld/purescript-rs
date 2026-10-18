//! EH-11: rejection boundary. A function whose CFG is irreducible AND
//! contains a try region is rejected with a diagnostic; the same shape
//! without a try region still structures through the unchanged dispatcher.

use super::*;
use crate::TargetCapabilities;
use crate::mir::NumericOp;
use crate::mir::verify_module_with_capabilities;

fn function(blocks: Vec<BasicBlock>, values: &[(u32, ValueType)], result: u32) -> Function {
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "dispatch_fixture".into(),
        parameters: Vec::new(),
        values: declarations(values),
        entry: BlockId(0),
        blocks,
        result: ValueId(result),
        result_type: ValueType::I32,
        frame_weight: 0,
        span: at(950),
    }
}

fn constant(destination: u32, value: i32, span: TextRange) -> Instruction {
    Instruction::Constant {
        destination: ValueId(destination),
        value,
        span,
    }
}

fn primitive(
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

fn jump(target: u32, arguments: Vec<u32>, span: TextRange) -> Terminator {
    Terminator::Jump {
        target: BlockId(target),
        arguments: arguments.into_iter().map(ValueId).collect(),
        span,
    }
}

/// The entry arms the region and both region blocks are entered from outside
/// the cycle, so the strongly connected component has two entries.
fn try_blocks() -> Vec<BasicBlock> {
    vec![
        block(
            0,
            vec![],
            vec![constant(1, 0, at(53)), constant(3, 1, at(54))],
            Terminator::Branch {
                condition: ValueId(3),
                then_block: BlockId(1),
                else_block: BlockId(2),
                span: at(54),
            },
        ),
        block(1, vec![], vec![], jump(4, vec![], at(55))),
        block(2, vec![], vec![], jump(3, vec![], at(56))),
        block(
            3,
            vec![],
            vec![],
            Terminator::Try {
                protected: BlockId(4),
                catches: Vec::new(),
                catch_all: Some(BlockId(5)),
                span: at(51),
            },
        ),
        block(4, vec![], vec![], jump(3, vec![], at(57))),
        block(
            5,
            vec![],
            vec![constant(7, 99, at(58))],
            Terminator::Return {
                value: ValueId(7),
                span: at(58),
            },
        ),
    ]
}

/// The same two-entry cycle without the region: the dispatcher structures it
/// and the artifact runs to a computed result. Blocks 5 and 6 form the cycle,
/// and both are entered from outside it (5 from block 1, 6 from block 2).
fn dispatcher_blocks() -> Vec<BasicBlock> {
    vec![
        block(
            0,
            vec![],
            vec![constant(11, 0, at(60)), constant(3, 1, at(61))],
            Terminator::Branch {
                condition: ValueId(3),
                then_block: BlockId(1),
                else_block: BlockId(2),
                span: at(61),
            },
        ),
        block(1, vec![], vec![], jump(5, vec![11], at(62))),
        block(2, vec![], vec![], jump(6, vec![], at(63))),
        block(
            5,
            vec![1],
            vec![
                constant(2, 1, at(64)),
                primitive(4, NumericOp::I32Add, 1, 2, at(64)),
                constant(8, 5, at(64)),
                primitive(6, NumericOp::I32LtS, 4, 8, at(64)),
            ],
            Terminator::Branch {
                condition: ValueId(6),
                then_block: BlockId(7),
                else_block: BlockId(12),
                span: at(64),
            },
        ),
        block(7, vec![], vec![], jump(9, vec![4], at(66))),
        block(12, vec![], vec![], jump(6, vec![], at(69))),
        block(
            6,
            vec![],
            vec![constant(13, 0, at(70))],
            jump(5, vec![13], at(65)),
        ),
        block(9, vec![9], vec![], jump(10, vec![9], at(67))),
        block(
            10,
            vec![10],
            vec![],
            Terminator::Return {
                value: ValueId(10),
                span: at(68),
            },
        ),
    ]
}

fn try_fixture() -> crate::mir::Module {
    crate::mir::Module {
        name: "DispatchBoundary".into(),
        types: Vec::new(),
        strings: Vec::new(),
        tags: vec![tag(0, Vec::new())],
        layout: None,
        imports: Vec::new(),
        functions: vec![function(
            try_blocks(),
            &[
                (1, ValueType::I32),
                (3, ValueType::Boolean),
                (7, ValueType::I32),
            ],
            7,
        )],
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        span: at(960),
    }
}

fn dispatcher_fixture() -> crate::mir::Module {
    crate::mir::Module {
        name: "DispatchUnchanged".into(),
        types: Vec::new(),
        strings: Vec::new(),
        tags: Vec::new(),
        layout: None,
        imports: Vec::new(),
        functions: vec![function(
            dispatcher_blocks(),
            &[
                (1, ValueType::I32),
                (2, ValueType::I32),
                (3, ValueType::Boolean),
                (4, ValueType::I32),
                (6, ValueType::Boolean),
                (8, ValueType::I32),
                (9, ValueType::I32),
                (10, ValueType::I32),
                (11, ValueType::I32),
                (13, ValueType::I32),
            ],
            10,
        )],
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        span: at(970),
    }
}

#[test]
fn rejects_an_irreducible_cfg_that_contains_a_try_region() {
    let mir = try_fixture();
    verify_module_with_capabilities(&mir, enabled_target()).expect("the fixture verifies");
    let mut wasi = crate::abi::WasiRegistry::load_with_capabilities(enabled_target())
        .expect("the vendored WASI WIT should load");
    let context = crate::linking::default_context().expect("default target context");
    let link = crate::linking::plan_for_module(&context, &mir, &mut wasi, enabled_target())
        .expect("checked target plan");
    let errors = crate::wasm::lower_module_with_plan(&mir, &mut wasi, enabled_target(), &link)
        .expect_err("an irreducible CFG with a try region is rejected");
    assert!(
        errors.iter().any(|error| {
            error.pass == "P10 Wasm structuring"
                && error.span == at(51)
                && error.message.contains("dispatcher cannot arm a try_table")
        }),
        "{errors:#?}"
    );
}

#[test]
fn structures_try_free_irreducible_input_through_the_unchanged_dispatcher() {
    let mir = dispatcher_fixture();
    verify_module_with_capabilities(&mir, TargetCapabilities::default())
        .expect("the try-free fixture verifies under the default profile");
    let mut wasi = crate::abi::WasiRegistry::load_with_capabilities(TargetCapabilities::default())
        .expect("the vendored WASI WIT should load");
    let context = crate::linking::default_context().expect("default target context");
    let link =
        crate::linking::plan_for_module(&context, &mir, &mut wasi, TargetCapabilities::default())
            .expect("checked target plan");
    let wasm =
        crate::wasm::lower_module_with_plan(&mir, &mut wasi, TargetCapabilities::default(), &link)
            .expect("try-free irreducible input still uses the dispatcher");
    // The dispatcher's state local is the one local beyond the ten lowered
    // values (no function parameters).
    assert_eq!(
        wasm.functions[0].locals.len(),
        11,
        "the dispatcher state local"
    );
    if !super::execution::require_engine() {
        return;
    }
    let Some(output) = super::execution::run_output(&mir, TargetCapabilities::default()) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(1),
        "the dispatched blocks must compute the result: {output:?}"
    );
}
