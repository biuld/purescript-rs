//! Full-module fixtures for the MIR-07/MIR-08 verifier gaps: reference casts,
//! closure captures, and generated aggregate conversion helpers.

use super::{module_with_function, span};
use crate::BackendErrorKind;
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

fn nullable_eq_storage() -> StorageType {
    StorageType::Ref(RefType {
        nullable: true,
        heap: HeapType::Eq,
    })
}

fn closure_value_type() -> ValueType {
    reference(HeapType::Struct)
}

fn custom_module(function: Function, types: Vec<RecGroup>) -> crate::mir::Module {
    module_with_function(function, types)
}

fn final_type(composite: CompositeType) -> DefinedType {
    DefinedType {
        final_type: true,
        supertype: None,
        composite,
    }
}

#[test]
fn rejects_ref_cast_between_unrelated_heaps() {
    let input = ValueId(0);
    let output = ValueId(1);
    let struct_type = final_type(CompositeType::Struct(Vec::new()));
    let function_type = final_type(CompositeType::Func {
        parameters: Vec::new(),
        results: Vec::new(),
    });
    let function = Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "bad_cast".into(),
        parameters: vec![input],
        values: vec![
            value(0, reference(HeapType::Index(DefinedTypeId(0)))),
            value(1, reference(HeapType::Index(DefinedTypeId(1)))),
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::RefCast {
                destination: output,
                value: input,
                reference: RefType {
                    nullable: false,
                    heap: HeapType::Index(DefinedTypeId(1)),
                },
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: output,
                span: span(),
            }),
        }],
        result: output,
        result_type: reference(HeapType::Index(DefinedTypeId(1))),
        span: span(),
    };
    let errors = crate::mir::verify_module(&custom_module(
        function,
        vec![RecGroup(vec![struct_type, function_type])],
    ))
    .unwrap_err();
    let error = errors
        .iter()
        .find(|error| error.message.contains("ref.cast operand and target heaps"))
        .expect("unrelated cast should be rejected");
    assert_eq!(error.kind, BackendErrorKind::InvalidCompilerIr);
    assert_eq!(error.span, span());
}

#[test]
fn accepts_an_erased_upcast_to_eqref() {
    let input = ValueId(0);
    let output = ValueId(1);
    let struct_type = final_type(CompositeType::Struct(Vec::new()));
    let function = Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "erased_upcast".into(),
        parameters: vec![input],
        values: vec![
            value(0, reference(HeapType::Index(DefinedTypeId(0)))),
            value(1, reference(HeapType::Eq)),
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::RefCast {
                destination: output,
                value: input,
                reference: RefType {
                    nullable: false,
                    heap: HeapType::Eq,
                },
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: output,
                span: span(),
            }),
        }],
        result: output,
        result_type: reference(HeapType::Eq),
        span: span(),
    };
    crate::mir::verify_module(&custom_module(function, vec![RecGroup(vec![struct_type])]))
        .expect("an aggregate-to-eqref upcast is legal");
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
        storage: nullable_eq_storage(),
        mutable: false,
    }));
    vec![RecGroup(vec![function_type, closure_type, capture_array])]
}

fn closure_callee() -> Function {
    let closure = ValueId(0);
    let argument = ValueId(1);
    Function {
        id: FunctionId(1),
        symbol: SymbolId::new(ModuleId(0), 1),
        name: "callee".into(),
        parameters: vec![closure, argument],
        values: vec![value(0, closure_value_type()), value(1, ValueType::I32)],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Some(Terminator::Return {
                value: argument,
                span: span(),
            }),
        }],
        result: argument,
        result_type: ValueType::I32,
        span: span(),
    }
}

#[test]
fn rejects_a_closure_capture_outside_the_eq_hierarchy() {
    let capture = ValueId(0);
    let closure = ValueId(1);
    let mut module = custom_module(
        Function {
            id: FunctionId(0),
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "bad_capture".into(),
            parameters: Vec::new(),
            values: vec![
                value(0, reference(HeapType::Index(DefinedTypeId(0)))),
                value(1, reference(HeapType::Index(DefinedTypeId(1)))),
            ],
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::RefFunc {
                        destination: capture,
                        function: SymbolId::new(ModuleId(0), 1),
                        type_index: DefinedTypeId(0),
                        span: span(),
                    },
                    Instruction::ClosureNew {
                        destination: closure,
                        function: SymbolId::new(ModuleId(0), 1),
                        type_index: DefinedTypeId(0),
                        closure_type: DefinedTypeId(1),
                        capture_array_type: DefinedTypeId(2),
                        boxed_integer_type: None,
                        boxed_f64_type: None,
                        captures: vec![capture],
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: closure,
                    span: span(),
                }),
            }],
            result: closure,
            result_type: reference(HeapType::Index(DefinedTypeId(1))),
            span: span(),
        },
        closure_types(),
    );
    module.functions.push(closure_callee());
    let errors = crate::mir::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("not eq-compatible")),
        "{errors:?}"
    );
}

