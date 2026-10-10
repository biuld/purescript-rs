//! MIR verification of a hand-built module.
use super::span;
use crate::mir::{BasicBlock, BlockId, Function, Instruction, Module, Terminator};
use crate::types::{
    CompositeType, DefinedType, FieldType, HeapType, RecGroup, RefType, StorageType, ValueDecl,
    ValueId, ValueType,
};
use psrs_hir::{ModuleId, SymbolId};

/// The MIR verifier checks struct field types, not just the type index range.
#[test]
fn rejects_a_struct_new_with_a_mistyped_field() {
    let mir = Module {
        name: "BadStruct".into(),
        entry: None,
        types: vec![RecGroup(vec![DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Struct(vec![FieldType {
                storage: StorageType::I64,
                mutable: false,
            }]),
        }])],
        strings: Vec::new(),
        dependencies: Default::default(),
        layout: None,
        imports: Vec::new(),
        functions: vec![Function {
            state: None,
            id: crate::types::FunctionId(0),
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: ValueType::I64,
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(2),
                    ty: ValueType::Ref(RefType {
                        nullable: false,
                        heap: HeapType::Index(crate::types::DefinedTypeId(0)),
                    }),
                },
            ],
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::Constant {
                        destination: ValueId(1),
                        value: 1,
                        span: span(),
                    },
                    Instruction::StructNew {
                        destination: ValueId(2),
                        type_index: crate::types::DefinedTypeId(0),
                        arguments: vec![ValueId(1)],
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: ValueId(0),
                    span: span(),
                }),
            }],
            result: ValueId(0),
            result_type: ValueType::I64,
            span: span(),
        }],
        span: span(),
    };

    let errors = crate::mir::verify_module(&mir).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("struct.new argument has the wrong type")),
        "{errors:?}"
    );
}
