//! Root fixtures whose historical owners were post-construction recognizers.
//!
//! Start's status-cell length is rejected by the P8 boundary. Sharing one fresh
//! root across exclusive successors, distributing Close, excluding a trapping
//! alternative, and copying the root reference are ordinary pre-optimization
//! MIR shapes. Generic MIR verification accepts them. A missing normal Close
//! stays a P8 rejection in `root`, not a new MIR recognizer.

use super::{SOURCE, binding};
use psrs_backend::cc::AssignmentKind as A;
use psrs_backend::cc::ControlHelperRole as R;
use psrs_backend::mir::{BasicBlock, BlockId, Instruction as I, Module, Terminator as T};
use psrs_backend::types::{ValueDecl, ValueId, ValueType};
use psrs_hir::SymbolId;
use std::collections::HashMap;

#[test]
fn cc_start_rejects_a_status_cell_whose_length_is_not_one() {
    for length in [0, 2] {
        let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
        let (start, _) = binding(&input.cc.representations, R::Start);
        let function = input
            .cc
            .functions
            .iter_mut()
            .find(|function| function.symbol == start)
            .expect("Start helper");
        let mut cells = 0;
        for assignment in &mut function.assignments {
            let A::ArrayNew { elements, .. } = &mut assignment.kind else {
                continue;
            };
            cells += 1;
            if length == 0 {
                elements.clear();
            } else {
                let element = elements[0];
                elements.push(element);
            }
        }
        assert_eq!(cells, 1, "Start allocates one status cell");
        let errors = psrs_backend::cc::verify_module(&input.cc)
            .expect_err("a status cell must contain exactly one live element");
        assert!(
            errors.iter().any(|error| error
                .message
                .contains("emitted machine operands or ordered operations")),
            "length {length}: {errors:?}"
        );
    }
}

#[test]
fn pre_optimization_run_accepts_exclusive_sharing_distributed_close_and_copies() {
    let (start, close, executor, mut module) = run_module();
    psrs_backend::mir::verify_module(&module).expect("unmodified Run lowers");
    let baseline = calls(&module, start);
    assert_eq!(baseline, 1);
    assert_eq!(calls(&module, close), 1);

    share_root(&mut module, start);
    psrs_backend::mir::verify_module(&module).unwrap_or_else(|errors| {
        panic!("exclusive successors may share one fresh root: {errors:?}")
    });
    assert_eq!(
        calls(&module, start),
        baseline,
        "Start stays one allocation"
    );
    assert_eq!(calls(&module, executor), baseline * 2);
    assert_eq!(calls(&module, close), baseline * 2);

    let (_, _, _, mut module) = run_module();
    distribute_close(&mut module, close, false);
    psrs_backend::mir::verify_module(&module)
        .unwrap_or_else(|errors| panic!("Close may be repeated on normal branches: {errors:?}"));
    assert_eq!(calls(&module, close), baseline * 2);

    let (_, _, _, mut module) = run_module();
    distribute_close(&mut module, close, true);
    psrs_backend::mir::verify_module(&module).unwrap_or_else(|errors| {
        panic!("a trapping alternative is outside the normal close proof: {errors:?}")
    });
    assert_eq!(calls(&module, close), baseline);
    assert!(module.functions.iter().any(|function| {
        function
            .blocks
            .iter()
            .any(|block| matches!(block.terminator, Some(T::Trap { .. })))
    }));

    let (_, _, _, mut module) = run_module();
    copy_root(&mut module, close);
    psrs_backend::mir::verify_module(&module)
        .unwrap_or_else(|errors| panic!("Close may use a copy of its root reference: {errors:?}"));
}

fn run_module() -> (SymbolId, SymbolId, SymbolId, Module) {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (start, _) = binding(&input.cc.representations, R::Start);
    let (close, _) = binding(&input.cc.representations, R::Close);
    let (executor, _) = binding(&input.cc.representations, R::Executor);
    let (run, _) = binding(&input.cc.representations, R::Run);
    let (module, _) = psrs_backend::mir::lower_module(input.cc).unwrap();
    let function = module
        .functions
        .iter()
        .find(|function| function.symbol == run)
        .expect("Run helper");
    assert_eq!(
        function.blocks.len(),
        1,
        "pre-optimization Run is the straight-line plan"
    );
    (start, close, executor, module)
}

fn run_function(module: &mut Module) -> &mut psrs_backend::mir::Function {
    module
        .functions
        .iter_mut()
        .find(|function| function.name == "control_run")
        .expect("Run helper name")
}

fn calls(module: &Module, target: SymbolId) -> usize {
    module
        .functions
        .iter()
        .filter(|function| function.name == "control_run")
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(
            |instruction| matches!(instruction, I::Call { function, .. } if *function == target),
        )
        .count()
}

fn share_root(module: &mut Module, start: SymbolId) {
    let function = run_function(module);
    let span = function.span;
    let start_at = function.blocks[0]
        .instructions
        .iter()
        .position(
            |instruction| matches!(instruction, I::Call { function, .. } if *function == start),
        )
        .expect("Start call");
    let tail = function.blocks[0].instructions.split_off(start_at + 1);
    let result = match function.blocks[0].terminator.take() {
        Some(T::Return { value, .. }) => value,
        other => panic!("Run returns normally, got {other:?}"),
    };
    assert!(
        tail.iter()
            .any(|instruction| instruction.destination() == Some(result)),
        "the Run result is produced after Start"
    );
    let (else_tail, copied) = duplicate(function, &tail);
    let else_result = copied[&result];
    let then_block = BlockId(
        function
            .blocks
            .iter()
            .map(|block| block.id.0)
            .max()
            .unwrap()
            + 1,
    );
    let else_block = BlockId(then_block.0 + 1);
    let condition = fresh_value(function, ValueType::Boolean);
    function.blocks[0].instructions.push(I::Constant {
        destination: condition,
        value: 1,
        span,
    });
    function.blocks[0].terminator = Some(T::Branch {
        condition,
        then_block,
        else_block,
        span,
    });
    function.blocks.extend([
        block_returning(then_block, tail, result, span),
        block_returning(else_block, else_tail, else_result, span),
    ]);
}

