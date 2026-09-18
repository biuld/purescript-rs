use super::{Function, Instruction, Module, Terminator, ValueId, ValueType};
use crate::BackendError;
use psrs_core::Primitive;
use psrs_hir::{ExternalKind, RuntimeFunction, SymbolId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
struct Signature {
    parameters: Vec<ValueType>,
    result: ValueType,
}

pub fn verify_module(module: &Module) -> Result<(), Vec<BackendError>> {
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
                    result: function.result_type,
                }),
            )
        })
        .collect::<HashMap<_, _>>();
    for external in &module.externals {
        if let ExternalKind::Runtime(RuntimeFunction::ConsoleLog) = external.kind {
            signatures.insert(
                external.symbol,
                Some(Signature {
                    parameters: vec![ValueType::I32],
                    result: ValueType::I32,
                }),
            );
        }
    }
    for function in &module.functions {
        verify_function(function, &signatures)?;
    }
    Ok(())
}

fn verify_function(
    function: &Function,
    signatures: &HashMap<SymbolId, Option<Signature>>,
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
            let (destination, span) = match instruction {
                Instruction::Constant {
                    destination, span, ..
                }
                | Instruction::StringConstant {
                    destination, span, ..
                }
                | Instruction::Copy {
                    destination, span, ..
                }
                | Instruction::Primitive {
                    destination, span, ..
                }
                | Instruction::Call {
                    destination, span, ..
                } => (*destination, *span),
            };
            if !definitions.contains_key(&destination) {
                return Err(mir_error(
                    span,
                    "MIR instruction destination has no value type",
                ));
            }
            match instruction {
                Instruction::Constant { .. } => {}
                Instruction::StringConstant { .. } => {}
                Instruction::Copy { value, span, .. } => {
                    require_value(&definitions, *value, *span)?;
                }
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
                    if value_type(function, *destination) != Some(signature.result) {
                        return Err(mir_error(*span, "MIR call result has the wrong type"));
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

fn mir_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P9 MIR verification", span, message)]
}
