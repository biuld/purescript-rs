//! Resource-handle lowering under DEC-14: the compiler never drops or releases
//! a handle on its own, and the verifier only rejects a double drop.

use super::common::{RecordingLowerer, signature};
use super::handles::verify_function;
use super::*;
use crate::abi::canonical::{CanonicalType, Ownership, ResourceId};
use crate::cc::{Signature, ValueShape};
use crate::mir::BoundWasiImport;
use crate::mir::{BasicBlock, BlockId, Function, Instruction, Terminator};
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use std::collections::HashMap;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn handle(ownership: Ownership) -> CanonicalType {
    CanonicalType::Handle {
        resource: ResourceId {
            interface: "fixture:handles/types@0.1.0".into(),
            name: "thing".into(),
        },
        ownership,
    }
}

fn import(
    result: Option<CanonicalType>,
    params: Vec<CanonicalType>,
    symbol: SymbolId,
) -> WasiImport {
    crate::abi::test_support::import(
        symbol,
        "fixture:handles/types@0.1.0",
        "take",
        params,
        result,
    )
}

fn bound(import: WasiImport) -> BoundWasiImport {
    BoundWasiImport {
        signature: Signature {
            parameters: vec![ValueShape::Integer; import.params.len()],
            result: ValueShape::Integer,
        },
        projection: None,
        import,
    }
}

#[test]
fn a_result_handle_is_not_released_by_the_compiler() {
    let drop_symbol = SymbolId::new(ModuleId::INTRINSICS, 9);
    let symbol = SymbolId::new(ModuleId::INTRINSICS, 3);
    let import = import(Some(handle(Ownership::Borrow)), Vec::new(), symbol);
    let mut lowerer = RecordingLowerer::default();
    let destination = ValueId(4);
    lower(
        &mut lowerer,
        &import,
        &signature(Vec::new()),
        None,
        destination,
        &[],
        span(),
        BlockId(0),
    )
    .expect("a handle result should lower");
    assert!(
        matches!(
            lowerer.instructions.as_slice(),
            [Instruction::Call {
                destination: actual,
                function,
                ..
            }] if *actual == destination && *function == symbol
        ),
        "the compiler must not drop or release a result handle: {:?}",
        lowerer.instructions
    );
    let _ = drop_symbol;
}

#[test]
fn an_owned_result_is_not_dropped_by_the_compiler() {
    let drop_symbol = SymbolId::new(ModuleId::INTRINSICS, 9);
    let symbol = SymbolId::new(ModuleId::INTRINSICS, 3);
    let import = import(
        Some(handle(Ownership::Own { drop: drop_symbol })),
        Vec::new(),
        symbol,
    );
    let mut lowerer = RecordingLowerer::default();
    lower(
        &mut lowerer,
        &import,
        &signature(Vec::new()),
        None,
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
        state: None,
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
        Some(handle(Ownership::Own { drop: drop_symbol })),
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
        errors.iter().any(|error| error
            .message
            .contains("drops or transfers an owned handle twice")),
        "{errors:?}"
    );
}

#[test]
fn the_verifier_accepts_an_explicit_drop() {
    let drop_symbol = SymbolId::new(ModuleId::INTRINSICS, 9);
    let symbol = SymbolId::new(ModuleId::INTRINSICS, 3);
    let import = import(
        Some(handle(Ownership::Own { drop: drop_symbol })),
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
    ];
    verify_function(&function_calling(instructions), &imports_for(import))
        .expect("one explicit resource.drop of an owned handle is accepted");
}
