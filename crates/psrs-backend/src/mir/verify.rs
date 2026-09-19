use super::{Function, Instruction, Module, Terminator, ValueId, ValueType};
use crate::BackendError;
use crate::types::{CompositeType, HeapType, StorageType};
use psrs_core::Primitive;
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
struct Signature {
    parameters: Vec<ValueType>,
    result: Option<ValueType>,
}

pub fn verify_module(module: &Module) -> Result<(), Vec<BackendError>> {
    let mut errors = Vec::new();
    verify_defined_types(module, &mut errors);
    let mut signatures = module
        .functions
        .iter()
        .map(|function| {
            let types = function
                .parameters
                .iter()
                .map(|parameter| value_type(function, *parameter))
                .collect::<Option<Vec<_>>>();
            (
                function.symbol,
                types.map(|parameters| Signature {
                    parameters,
                    result: Some(function.result_type),
                }),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut import_symbols = HashSet::new();
    for import in &module.imports {
        if !import_symbols.insert(import.symbol) {
            errors.extend(mir_error(module.span, "MIR import symbols are duplicated"));
        }
        if import.module.is_empty() || import.name.is_empty() {
            errors.extend(mir_error(
                module.span,
                "MIR import has an empty module or field name",
            ));
        }
        signatures.insert(
            import.symbol,
            Some(Signature {
                parameters: import.parameters.clone(),
                result: import.result,
            }),
        );
    }
    let defined_types = defined_type_count(module);
    for function in &module.functions {
        for value in &function.values {
            verify_value_type(&value.ty, defined_types, module.span, &mut errors);
        }
        verify_function(function, &signatures, defined_types)?;
    }
    Ok(())
}

fn defined_type_count(module: &Module) -> u32 {
    module.types.iter().map(|group| group.0.len() as u32).sum()
}

fn verify_defined_types(module: &Module, errors: &mut Vec<BackendError>) {
    let count = defined_type_count(module);
    for group in &module.types {
        for def in &group.0 {
            if let Some(supertype) = def.supertype
                && supertype >= count
            {
                errors.extend(mir_error(
                    module.span,
                    "MIR defined type supertype is out of range",
                ));
            }
            match &def.composite {
                CompositeType::Func {
                    parameters,
                    results,
                } => {
                    for ty in parameters.iter().chain(results) {
                        verify_value_type(ty, count, module.span, errors);
                    }
                }
                CompositeType::Struct(fields) => {
                    for field in fields {
                        verify_storage_type(field.storage, count, module.span, errors);
                    }
                }
                CompositeType::Array(field) => {
                    verify_storage_type(field.storage, count, module.span, errors);
                }
            }
        }
    }
}

fn verify_value_type(ty: &ValueType, count: u32, span: TextRange, errors: &mut Vec<BackendError>) {
    if let ValueType::Ref(reference) = ty {
        verify_heap(reference.heap, count, span, errors);
    }
}

fn verify_storage_type(
    storage: StorageType,
    count: u32,
    span: TextRange,
    errors: &mut Vec<BackendError>,
) {
    if let StorageType::Ref(reference) = storage {
        verify_heap(reference.heap, count, span, errors);
    }
}

fn verify_heap(heap: HeapType, count: u32, span: TextRange, errors: &mut Vec<BackendError>) {
    if let HeapType::Index(index) = heap
        && index >= count
    {
        errors.extend(mir_error(
            span,
            "MIR defined type reference is out of range",
        ));
    }
}

fn verify_function(
    function: &Function,
    signatures: &HashMap<SymbolId, Option<Signature>>,
    defined_types: u32,
) -> Result<(), Vec<BackendError>> {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    if blocks.len() != function.blocks.len() || !blocks.contains_key(&function.entry) {
        return Err(mir_error(
            function.span,
            "MIR block IDs are duplicated or entry is missing",
        ));
    }
    let mut definitions = HashMap::<ValueId, ValueType>::new();
    for value in &function.values {
        if definitions.insert(value.id, value.ty).is_some() {
            return Err(mir_error(function.span, "MIR value IDs are duplicated"));
        }
    }
    let mut block_ids = HashSet::new();
    for block in &function.blocks {
        for parameter in &block.parameters {
            if !block_ids.insert((block.id, *parameter)) {
                return Err(mir_error(
                    function.span,
                    "MIR block parameter is duplicated",
                ));
            }
            if !definitions.contains_key(parameter) {
                return Err(mir_error(
                    function.span,
                    "MIR block parameter has no value type",
                ));
            }
        }
        if block.terminator.is_none() {
            return Err(mir_error(
                function.span,
                "MIR basic block has no terminator",
            ));
        }
    }
    for block in &function.blocks {
        for instruction in &block.instructions {
            let span = instruction.span();
            if let Some(destination) = instruction.destination()
                && !definitions.contains_key(&destination)
            {
                return Err(mir_error(
                    span,
                    "MIR instruction destination has no value type",
                ));
            }
            for operand in instruction.operands() {
                require_value(&definitions, operand, span)?;
            }
            match instruction {
                Instruction::Constant { .. } => {}
                Instruction::StringConstant { .. } => {}
                Instruction::Copy { .. } => {}
                Instruction::Primitive {
                    destination,
                    op,
                    left,
                    right,
                    span,
                } => {
                    let left_ty = require_value(&definitions, *left, *span)?;
                    let right_ty = require_value(&definitions, *right, *span)?;
                    let result_ty = value_type(function, *destination)
                        .ok_or_else(|| mir_error(*span, "missing MIR result type"))?;
                    let expected_result = match op {
                        Primitive::Eq
                        | Primitive::Ne
                        | Primitive::LtS
                        | Primitive::LeS
                        | Primitive::GtS
                        | Primitive::GeS => ValueType::Boolean,
                        _ => ValueType::I32,
                    };
                    if left_ty != ValueType::I32
                        || right_ty != ValueType::I32
                        || result_ty != expected_result
                    {
                        return Err(mir_error(
                            *span,
                            "MIR primitive operand or result type is invalid",
                        ));
                    }
                }
                Instruction::Call {
                    destination,
                    function: callee,
                    arguments,
                    span,
                } => {
                    let Some(Some(signature)) = signatures.get(callee) else {
                        return Err(mir_error(*span, "MIR call target has no valid signature"));
                    };
                    if arguments.len() != signature.parameters.len() {
                        return Err(mir_error(
                            *span,
                            "MIR call has the wrong number of arguments",
                        ));
                    }
                    for (argument, expected) in arguments.iter().zip(&signature.parameters) {
                        if require_value(&definitions, *argument, *span)? != *expected {
                            return Err(mir_error(*span, "MIR call argument has the wrong type"));
                        }
                    }
                    let Some(expected) = signature.result else {
                        return Err(mir_error(
                            *span,
                            "MIR call to a void import has a destination",
                        ));
                    };
                    if value_type(function, *destination) != Some(expected) {
                        return Err(mir_error(*span, "MIR call result has the wrong type"));
                    }
                }
                Instruction::CallVoid {
                    function: callee,
                    arguments,
                    span,
                } => {
                    let Some(Some(signature)) = signatures.get(callee) else {
                        return Err(mir_error(*span, "MIR call target has no valid signature"));
                    };
                    if signature.result.is_some() {
                        return Err(mir_error(
                            *span,
                            "MIR void call targets a function that returns a value",
                        ));
                    }
                    if arguments.len() != signature.parameters.len() {
                        return Err(mir_error(
                            *span,
                            "MIR call has the wrong number of arguments",
                        ));
                    }
                    for (argument, expected) in arguments.iter().zip(&signature.parameters) {
                        if require_value(&definitions, *argument, *span)? != *expected {
                            return Err(mir_error(*span, "MIR call argument has the wrong type"));
                        }
                    }
                }
                Instruction::RefNull { heap, span, .. } => {
                    check_heap(*heap, defined_types, *span)?;
                }
                Instruction::RefTest {
                    reference, span, ..
                }
                | Instruction::RefCast {
                    reference, span, ..
                } => {
                    check_heap(reference.heap, defined_types, *span)?;
                }
                Instruction::StructNew {
                    type_index, span, ..
                }
                | Instruction::StructGet {
                    type_index, span, ..
                }
                | Instruction::StructSet {
                    type_index, span, ..
                }
                | Instruction::ArrayNew {
                    type_index, span, ..
                }
                | Instruction::ArrayGet {
                    type_index, span, ..
                }
                | Instruction::ArraySet {
                    type_index, span, ..
                } => {
                    if *type_index >= defined_types {
                        return Err(mir_error(*span, "MIR type index is out of range"));
                    }
                }
                Instruction::RefIsNull { .. }
                | Instruction::I31New { .. }
                | Instruction::I31GetS { .. }
                | Instruction::ArrayLen { .. }
                | Instruction::Load { .. }
                | Instruction::Store { .. } => {}
                Instruction::WrapI64 {
                    destination,
                    value,
                    span,
                } => {
                    if require_value(&definitions, *value, *span)? != ValueType::I64
                        || value_type(function, *destination) != Some(ValueType::I32)
                    {
                        return Err(mir_error(
                            *span,
                            "MIR i32.wrap_i64 operand or result type is invalid",
                        ));
                    }
                }
            }
        }
        let terminator = block.terminator.as_ref().expect("checked above");
        match terminator {
            Terminator::Return { value, span } => {
                if require_value(&definitions, *value, *span)? != function.result_type {
                    return Err(mir_error(*span, "MIR return value has the wrong type"));
                }
            }
            Terminator::Jump {
                target,
                arguments,
                span,
            } => {
                let Some(target) = blocks.get(target) else {
                    return Err(mir_error(*span, "MIR jump target does not exist"));
                };
                if target.parameters.len() != arguments.len() {
                    return Err(mir_error(
                        *span,
                        "MIR jump argument count differs from target parameters",
                    ));
                }
                for (argument, parameter) in arguments.iter().zip(&target.parameters) {
                    let expected = value_type(function, *parameter)
                        .ok_or_else(|| mir_error(*span, "MIR block parameter has no type"))?;
                    if require_value(&definitions, *argument, *span)? != expected {
                        return Err(mir_error(*span, "MIR jump argument has the wrong type"));
                    }
                }
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
                merge_block,
                span,
            } => {
                if require_value(&definitions, *condition, *span)? != ValueType::Boolean {
                    return Err(mir_error(*span, "MIR branch condition is not Boolean"));
                }
                if !blocks.contains_key(then_block) || !blocks.contains_key(else_block) {
                    return Err(mir_error(*span, "MIR branch target does not exist"));
                }
                if blocks
                    .get(merge_block)
                    .is_none_or(|block| block.parameters.len() != 1)
                {
                    return Err(mir_error(
                        *span,
                        "MIR branch merge must have one result parameter",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn value_type(function: &Function, value: ValueId) -> Option<ValueType> {
    function
        .values
        .iter()
        .find(|decl| decl.id == value)
        .map(|decl| decl.ty)
}

fn require_value(
    definitions: &HashMap<ValueId, ValueType>,
    value: ValueId,
    span: TextRange,
) -> Result<ValueType, Vec<BackendError>> {
    definitions
        .get(&value)
        .copied()
        .ok_or_else(|| mir_error(span, "MIR instruction uses an unknown value"))
}

fn check_heap(heap: HeapType, count: u32, span: TextRange) -> Result<(), Vec<BackendError>> {
    if let HeapType::Index(index) = heap
        && index >= count
    {
        return Err(mir_error(
            span,
            "MIR defined type reference is out of range",
        ));
    }
    Ok(())
}

fn mir_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P9 MIR verification", span, message)]
}
