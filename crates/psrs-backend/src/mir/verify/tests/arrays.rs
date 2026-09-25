use super::*;

fn array_conversion_function(
    element_storage: crate::types::StorageType,
    length_type: ValueType,
    source_type: ValueType,
) -> (Function, Vec<RecGroup>) {
    use crate::types::{DefinedTypeId, FieldType, HeapType, RefType};

    let array_reference = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(DefinedTypeId(0)),
    });
    let function = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "array_default".into(),
        parameters: vec![ValueId(0)],
        values: vec![
            ValueDecl {
                id: ValueId(0),
                ty: source_type,
            },
            ValueDecl {
                id: ValueId(1),
                ty: length_type,
            },
            ValueDecl {
                id: ValueId(2),
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(3),
                ty: array_reference,
            },
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                if length_type == ValueType::F64 {
                    Instruction::NumberConstant {
                        destination: ValueId(1),
                        value: "1.0".into(),
                        span: span(),
                    }
                } else {
                    Instruction::Constant {
                        destination: ValueId(1),
                        value: 1,
                        span: span(),
                    }
                },
                Instruction::Constant {
                    destination: ValueId(2),
                    value: 0,
                    span: span(),
                },
                Instruction::ArrayNewDefault {
                    destination: ValueId(3),
                    type_index: DefinedTypeId(0),
                    length: ValueId(1),
                    source: ValueId(0),
                    header: BlockId(1),
                    body: BlockId(2),
                    exit: BlockId(3),
                    index: ValueId(2),
                    span: span(),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(3),
                span: span(),
            }),
        }],
        result: ValueId(3),
        result_type: array_reference,
        span: span(),
    };
    let types = vec![RecGroup(vec![DefinedType {
        final_type: true,
        supertype: None,
        composite: CompositeType::Array(FieldType {
            storage: element_storage,
            mutable: true,
        }),
    }])];
    (function, types)
}

#[test]
fn rejects_array_new_default_with_non_defaultable_element_storage() {
    use crate::types::{HeapType, RefType, StorageType};

    let (function, types) = array_conversion_function(
        StorageType::Ref(RefType {
            nullable: false,
            heap: HeapType::Eq,
        }),
        ValueType::I32,
        ValueType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(crate::types::DefinedTypeId(0)),
        }),
    );
    let errors = verify_module(&module_with_function(function, types)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("not defaultable")),
        "{errors:?}"
    );
}

#[test]
fn rejects_nullable_array_load_declared_as_nonnullable() {
    use crate::types::{DefinedTypeId, HeapType, RefType, StorageType};
    let array = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(DefinedTypeId(0)),
    });
    let nonnull = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Eq,
    });
    let (mut function, types) = array_conversion_function(
        StorageType::Ref(RefType {
            nullable: true,
            heap: HeapType::Eq,
        }),
        ValueType::I32,
        array,
    );
    function.values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: array,
        },
        ValueDecl {
            id: ValueId(1),
            ty: ValueType::I32,
        },
        ValueDecl {
            id: ValueId(2),
            ty: nonnull,
        },
    ];
    function.blocks[0].instructions = vec![
        Instruction::Constant {
            destination: ValueId(1),
            value: 0,
            span: span(),
        },
        Instruction::ArrayGet {
            destination: ValueId(2),
            type_index: DefinedTypeId(0),
            value: ValueId(0),
            index: ValueId(1),
            span: span(),
        },
    ];
    function.blocks[0].terminator = Some(Terminator::Return {
        value: ValueId(2),
        span: span(),
    });
    function.result = ValueId(2);
    function.result_type = nonnull;
    let errors = verify_module(&module_with_function(function, types)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("array.get result"))
    );
}

#[test]
fn rejects_array_new_default_with_a_non_i32_length() {
    use crate::types::{HeapType, RefType, StorageType};

    let (function, types) = array_conversion_function(
        StorageType::I32,
        ValueType::F64,
        ValueType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(crate::types::DefinedTypeId(0)),
        }),
    );
    let errors = verify_module(&module_with_function(function, types)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("length must be i32")),
        "{errors:?}"
    );
}

#[test]
fn rejects_array_new_default_with_a_non_array_source() {
    use crate::types::StorageType;

    let (function, types) =
        array_conversion_function(StorageType::I32, ValueType::I32, ValueType::I32);
    let errors = verify_module(&module_with_function(function, types)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("source must be an array")),
        "{errors:?}"
    );
}
