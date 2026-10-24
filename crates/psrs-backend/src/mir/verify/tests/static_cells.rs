use super::*;
use crate::types::{DefinedTypeId, FieldType, HeapType, RefType, StorageType};
fn fixture() -> Module {
    let erased = RefType {
        nullable: true,
        heap: HeapType::Eq,
    };
    let array = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(DefinedTypeId(0)),
    });
    let mut module = module_with_function(
        Function {
            id: crate::types::FunctionId(0),
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "static".into(),
            parameters: Vec::new(),
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: ValueType::Ref(erased),
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: array,
                },
            ],
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::RefNull {
                        destination: ValueId(0),
                        heap: HeapType::Eq,
                        span: span(),
                    },
                    Instruction::StaticCell {
                        destination: ValueId(1),
                        binding: SymbolId::new(ModuleId(1), 0),
                        type_index: DefinedTypeId(0),
                        initial: ValueId(0),
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: ValueId(1),
                    span: span(),
                }),
            }],
            result: ValueId(1),
            result_type: array,
            frame_weight: 0,
            span: span(),
        },
        Vec::new(),
    );
    module.types.push(RecGroup(vec![DefinedType {
        composite: CompositeType::Array(FieldType {
            storage: StorageType::Ref(erased),
            mutable: true,
        }),
        final_type: true,
        supertype: None,
    }]));
    module
}
#[test]
fn static_storage_rejects_incompatible_physical_shapes() {
    let original = fixture();
    verify_module(&original).unwrap();
    for change in 0..3 {
        let mut module = original.clone();
        match change {
            0 => {
                let CompositeType::Array(field) = &mut module.types[0].0[0].composite else {
                    unreachable!()
                };
                field.mutable = false;
            }
            1 => {
                let CompositeType::Array(field) = &mut module.types[0].0[0].composite else {
                    unreachable!()
                };
                field.storage = StorageType::I32;
            }
            _ => module.functions[0].values[1].ty = ValueType::I32,
        }
        assert!(verify_module(&module).is_err(), "change {change}");
    }
}
#[test]
fn static_storage_requires_mutable_globals() {
    let target = crate::TargetCapabilities {
        mutable_globals: false,
        ..Default::default()
    };
    let errors = crate::mir::verify_module_with_capabilities(&fixture(), target).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("mutable globals")),
        "{errors:?}"
    );
}
#[test]
fn static_binding_cannot_change_its_layout_between_uses() {
    let mut module = fixture();
    module.types.push(module.types[0].clone());
    let mut other = module.functions[0].blocks[0].instructions[1].clone();
    let Instruction::StaticCell { type_index, .. } = &mut other else {
        unreachable!()
    };
    *type_index = DefinedTypeId(1);
    module.functions[0].blocks[0].instructions.push(other);
    let errors = verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("inconsistent array layouts")),
        "{errors:?}"
    );
}
