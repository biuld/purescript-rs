//! Full-module negative fixtures for dictionary lowering (DICT-09).
//!
//! A class dictionary reaches P9 as an ordinary GC struct whose fields are
//! method closures and (nested) superclass dictionaries. These fixtures mutate
//! a well-formed dictionary module so the MIR verifier must reject the result
//! as invalid compiler IR rather than accepting a class-specific encoding.

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

fn final_type(composite: CompositeType) -> DefinedType {
    DefinedType {
        final_type: true,
        supertype: None,
        composite,
    }
}

fn field(storage: StorageType) -> FieldType {
    FieldType {
        storage,
        mutable: false,
    }
}

/// A dictionary record with one method closure and one nested superclass
/// dictionary field, followed by a method closure signature.
fn dictionary_types() -> Vec<RecGroup> {
    vec![RecGroup(vec![
        final_type(CompositeType::Struct(vec![
            field(StorageType::Ref(RefType {
                nullable: false,
                heap: HeapType::Func,
            })),
            field(StorageType::Ref(RefType {
                nullable: false,
                heap: HeapType::Index(DefinedTypeId(0)),
            })),
        ])),
        final_type(CompositeType::Func {
            parameters: Vec::new(),
            results: vec![ValueType::I32],
        }),
    ])]
}

fn return_function(
    name: &str,
    parameters: Vec<ValueId>,
    values: Vec<ValueDecl>,
    instructions: Vec<Instruction>,
    result: ValueId,
    result_type: ValueType,
) -> Function {
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: name.into(),
        parameters,
        values,
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions,
            terminator: Some(Terminator::Return {
                value: result,
                span: span(),
            }),
        }],
        result,
        result_type,
        span: span(),
    }
}

fn dictionary_error(message: &str, errors: &[crate::BackendError]) {
    let error = errors
        .iter()
        .find(|error| error.message.contains(message))
        .unwrap_or_else(|| panic!("expected an error containing `{message}`: {errors:?}"));
    assert_eq!(error.kind, BackendErrorKind::InvalidCompilerIr);
    assert_eq!(error.pass, "P9 MIR verification");
}

#[test]
fn rejects_dictionary_struct_new_with_wrong_arity() {
    let method = ValueId(0);
    let dictionary = ValueId(1);
    let function = return_function(
        "wrong_arity",
        vec![method],
        vec![
            value(0, reference(HeapType::Func)),
            value(1, reference(HeapType::Index(DefinedTypeId(0)))),
        ],
        vec![Instruction::StructNew {
            destination: dictionary,
            type_index: DefinedTypeId(0),
            arguments: vec![method],
            span: span(),
        }],
        dictionary,
        reference(HeapType::Index(DefinedTypeId(0))),
    );
    // A one-argument struct.new cannot fill both the method and superclass
    // fields of the dictionary layout.
    let errors =
        crate::mir::verify_module(&module_with_function(function, dictionary_types())).unwrap_err();
    dictionary_error("argument count differs from the struct fields", &errors);
}

#[test]
fn rejects_dictionary_struct_new_with_wrong_field_type() {
    let method = ValueId(0);
    let dictionary = ValueId(1);
    let function = return_function(
        "wrong_field",
        vec![method],
        vec![
            value(0, ValueType::I32),
            value(1, reference(HeapType::Index(DefinedTypeId(0)))),
        ],
        vec![Instruction::StructNew {
            destination: dictionary,
            type_index: DefinedTypeId(0),
            arguments: vec![method, method],
            span: span(),
        }],
        dictionary,
        reference(HeapType::Index(DefinedTypeId(0))),
    );
    // An integer method argument must not be stored in the closure field.
    let errors =
        crate::mir::verify_module(&module_with_function(function, dictionary_types())).unwrap_err();
    dictionary_error("argument has the wrong type", &errors);
}

