use super::verify_array_maps;
use crate::mir::{BasicBlock, BlockId, Function, Instruction, NumericOp, Terminator};
use crate::types::{DefinedTypeId, FunctionId, HeapType, RefType, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn verified_array_map_module(mut function: Function) -> crate::mir::Module {
    use crate::types::{CompositeType, DefinedType, FieldType, RecGroup, StorageType};
    function.parameters.push(ValueId(8));
    crate::mir::Module {
        name: "ArrayMapVerifier".into(),
        types: vec![
            RecGroup(vec![DefinedType {
                final_type: true,
                supertype: None,
                composite: CompositeType::Array(FieldType {
                    storage: StorageType::I32,
                    mutable: true,
                }),
            }]),
            RecGroup(vec![DefinedType {
                final_type: true,
                supertype: None,
                composite: CompositeType::Array(FieldType {
                    storage: StorageType::I32,
                    mutable: true,
                }),
            }]),
        ],
        strings: Vec::new(),
        imports: Vec::new(),
        entry: Some(function.symbol),
        functions: vec![function],
        span: span(),
    }
}

const fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn array_map_function() -> Function {
    let source = ValueId(0);
    let destination = ValueId(1);
    let length = ValueId(2);
    let index = ValueId(3);
    let condition = ValueId(4);
    let zero = ValueId(5);
    let one = ValueId(6);
    let incremented = ValueId(7);
    let element = ValueId(8);
    let array_ref = |type_index| {
        ValueType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(DefinedTypeId(type_index)),
        })
    };

    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "array_map_verifier_test".into(),
        parameters: vec![source],
        values: vec![
            ValueDecl {
                id: source,
                ty: array_ref(0),
            },
            ValueDecl {
                id: destination,
                ty: array_ref(1),
            },
            ValueDecl {
                id: length,
                ty: ValueType::I32,
            },
            ValueDecl {
                id: index,
                ty: ValueType::I32,
            },
            ValueDecl {
                id: condition,
                ty: ValueType::Boolean,
            },
            ValueDecl {
                id: zero,
                ty: ValueType::I32,
            },
            ValueDecl {
                id: one,
                ty: ValueType::I32,
            },
            ValueDecl {
                id: incremented,
                ty: ValueType::I32,
            },
            ValueDecl {
                id: element,
                ty: ValueType::I32,
            },
        ],
        entry: BlockId(0),
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::ArrayLen {
                        destination: length,
                        value: source,
                        span: span(),
                    },
                    Instruction::ArrayNewDefault {
                        destination,
                        type_index: DefinedTypeId(1),
                        length,
                        source,
                        header: BlockId(1),
                        body: BlockId(2),
                        exit: BlockId(3),
                        index,
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: zero,
                        value: 0,
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Jump {
                    target: BlockId(1),
                    arguments: vec![zero],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: vec![index],
                instructions: vec![Instruction::Primitive {
                    destination: condition,
                    op: NumericOp::I32LtS,
                    left: index,
                    right: length,
                    span: span(),
                }],
                terminator: Some(Terminator::Branch {
                    condition,
                    then_block: BlockId(2),
                    else_block: BlockId(3),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::ArraySet {
                        type_index: DefinedTypeId(1),
                        value: destination,
                        index,
                        new_value: element,
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: one,
                        value: 1,
                        span: span(),
                    },
                    Instruction::Primitive {
                        destination: incremented,
                        op: NumericOp::I32Add,
                        left: index,
                        right: one,
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Jump {
                    target: BlockId(1),
                    arguments: vec![incremented],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(3),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: destination,
                    span: span(),
                }),
            },
        ],
        result: destination,
        result_type: array_ref(1),
        span: span(),
    }
}

#[test]
fn accepts_a_complete_array_map_loop() {
    assert!(verify_array_maps(&array_map_function()).is_ok());
}

#[test]
fn rejects_an_exit_after_storing_only_the_current_element() {
    let mut function = array_map_function();
    let increment = function.blocks[2].instructions.split_off(1);
    let backedge = function.blocks[2].terminator.take();
    function.blocks[2].terminator = Some(Terminator::Branch {
        condition: ValueId(4),
        then_block: BlockId(3),
        else_block: BlockId(4),
        span: span(),
    });
    function.blocks.push(BasicBlock {
        id: BlockId(4),
        parameters: Vec::new(),
        instructions: increment,
        terminator: backedge,
    });
    let errors = verify_array_maps(&function).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("without initializing")),
        "{errors:?}"
    );
    let errors = crate::mir::verify_module(&verified_array_map_module(function)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("without initializing")),
        "{errors:?}"
    );
}

#[test]
fn rejects_reading_the_destination_before_its_loop() {
    let mut function = array_map_function();
    function.blocks[0].instructions.push(Instruction::ArrayGet {
        destination: ValueId(9),
        type_index: DefinedTypeId(1),
        value: ValueId(1),
        index: ValueId(5),
        span: span(),
    });
    let errors = verify_array_maps(&function).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("escapes before initialization")),
        "{errors:?}"
    );
    function.values.push(crate::types::ValueDecl {
        id: ValueId(9),
        ty: crate::types::ValueType::I32,
    });
    let errors = crate::mir::verify_module(&verified_array_map_module(function)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("escapes before initialization")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_loop_that_does_not_initialize_each_element() {
    let mut function = array_map_function();
    function.blocks[2].instructions.remove(0);

    let errors = verify_array_maps(&function).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message.contains("never initializes")
            || error.message.contains("without initializing")
    }));
}

#[test]
fn rejects_a_loop_that_does_not_advance_by_one() {
    let mut function = array_map_function();
    let Some(Instruction::Primitive { op, .. }) = function.blocks[2].instructions.get_mut(2) else {
        panic!("expected the loop increment instruction");
    };
    *op = NumericOp::I32Sub;

    let errors = verify_array_maps(&function).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("advance its index by one"))
    );
}

#[test]
fn rejects_a_loop_iteration_that_can_escape_before_storing() {
    let mut function = array_map_function();
    let init = BlockId(4);
    let moved = std::mem::take(&mut function.blocks[2].instructions);
    function.blocks[2].terminator = Some(Terminator::Branch {
        condition: ValueId(4),
        then_block: init,
        else_block: BlockId(3),
        span: span(),
    });
    function.blocks.push(BasicBlock {
        id: init,
        parameters: Vec::new(),
        instructions: moved,
        terminator: Some(Terminator::Jump {
            target: BlockId(1),
            arguments: vec![ValueId(7)],
            span: span(),
        }),
    });

    let errors = verify_array_maps(&function).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("without initializing")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_store_that_happens_outside_the_loop() {
    let mut function = array_map_function();
    let escaped = BlockId(4);
    let instruction = function.blocks[2].instructions.remove(0);
    function.blocks[2].instructions.push(Instruction::Constant {
        destination: ValueId(5),
        value: 0,
        span: span(),
    });
    function.blocks.push(BasicBlock {
        id: escaped,
        parameters: Vec::new(),
        instructions: vec![instruction],
        terminator: Some(Terminator::Return {
            value: ValueId(1),
            span: span(),
        }),
    });

    let errors = verify_array_maps(&function).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("never initializes")
                || error.message.contains("outside its loop")),
        "{errors:?}"
    );
}
