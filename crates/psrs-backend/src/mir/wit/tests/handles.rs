//! Resource-handle lowering: `resource.drop` for `own<T>`, borrow release at
//! the end of the call, and the scope verifier.

use super::common::{RecordingLowerer, source_signature};
use super::handles::verify_function;
use super::*;
use crate::abi::{
    BoundWasiImport, HandleMode, HandleResource, SourceSignature, SourceType, WasiImport,
    WasiParamKind, WasiResultKind,
};
use crate::mir::{BasicBlock, BlockId, Function, Instruction, Terminator};
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use std::collections::HashMap;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn handle(mode: HandleMode, symbol: SymbolId) -> HandleResource {
    HandleResource {
        mode,
        interface: "fixture:handles/types@0.1.0".into(),
        name: "thing".into(),
        drop_symbol: symbol,
    }
}

fn import(result: WasiResultKind, params: Vec<WasiParamKind>, symbol: SymbolId) -> WasiImport {
    WasiImport {
        symbol,
        module: "fixture:handles/types@0.1.0".into(),
        name: "take".into(),
        parameters: vec![ValueType::I32; params.len()],
        param_kinds: params,
        result: Some(ValueType::I32),
        result_kind: result,
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    }
}

fn bound(import: WasiImport) -> BoundWasiImport {
    BoundWasiImport {
        signature: SourceSignature {
            parameters: vec![SourceType::Int; import.param_kinds.len()],
            result: SourceType::Int,
            span: span(),
        },
        import,
    }
}

#[test]
fn a_borrow_result_is_released_when_the_call_returns() {
    let drop_symbol = SymbolId::new(ModuleId::INTRINSICS, 9);
    let symbol = SymbolId::new(ModuleId::INTRINSICS, 3);
    let import = import(
        WasiResultKind::Handle(handle(HandleMode::Borrow, drop_symbol)),
        Vec::new(),
        symbol,
    );
    let mut lowerer = RecordingLowerer::default();
    let destination = ValueId(4);
    lower(
        &mut lowerer,
        &import,
        &source_signature(Vec::new(), SourceType::Int),
        destination,
        &[],
        span(),
        BlockId(0),
    )
    .expect("a borrow result should lower");
    assert!(
        matches!(
            lowerer.instructions.as_slice(),
            [
                Instruction::Call {
                    destination: actual,
                    function,
                    ..
                },
                Instruction::CallVoid {
                    function: drop,
                    arguments,
                    ..
                },
            ] if *actual == destination
                && *function == symbol
                && *drop == drop_symbol
                && arguments == &[destination]
        ),
        "the borrow must be released by resource.drop when the call returns: {:?}",
        lowerer.instructions
    );
}

#[test]
fn an_owned_result_is_not_dropped_before_the_caller_can_use_it() {
    let drop_symbol = SymbolId::new(ModuleId::INTRINSICS, 9);
    let symbol = SymbolId::new(ModuleId::INTRINSICS, 3);
    let import = import(
        WasiResultKind::Handle(handle(HandleMode::Own, drop_symbol)),
        Vec::new(),
        symbol,
    );
    let mut lowerer = RecordingLowerer::default();
    lower(
        &mut lowerer,
        &import,
        &source_signature(Vec::new(), SourceType::Int),
        ValueId(4),
        &[],
        span(),
        BlockId(0),
    )
    .expect("an owned result should lower");
    assert!(
        lowerer
            .instructions
            .iter()
            .all(|instruction| !matches!(instruction, Instruction::CallVoid { .. })),
        "own<T> is dropped when the value is consumed, not at the call: {:?}",
        lowerer.instructions
    );
}

fn function_calling(instructions: Vec<Instruction>) -> Function {
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "use-handle".into(),
        parameters: Vec::new(),
        values: vec![ValueDecl {
            id: ValueId(1),
            ty: ValueType::I32,
        }],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions,
            terminator: Some(Terminator::Return {
                value: ValueId(1),
                span: span(),
            }),
        }],
        result: ValueId(1),
        result_type: ValueType::I32,
        span: span(),
    }
}

fn imports_for(import: WasiImport) -> HashMap<SymbolId, BoundWasiImport> {
    let mut imports = HashMap::new();
    imports.insert(import.symbol, bound(import));
    imports
}

#[test]
fn the_verifier_rejects_an_owned_handle_dropped_twice() {
    let drop_symbol = SymbolId::new(ModuleId::INTRINSICS, 9);
    let symbol = SymbolId::new(ModuleId::INTRINSICS, 3);
    let import = import(
        WasiResultKind::Handle(handle(HandleMode::Own, drop_symbol)),
        Vec::new(),
        symbol,
    );
    let instructions = vec![
        Instruction::Call {
            destination: ValueId(1),
            function: symbol,
            arguments: Vec::new(),
            span: span(),
        },
        Instruction::CallVoid {
            function: drop_symbol,
            arguments: vec![ValueId(1)],
            span: span(),
        },
        Instruction::CallVoid {
            function: drop_symbol,
            arguments: vec![ValueId(1)],
            span: span(),
        },
    ];
    let errors = verify_function(&function_calling(instructions), &imports_for(import))
        .expect_err("a second resource.drop of an owned handle must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("owned handle dropped twice")),
        "{errors:?}"
    );
}

#[test]
fn the_verifier_rejects_a_handle_used_after_its_borrow_scope() {
    let drop_symbol = SymbolId::new(ModuleId::INTRINSICS, 9);
    let symbol = SymbolId::new(ModuleId::INTRINSICS, 3);
    let import = import(
        WasiResultKind::Handle(handle(HandleMode::Borrow, drop_symbol)),
        Vec::new(),
        symbol,
    );
    let instructions = vec![
        Instruction::Call {
            destination: ValueId(1),
            function: symbol,
            arguments: Vec::new(),
            span: span(),
        },
        Instruction::CallVoid {
            function: drop_symbol,
            arguments: vec![ValueId(1)],
            span: span(),
        },
        Instruction::Copy {
            destination: ValueId(1),
            value: ValueId(1),
            span: span(),
        },
    ];
    let errors = verify_function(&function_calling(instructions), &imports_for(import))
        .expect_err("a borrow must not be used after resource.drop releases it");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("after its borrow scope")),
        "{errors:?}"
    );
}