#[test]
fn rejects_dictionary_struct_get_out_of_range() {
    let dictionary = ValueId(0);
    let projected = ValueId(1);
    let function = return_function(
        "wrong_field_index",
        vec![dictionary],
        vec![
            value(0, reference(HeapType::Index(DefinedTypeId(0)))),
            value(1, reference(HeapType::Func)),
        ],
        vec![Instruction::StructGet {
            destination: projected,
            type_index: DefinedTypeId(0),
            field: 2,
            value: dictionary,
            span: span(),
        }],
        projected,
        reference(HeapType::Func),
    );
    let errors =
        crate::mir::verify_module(&module_with_function(function, dictionary_types())).unwrap_err();
    dictionary_error("field is out of range", &errors);
}

#[test]
fn rejects_dictionary_struct_get_with_wrong_result_type() {
    let dictionary = ValueId(0);
    let projected = ValueId(1);
    let function = return_function(
        "wrong_projection_type",
        vec![dictionary],
        vec![
            value(0, reference(HeapType::Index(DefinedTypeId(0)))),
            value(1, ValueType::I32),
        ],
        vec![Instruction::StructGet {
            destination: projected,
            type_index: DefinedTypeId(0),
            field: 0,
            value: dictionary,
            span: span(),
        }],
        projected,
        ValueType::I32,
    );
    // The method field stores a closure reference, not an i32.
    let errors =
        crate::mir::verify_module(&module_with_function(function, dictionary_types())).unwrap_err();
    dictionary_error("result has the wrong type", &errors);
}

fn method_function() -> Function {
    let result = ValueId(0);
    Function {
        id: FunctionId(1),
        symbol: SymbolId::new(ModuleId(0), 1),
        name: "method".into(),
        parameters: Vec::new(),
        values: vec![value(0, ValueType::I32)],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::Constant {
                destination: result,
                value: 42,
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: result,
                span: span(),
            }),
        }],
        result,
        result_type: ValueType::I32,
        span: span(),
    }
}

#[test]
fn rejects_dictionary_method_call_with_wrong_arity() {
    let method = ValueId(0);
    let destination = ValueId(1);
    let mut module = module_with_function(
        return_function(
            "wrong_call_arity",
            vec![method],
            vec![
                value(0, reference(HeapType::Func)),
                value(1, ValueType::I32),
            ],
            vec![Instruction::Call {
                destination,
                function: SymbolId::new(ModuleId(0), 1),
                arguments: vec![method],
                span: span(),
            }],
            destination,
            ValueType::I32,
        ),
        Vec::new(),
    );
    module.functions.push(method_function());
    let errors = crate::mir::verify_module(&module).unwrap_err();
    dictionary_error("wrong number of arguments", &errors);
}

#[test]
fn rejects_a_non_dominating_dictionary_projection() {
    let condition = ValueId(0);
    let method = ValueId(1);
    let super_dict = ValueId(2);
    let dictionary = ValueId(3);
    let projected = ValueId(4);
    let function = Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "non_dominating_projection".into(),
        parameters: vec![condition, method, super_dict],
        values: vec![
            value(0, ValueType::Boolean),
            value(1, reference(HeapType::Func)),
            value(2, reference(HeapType::Index(DefinedTypeId(0)))),
            value(3, reference(HeapType::Index(DefinedTypeId(0)))),
            value(4, reference(HeapType::Func)),
        ],
        entry: BlockId(0),
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Branch {
                    condition,
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: Vec::new(),
                instructions: vec![Instruction::StructNew {
                    destination: dictionary,
                    type_index: DefinedTypeId(0),
                    arguments: vec![method, super_dict],
                    span: span(),
                }],
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: Vec::new(),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: Vec::new(),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(3),
                parameters: Vec::new(),
                instructions: vec![Instruction::StructGet {
                    destination: projected,
                    type_index: DefinedTypeId(0),
                    field: 0,
                    value: dictionary,
                    span: span(),
                }],
                terminator: Some(Terminator::Return {
                    value: projected,
                    span: span(),
                }),
            },
        ],
        result: projected,
        result_type: reference(HeapType::Func),
        span: span(),
    };
    let errors =
        crate::mir::verify_module(&module_with_function(function, dictionary_types())).unwrap_err();
    dictionary_error("outside its dominance scope", &errors);
}