fn convertible_types() -> Vec<RecGroup> {
    let parent = DefinedType {
        final_type: false,
        supertype: None,
        composite: CompositeType::Struct(vec![FieldType {
            storage: StorageType::I32,
            mutable: false,
        }]),
    };
    let child = DefinedType {
        final_type: true,
        supertype: Some(DefinedTypeId(0)),
        composite: CompositeType::Struct(vec![
            FieldType {
                storage: StorageType::I32,
                mutable: false,
            },
            FieldType {
                storage: StorageType::I32,
                mutable: false,
            },
        ]),
    };
    vec![RecGroup(vec![parent, child])]
}

fn helper_function(blocks: Vec<BasicBlock>, values: Vec<ValueDecl>) -> Function {
    let input = ValueId(0);
    let output = ValueId(1);
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "aggregate_convert_0".into(),
        parameters: vec![input],
        values,
        entry: BlockId(0),
        blocks,
        result: output,
        result_type: reference(HeapType::Index(DefinedTypeId(1))),
        span: span(),
    }
}

#[test]
fn rejects_a_conversion_helper_that_casts_instead_of_rebuilding() {
    let function = helper_function(
        vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::RefCast {
                destination: ValueId(1),
                value: ValueId(0),
                reference: RefType {
                    nullable: false,
                    heap: HeapType::Index(DefinedTypeId(1)),
                },
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: ValueId(1),
                span: span(),
            }),
        }],
        vec![
            value(0, reference(HeapType::Index(DefinedTypeId(0)))),
            value(1, reference(HeapType::Index(DefinedTypeId(1)))),
        ],
    );
    let errors =
        crate::mir::verify_module(&custom_module(function, convertible_types())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("replaces a nominal conversion")),
        "{errors:?}"
    );
}

#[test]
fn accepts_a_conversion_helper_that_rebuilds_the_aggregate() {
    let function = helper_function(
        vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::StructGet {
                    destination: ValueId(2),
                    type_index: DefinedTypeId(0),
                    field: 0,
                    value: ValueId(0),
                    span: span(),
                },
                Instruction::StructNew {
                    destination: ValueId(1),
                    type_index: DefinedTypeId(1),
                    arguments: vec![ValueId(2), ValueId(2)],
                    span: span(),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(1),
                span: span(),
            }),
        }],
        vec![
            value(0, reference(HeapType::Index(DefinedTypeId(0)))),
            value(1, reference(HeapType::Index(DefinedTypeId(1)))),
            value(2, ValueType::I32),
        ],
    );
    crate::mir::verify_module(&custom_module(function, convertible_types()))
        .expect("a rebuilding conversion helper is legal");
}

fn get_capture_function(destination_type: ValueType) -> Function {
    let closure = ValueId(0);
    let destination = ValueId(1);
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "capture_projection".into(),
        parameters: vec![closure],
        values: vec![value(0, closure_value_type()), value(1, destination_type)],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::ClosureGetCapture {
                destination,
                closure,
                closure_type: DefinedTypeId(1),
                capture_array_type: DefinedTypeId(2),
                boxed_integer_type: None,
                boxed_f64_type: None,
                index: 0,
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: destination,
                span: span(),
            }),
        }],
        result: destination,
        result_type: destination_type,
        span: span(),
    }
}

#[test]
fn rejects_a_closure_capture_projection_with_a_non_eq_reference_result() {
    let errors = crate::mir::verify_module(&custom_module(
        get_capture_function(reference(HeapType::Func)),
        closure_types(),
    ))
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("not eq-compatible")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_closure_capture_projection_with_an_unrepresentable_result() {
    let errors = crate::mir::verify_module(&custom_module(
        get_capture_function(ValueType::F32),
        closure_types(),
    ))
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("not representable")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_conversion_helper_that_casts_even_with_an_unrelated_rebuild() {
    let function = helper_function(
        vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::RefCast {
                    destination: ValueId(1),
                    value: ValueId(0),
                    reference: RefType {
                        nullable: false,
                        heap: HeapType::Index(DefinedTypeId(1)),
                    },
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(3),
                    value: 0,
                    span: span(),
                },
                Instruction::StructNew {
                    destination: ValueId(2),
                    type_index: DefinedTypeId(1),
                    arguments: vec![ValueId(3), ValueId(3)],
                    span: span(),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(1),
                span: span(),
            }),
        }],
        vec![
            value(0, reference(HeapType::Index(DefinedTypeId(0)))),
            value(1, reference(HeapType::Index(DefinedTypeId(1)))),
            value(2, reference(HeapType::Index(DefinedTypeId(1)))),
            value(3, ValueType::I32),
        ],
    );
    let errors =
        crate::mir::verify_module(&custom_module(function, convertible_types())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("replaces a nominal conversion")),
        "{errors:?}"
    );
}