fn distribute_close(module: &mut Module, close: SymbolId, trap: bool) {
    let function = run_function(module);
    let span = function.span;
    let close_at = function.blocks[0]
        .instructions
        .iter()
        .position(
            |instruction| matches!(instruction, I::Call { function, .. } if *function == close),
        )
        .expect("Close call");
    let close_instruction = function.blocks[0].instructions.remove(close_at);
    let suffix = function.blocks[0].instructions.split_off(close_at);
    let terminator = function.blocks[0].terminator.take();
    let destination = match &close_instruction {
        I::Call { destination, .. } => *destination,
        _ => unreachable!("Close is a call"),
    };
    let result_ty = function
        .values
        .iter()
        .find(|value| value.id == destination)
        .unwrap()
        .ty;
    let alias = fresh_value(function, result_ty);
    let mut other = close_instruction.clone();
    let I::Call { destination, .. } = &mut other else {
        unreachable!("Close is a call");
    };
    *destination = alias;
    let then_block = BlockId(
        function
            .blocks
            .iter()
            .map(|block| block.id.0)
            .max()
            .unwrap()
            + 1,
    );
    let else_block = BlockId(then_block.0 + 1);
    let join = BlockId(then_block.0 + 2);
    let condition = fresh_value(function, ValueType::Boolean);
    function.blocks[0].instructions.push(I::Constant {
        destination: condition,
        value: 1,
        span,
    });
    function.blocks[0].terminator = Some(T::Branch {
        condition,
        then_block,
        else_block,
        span,
    });
    let jump = Some(T::Jump {
        target: join,
        arguments: vec![],
        span,
    });
    function.blocks.extend([
        BasicBlock {
            id: then_block,
            parameters: vec![],
            instructions: vec![close_instruction],
            terminator: jump.clone(),
        },
        BasicBlock {
            id: else_block,
            parameters: vec![],
            instructions: if trap { vec![] } else { vec![other] },
            terminator: if trap { Some(T::Trap { span }) } else { jump },
        },
        BasicBlock {
            id: join,
            parameters: vec![],
            instructions: suffix,
            terminator,
        },
    ]);
}

fn copy_root(module: &mut Module, close: SymbolId) {
    let function = run_function(module);
    let span = function.span;
    let close_at = function.blocks[0]
        .instructions
        .iter()
        .position(
            |instruction| matches!(instruction, I::Call { function, .. } if *function == close),
        )
        .expect("Close call");
    let root = match &function.blocks[0].instructions[close_at] {
        I::Call { arguments, .. } => arguments[0],
        _ => unreachable!("Close is a call"),
    };
    let ty = function
        .values
        .iter()
        .find(|value| value.id == root)
        .unwrap()
        .ty;
    let alias = fresh_value(function, ty);
    match &mut function.blocks[0].instructions[close_at] {
        I::Call { arguments, .. } => arguments[0] = alias,
        _ => unreachable!("Close is a call"),
    }
    function.blocks[0].instructions.insert(
        close_at,
        I::Copy {
            destination: alias,
            value: root,
            span,
        },
    );
}

fn duplicate(
    function: &mut psrs_backend::mir::Function,
    tail: &[I],
) -> (Vec<I>, HashMap<ValueId, ValueId>) {
    let mut map = HashMap::new();
    for instruction in tail {
        if let Some(destination) = instruction.destination() {
            let ty = function
                .values
                .iter()
                .find(|value| value.id == destination)
                .expect("defined Run value")
                .ty;
            map.insert(destination, fresh_value(function, ty));
        }
    }
    let copied = tail
        .iter()
        .cloned()
        .map(|instruction| retarget(instruction, &map))
        .collect();
    (copied, map)
}

fn retarget(instruction: I, map: &HashMap<ValueId, ValueId>) -> I {
    let id = |value: ValueId| map.get(&value).copied().unwrap_or(value);
    match instruction {
        I::Call {
            contract,
            destination,
            function,
            arguments,
            span,
        } => I::Call {
            contract,
            destination: id(destination),
            function,
            arguments: arguments.into_iter().map(id).collect(),
            span,
        },
        I::StructNew {
            destination,
            type_index,
            arguments,
            span,
        } => I::StructNew {
            destination: id(destination),
            type_index,
            arguments: arguments.into_iter().map(id).collect(),
            span,
        },
        I::Constant {
            destination,
            value,
            span,
        } => I::Constant {
            destination: id(destination),
            value,
            span,
        },
        I::RefCast {
            destination,
            value,
            reference,
            span,
        } => I::RefCast {
            destination: id(destination),
            value: id(value),
            reference,
            span,
        },
        I::Copy {
            destination,
            value,
            span,
        } => I::Copy {
            destination: id(destination),
            value: id(value),
            span,
        },
        other => panic!("Run lowering produced an unexpected instruction: {other:?}"),
    }
}

fn block_returning(
    id: BlockId,
    instructions: Vec<I>,
    value: ValueId,
    span: psrs_span::TextRange,
) -> BasicBlock {
    BasicBlock {
        id,
        parameters: vec![],
        instructions,
        terminator: Some(T::Return { value, span }),
    }
}

fn fresh_value(function: &mut psrs_backend::mir::Function, ty: ValueType) -> ValueId {
    let id = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    function.values.push(ValueDecl { id, ty });
    id
}
