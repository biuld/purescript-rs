use super::Signature;
use super::util::{composite_at, mir_error, require_value, value_type};
use crate::BackendError;
use crate::mir::{Function, Instruction, ValueType};
use crate::types::{CompositeType, DefinedType, HeapType, RefType};
use psrs_hir::SymbolId;
use std::collections::HashMap;

pub(super) fn verify_ref_func(
    function: &Function,
    instruction: &Instruction,
    signatures: &HashMap<SymbolId, Option<Signature>>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::RefFunc {
        destination,
        function: callee,
        type_index,
        span,
    } = instruction
    else {
        unreachable!("ref.func verifier received another instruction");
    };
    if !signatures.contains_key(callee) {
        return Err(mir_error(*span, "MIR ref.func target has no signature"));
    }
    if !matches!(
        composite_at(defined, *type_index),
        Some(CompositeType::Func { .. })
    ) {
        return Err(mir_error(*span, "MIR ref.func type is not a function type"));
    }
    if value_type(function, *destination)
        != Some(ValueType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(*type_index),
        }))
    {
        return Err(mir_error(*span, "MIR ref.func result has the wrong type"));
    }
    Ok(())
}

pub(super) fn verify_call_ref(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<crate::mir::ValueId, ValueType>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::CallRef {
        destination,
        function: callee,
        type_index,
        arguments,
        span,
    } = instruction
    else {
        unreachable!("call_ref verifier received another instruction");
    };
    let Some(CompositeType::Func {
        parameters,
        results,
    }) = composite_at(defined, *type_index)
    else {
        return Err(mir_error(*span, "MIR call_ref type is not a function type"));
    };
    let expected_function = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(*type_index),
    });
    if require_value(definitions, *callee, *span)? != expected_function {
        return Err(mir_error(*span, "MIR call_ref target has the wrong type"));
    }
    if arguments.len() != parameters.len()
        || arguments
            .iter()
            .zip(parameters)
            .any(|(argument, expected)| {
                require_value(definitions, *argument, *span).ok() != Some(*expected)
            })
    {
        return Err(mir_error(*span, "MIR call_ref argument has the wrong type"));
    }
    if results.len() != 1 || value_type(function, *destination) != results.first().copied() {
        return Err(mir_error(*span, "MIR call_ref result has the wrong type"));
    }
    Ok(())
}
