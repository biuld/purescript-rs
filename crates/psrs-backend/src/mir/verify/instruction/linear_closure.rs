//! Type checks for the MVP linear-memory closure ABI.

use super::super::Signature;
use super::super::util::{
    call_value_types_match, composite_at, mir_error, require_value, value_type,
};
use crate::BackendError;
use crate::mir::{Function, Instruction, ValueId, ValueType};
use crate::types::{CompositeType, DefinedType};
use psrs_hir::SymbolId;
use std::collections::HashMap;

pub(super) fn verify_new(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
    signatures: &HashMap<SymbolId, Option<Signature>>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::LinearClosureNew {
        destination,
        function: callee,
        type_index,
        captures,
        span,
        ..
    } = instruction
    else {
        unreachable!("linear closure verifier received another instruction");
    };
    let Some(Some(signature)) = signatures.get(callee) else {
        return Err(mir_error(
            *span,
            "MIR linear closure target has no signature",
        ));
    };
    let Some(CompositeType::Func {
        parameters,
        results,
    }) = composite_at(defined, *type_index)
    else {
        return Err(mir_error(
            *span,
            "MIR linear closure type is not a function type",
        ));
    };
    if signature.parameters != *parameters
        || results.len() != 1
        || signature.result != results.first().copied()
        || parameters.first() != Some(&ValueType::I32)
    {
        return Err(mir_error(
            *span,
            "MIR linear closure type does not match its target",
        ));
    }
    if value_type(function, *destination) != Some(ValueType::I32) {
        return Err(mir_error(
            *span,
            "MIR linear closure environment has an invalid capture",
        ));
    }
    for capture in captures {
        if !linear_capture_type(require_value(definitions, *capture, *span)?) {
            return Err(mir_error(
                *span,
                "MIR linear closure environment has an invalid capture",
            ));
        }
    }
    Ok(())
}

pub(super) fn verify_call(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::LinearClosureCall {
        destination,
        function: callee,
        type_index,
        arguments,
        span,
    } = instruction
    else {
        unreachable!("linear closure verifier received another instruction");
    };
    let Some(CompositeType::Func {
        parameters,
        results,
    }) = composite_at(defined, *type_index)
    else {
        return Err(mir_error(
            *span,
            "MIR linear closure call type is not a function type",
        ));
    };
    if parameters.first() != Some(&ValueType::I32)
        || require_value(definitions, *callee, *span)? != ValueType::I32
        || parameters.len() != arguments.len() + 1
        || results.len() != 1
        || value_type(function, *destination) != results.first().copied()
    {
        return Err(mir_error(
            *span,
            "MIR linear closure call signature is invalid",
        ));
    }
    for (argument, expected) in arguments.iter().zip(parameters.iter().skip(1)) {
        if !call_value_types_match(require_value(definitions, *argument, *span)?, *expected) {
            return Err(mir_error(
                *span,
                "MIR linear closure call argument has the wrong type",
            ));
        }
    }
    Ok(())
}

fn linear_capture_type(ty: ValueType) -> bool {
    matches!(ty, ValueType::I32 | ValueType::Boolean | ValueType::F64)
}
