//! MIR-verifier fixtures for the `Effect` closure and hidden-token boundary.
//!
//! An effect closure takes its hidden token as its first user argument. These
//! fixtures reject call sites with the wrong token arity or result type.

use super::{module_with_function, span};
use crate::mir::{BasicBlock, BlockId, Function, Instruction, Terminator};
use crate::types::{
    CompositeType, DefinedType, DefinedTypeId, FieldType, FunctionId, HeapType, RecGroup, RefType,
    StorageType, ValueDecl, ValueId, ValueType,
};
use psrs_hir::{ModuleId, SymbolId};

fn value(id: u32, ty: ValueType) -> ValueDecl {
    ValueDecl {
        id: ValueId(id),
        ty,
    }
}

fn reference(heap: HeapType) -> ValueType {
    ValueType::Ref(RefType {
        nullable: false,
        heap,
    })
}

fn closure_value_type() -> ValueType {
    reference(HeapType::Struct)
}

fn final_type(composite: CompositeType) -> DefinedType {
    DefinedType {
        final_type: true,
        supertype: None,
        composite,
    }
}

fn closure_types() -> Vec<RecGroup> {
    let function_type = final_type(CompositeType::Func {
        parameters: vec![closure_value_type(), ValueType::I32],
        results: vec![ValueType::I32],
    });
    let closure_type = final_type(CompositeType::Struct(vec![
        FieldType {
            storage: StorageType::Ref(RefType {
                nullable: false,
                heap: HeapType::Func,
            }),
            mutable: false,
        },
        FieldType {
            storage: StorageType::Ref(RefType {
                nullable: false,
                heap: HeapType::Index(DefinedTypeId(2)),
            }),
            mutable: false,
        },
    ]));
    let capture_array = final_type(CompositeType::Array(FieldType {
        storage: StorageType::Ref(RefType {
            nullable: true,
            heap: HeapType::Eq,
        }),
        mutable: false,
    }));
    vec![RecGroup(vec![function_type, closure_type, capture_array])]
}

/// Builds a caller of the effect-shaped closure type `[closure, token] -> result`
/// with the supplied call operands and destination type.
fn effect_closure_call(
    arguments: Vec<ValueId>,
    destination_type: ValueType,
    values: Vec<ValueDecl>,
) -> Function {
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "effect_closure_call".into(),
        parameters: values
            .iter()
            .filter(|value| value.id.0 < 2)
            .map(|value| value.id)
            .collect(),
        values,
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::ClosureCall {
                destination: ValueId(2),
                function: ValueId(0),
                type_index: DefinedTypeId(0),
                closure_type: DefinedTypeId(1),
                capture_array_type: DefinedTypeId(2),
                arguments,
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: ValueId(2),
                span: span(),
            }),
        }],
        result: ValueId(2),
        result_type: destination_type,
        span: span(),
    }
}

#[test]
fn rejects_an_effect_closure_call_with_the_wrong_token_arity() {
    let function = effect_closure_call(
        Vec::new(),
        ValueType::I32,
        vec![
            value(0, closure_value_type()),
            value(1, ValueType::I32),
            value(2, ValueType::I32),
        ],
    );
    let errors =
        crate::mir::verify_module(&module_with_function(function, closure_types())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("closure.call operands")),
        "{errors:?}"
    );
}

#[test]
fn rejects_an_effect_closure_call_with_the_wrong_result_type() {
    let function = effect_closure_call(
        vec![ValueId(1)],
        ValueType::F64,
        vec![
            value(0, closure_value_type()),
            value(1, ValueType::I32),
            value(2, ValueType::F64),
        ],
    );
    let errors =
        crate::mir::verify_module(&module_with_function(function, closure_types())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("closure.call operands")),
        "{errors:?}"
    );
}
