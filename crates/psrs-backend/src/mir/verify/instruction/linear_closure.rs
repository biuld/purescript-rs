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
        capture_offsets,
        allocation_bytes,
        allocation_alignment,
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
    let expected_bytes = u32::try_from(captures.len())
        .ok()
        .and_then(|count| count.checked_mul(8))
        .and_then(|bytes| bytes.checked_add(8));
    if *allocation_alignment != 8
        || expected_bytes != Some(*allocation_bytes)
        || capture_offsets.len() != captures.len()
    {
        return Err(mir_error(*span, "MIR linear closure layout is invalid"));
    }
    for (index, (capture, offset)) in captures.iter().zip(capture_offsets).enumerate() {
        let capture_type = require_value(definitions, *capture, *span)?;
        let expected_offset = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_mul(8))
            .and_then(|offset| offset.checked_add(8));
        let capture_size = if capture_type == ValueType::F64 { 8 } else { 4 };
        if !linear_capture_type(capture_type)
            || expected_offset != Some(*offset)
            || offset
                .checked_add(capture_size)
                .is_none_or(|end| end > *allocation_bytes)
        {
            return Err(mir_error(
                *span,
                "MIR linear closure capture does not fit its slot",
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
