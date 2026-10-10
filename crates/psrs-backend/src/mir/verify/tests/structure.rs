//! MIR structural negative fixtures: duplicate identifiers, a missing
//! terminator, and a value defined twice are rejected before encoding.

use super::*;

fn simple_function(values: Vec<ValueDecl>, blocks: Vec<BasicBlock>) -> Function {
    Function {
        state: None,
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "structure".into(),
        parameters: Vec::new(),
        values,
        entry: BlockId(0),
        blocks,
        result: ValueId(0),
        result_type: ValueType::I32,
        span: span(),
    }
}

fn return_block(id: u32, value: ValueId) -> BasicBlock {
    BasicBlock {
        id: BlockId(id),
        parameters: Vec::new(),
        instructions: Vec::new(),
        terminator: Some(Terminator::Return {
            value,
            span: span(),
        }),
    }
}

#[test]
fn rejects_duplicate_value_ids() {
    let function = simple_function(
        vec![
            ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            },
        ],
        vec![return_block(0, ValueId(0))],
    );
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("value IDs are duplicated")),
        "{errors:?}"
    );
}

#[test]
fn rejects_duplicate_block_ids() {
    let function = simple_function(
        vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::I32,
        }],
        vec![return_block(0, ValueId(0)), return_block(0, ValueId(0))],
    );
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("block IDs are duplicated")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_block_without_a_terminator() {
    let function = simple_function(
        vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::I32,
        }],
        vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: None,
        }],
    );
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("has no terminator")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_value_defined_by_two_instructions() {
    let function = simple_function(
        vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::I32,
        }],
        vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 1,
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 2,
                    span: span(),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(0),
                span: span(),
            }),
        }],
    );
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("defined more than once")),
        "{errors:?}"
    );
}

#[test]
fn recursive_references_are_scoped_to_their_declared_group() {
    use crate::types::{
        CompositeType, DefinedType, DefinedTypeId, FieldType, HeapType, RecGroup, RefType,
        StorageType,
    };
    let definition = |target| DefinedType {
        final_type: true,
        supertype: None,
        composite: CompositeType::Struct(vec![FieldType {
            storage: StorageType::Ref(RefType {
                nullable: true,
                heap: HeapType::Index(DefinedTypeId(target)),
            }),
            mutable: false,
        }]),
    };
    let mut source = Module {
        name: "recursive_group_scope".into(),
        types: vec![RecGroup(vec![definition(1), definition(0)])],
        strings: Vec::new(),
        dependencies: Default::default(),
        layout: None,
        imports: Vec::new(),
        functions: Vec::new(),
        entry: None,
        span: span(),
    };
    verify_module(&source).unwrap();
    source.types = vec![RecGroup(vec![definition(1)]), RecGroup(vec![definition(0)])];
    assert!(verify_module(&source).unwrap_err().iter().any(|error| {
        error
            .message
            .contains("defined type reference is out of range")
    }));
    source.types = vec![RecGroup(Vec::new())];
    assert!(
        verify_module(&source)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("recursion group is empty"))
    );
}
