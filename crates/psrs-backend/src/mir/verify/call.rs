use super::Signature;
use super::util::{
    composite_at, is_struct_reference, mir_error, require_value, storage_value_type, value_type,
};
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
    let Some(Some(signature)) = signatures.get(callee) else {
        return Err(mir_error(
            *span,
            "MIR ref.func target has no valid signature",
        ));
    };
    let Some(CompositeType::Func {
        parameters,
        results,
    }) = composite_at(defined, *type_index)
    else {
        return Err(mir_error(*span, "MIR ref.func type is not a function type"));
    };
    if signature.parameters != *parameters
        || results.len() != 1
        || signature.result != results.first().copied()
    {
        return Err(mir_error(
            *span,
            "MIR ref.func type does not match its target signature",
        ));
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

pub(super) fn verify_closure_new(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<crate::mir::ValueId, ValueType>,
    signatures: &HashMap<SymbolId, Option<Signature>>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::ClosureNew {
        destination,
        function: callee,
        type_index,
        closure_type,
        capture_array_type,
        boxed_f64_type,
        captures,
        span,
    } = instruction
    else {
        unreachable!("closure.new verifier received another instruction");
    };
    let Some(CompositeType::Func {
        parameters,
        results,
    }) = composite_at(defined, *type_index)
    else {
        return Err(mir_error(*span, "closure code type is not a function type"));
    };
    let Some(Some(signature)) = signatures.get(callee) else {
        return Err(mir_error(*span, "closure code target has no signature"));
    };
    if signature.parameters != *parameters
        || results.len() != 1
        || signature.result != results.first().copied()
    {
        return Err(mir_error(
            *span,
            "closure code signature does not match its target",
        ));
    }
    verify_closure_layout(*closure_type, *capture_array_type, defined, *span)?;
    if !is_struct_reference(
        value_type(function, *destination)
            .ok_or_else(|| mir_error(*span, "closure.new result has no value type"))?,
        *closure_type,
        defined,
    ) {
        return Err(mir_error(*span, "closure.new result must be a reference"));
    }
    for capture in captures {
        let capture_type = require_value(definitions, *capture, *span)?;
        if !matches!(
            capture_type,
            ValueType::I32 | ValueType::Boolean | ValueType::Ref(_)
        ) {
            if capture_type == ValueType::F64 {
                verify_f64_box(*boxed_f64_type, defined, *span)?;
                continue;
            }
            return Err(mir_error(
                *span,
                "closure capture type is not representable in an eqref array",
            ));
        }
    }
    Ok(())
}

pub(super) fn verify_closure_call(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<crate::mir::ValueId, ValueType>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::ClosureCall {
        destination,
        function: callee,
        type_index,
        closure_type,
        capture_array_type,
        arguments,
        span,
    } = instruction
    else {
        unreachable!("closure.call verifier received another instruction");
    };
    verify_closure_layout(*closure_type, *capture_array_type, defined, *span)?;
    if !is_struct_reference(
        require_value(definitions, *callee, *span)?,
        *closure_type,
        defined,
    ) {
        return Err(mir_error(*span, "closure.call target must be a reference"));
    }
    let Some(CompositeType::Func {
        parameters,
        results,
    }) = composite_at(defined, *type_index)
    else {
        return Err(mir_error(*span, "closure call type is not a function type"));
    };
    if parameters.first().copied() != Some(closure_value_type())
        || parameters.len() != arguments.len() + 1
        || arguments
            .iter()
            .zip(&parameters[1..])
            .any(|(argument, expected)| {
                require_value(definitions, *argument, *span).ok() != Some(*expected)
            })
        || results.len() != 1
        || value_type(function, *destination) != results.first().copied()
    {
        return Err(mir_error(
            *span,
            "closure.call operands have the wrong type",
        ));
    }
    Ok(())
}

pub(super) fn verify_closure_get_capture(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<crate::mir::ValueId, ValueType>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::ClosureGetCapture {
        destination,
        closure,
        closure_type,
        capture_array_type,
        boxed_f64_type,
        index: _,
        span,
    } = instruction
    else {
        unreachable!("closure.get_capture verifier received another instruction");
    };
    verify_closure_layout(*closure_type, *capture_array_type, defined, *span)?;
    if !is_struct_reference(
        require_value(definitions, *closure, *span)?,
        *closure_type,
        defined,
    ) {
        return Err(mir_error(
            *span,
            "closure capture source must be a reference",
        ));
    }
    let Some(CompositeType::Array(field)) = composite_at(defined, *capture_array_type) else {
        return Err(mir_error(
            *span,
            "closure capture array type is not an array",
        ));
    };
    if storage_value_type(&field.storage).is_none() {
        return Err(mir_error(
            *span,
            "closure capture array is not representable",
        ));
    }
    let Some(destination_type) = value_type(function, *destination) else {
        return Err(mir_error(*span, "closure capture result has no value type"));
    };
    if destination_type == ValueType::F64 {
        verify_f64_box(*boxed_f64_type, defined, *span)?;
    }
    Ok(())
}

fn verify_f64_box(
    boxed_f64_type: Option<u32>,
    defined: &[&DefinedType],
    span: psrs_span::TextRange,
) -> Result<(), Vec<BackendError>> {
    let Some(index) = boxed_f64_type else {
        return Err(mir_error(span, "F64 closure capture has no box layout"));
    };
    let Some(CompositeType::Struct(fields)) = composite_at(defined, index) else {
        return Err(mir_error(span, "F64 closure capture box is not a struct"));
    };
    if fields.len() != 1 || storage_value_type(&fields[0].storage) != Some(ValueType::F64) {
        return Err(mir_error(
            span,
            "F64 closure capture box has the wrong layout",
        ));
    }
    Ok(())
}

fn verify_closure_layout(
    closure_type: u32,
    capture_array_type: u32,
    defined: &[&DefinedType],
    span: psrs_span::TextRange,
) -> Result<(), Vec<BackendError>> {
    let Some(CompositeType::Struct(fields)) = composite_at(defined, closure_type) else {
        return Err(mir_error(span, "closure type is not a struct"));
    };
    if fields.len() != 2 {
        return Err(mir_error(
            span,
            "closure type must have code and capture fields",
        ));
    }
    if !matches!(
        fields[0].storage,
        crate::types::StorageType::Ref(RefType {
            nullable: false,
            heap: HeapType::Func,
        })
    ) || !matches!(
        fields[1].storage,
        crate::types::StorageType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(index),
        }) if index == capture_array_type
    ) {
        return Err(mir_error(span, "closure fields have the wrong layout"));
    }
    let Some(CompositeType::Array(field)) = composite_at(defined, capture_array_type) else {
        return Err(mir_error(span, "closure capture type is not an array"));
    };
    if field.storage
        != crate::types::StorageType::Ref(RefType {
            nullable: true,
            heap: HeapType::Eq,
        })
    {
        return Err(mir_error(span, "closure captures must use nullable eqref"));
    }
    Ok(())
}

fn closure_value_type() -> ValueType {
    ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Struct,
    })
}
