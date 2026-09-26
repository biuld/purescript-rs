//! Instruction and terminator type checks for MIR verification.
use super::Signature;
use super::call::{verify_call_ref, verify_ref_func};
use super::util::{
    call_value_types_match, check_heap, composite_at, is_array_reference, is_ref, is_ref_opt,
    is_struct_reference, mir_error, require_value, storage_value_type,
    struct_field_type_compatible, value_type, value_type_assignable,
};
use crate::BackendError;
use crate::mir::{Function, Instruction, ValueId, ValueType};
use crate::types::{CompositeType, DefinedType, HeapType, RefType};
use psrs_hir::SymbolId;
use std::collections::HashMap;
mod arrays;
mod copy;
mod lists;
mod memory;
mod primitive;
mod unary;

pub(super) fn verify_instruction(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
    signatures: &HashMap<SymbolId, Option<Signature>>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    match instruction {
        Instruction::Copy { .. } => copy::verify_copy(function, instruction, definitions)?,
        Instruction::Constant {
            destination,
            value,
            span,
        } => {
            let Some(result) = value_type(function, *destination) else {
                return Err(mir_error(*span, "MIR constant has no result type"));
            };
            if !matches!(result, ValueType::I32 | ValueType::Boolean)
                || (result == ValueType::Boolean && !matches!(value, 0 | 1))
            {
                return Err(mir_error(
                    *span,
                    "MIR integer constant has the wrong result type",
                ));
            }
        }
        Instruction::NumberConstant {
            destination, span, ..
        } => {
            if value_type(function, *destination) != Some(ValueType::F64) {
                return Err(mir_error(*span, "MIR number constant must produce f64"));
            }
        }
        Instruction::ArrayNewData { .. } => {
            arrays::verify_array_new_data(function, instruction, defined)?
        }
        Instruction::Primitive { .. } => {
            primitive::verify_primitive(function, instruction, definitions)?
        }
        Instruction::UnaryPrimitive { .. } => {
            unary::verify_unary(function, instruction, definitions)?
        }
        Instruction::TrapIf { condition, span } => {
            if require_value(definitions, *condition, *span)? != ValueType::Boolean {
                return Err(mir_error(*span, "MIR trap condition must be Boolean"));
            }
        }
        Instruction::Unreachable { .. } => {}
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
                let actual = require_value(definitions, *argument, *span)?;
                if !call_value_types_match(actual, *expected) {
                    return Err(mir_error(*span, "MIR call argument has the wrong type"));
                }
            }
            let Some(expected) = signature.result else {
                return Err(mir_error(
                    *span,
                    "MIR call to a void import has a destination",
                ));
            };
            if value_type(function, *destination)
                .is_none_or(|actual| !call_value_types_match(actual, expected))
            {
                return Err(mir_error(*span, "MIR call result has the wrong type"));
            }
        }
        Instruction::RefFunc { .. } => verify_ref_func(function, instruction, signatures, defined)?,
        Instruction::ClosureNew { .. } => super::call::verify_closure_new(
            function,
            instruction,
            definitions,
            signatures,
            defined,
        )?,
        Instruction::CallRef { .. } => {
            verify_call_ref(function, instruction, definitions, defined)?
        }
        Instruction::ClosureCall { .. } => {
            super::call::verify_closure_call(function, instruction, definitions, defined)?
        }
        Instruction::ClosureGetCapture { .. } => {
            super::call::verify_closure_get_capture(function, instruction, definitions, defined)?
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
                if !call_value_types_match(require_value(definitions, *argument, *span)?, *expected)
                {
                    return Err(mir_error(*span, "MIR call argument has the wrong type"));
                }
            }
        }
        Instruction::RefNull {
            destination,
            heap,
            span,
        } => {
            check_heap(*heap, defined, *span)?;
            if !is_ref_opt(value_type(function, *destination)) {
                return Err(mir_error(*span, "MIR ref.null result must be a reference"));
            }
        }
        Instruction::RefIsNull {
            destination,
            value,
            span,
        } => {
            if !is_ref(require_value(definitions, *value, *span)?) {
                return Err(mir_error(
                    *span,
                    "MIR ref.is_null operand must be a reference",
                ));
            }
            if value_type(function, *destination) != Some(ValueType::Boolean) {
                return Err(mir_error(*span, "MIR ref.is_null result must be Boolean"));
            }
        }
        Instruction::RefTest {
            destination,
            value,
            reference,
            span,
        } => {
            check_heap(reference.heap, defined, *span)?;
            let ValueType::Ref(operand) = require_value(definitions, *value, *span)? else {
                return Err(mir_error(*span, "MIR ref.test operand must be a reference"));
            };
            if !super::subtype::heap_related(operand.heap, reference.heap, defined) {
                return Err(mir_error(
                    *span,
                    "MIR ref.test operand and target heaps are unrelated",
                ));
            }
            if value_type(function, *destination) != Some(ValueType::Boolean) {
                return Err(mir_error(*span, "MIR ref.test result must be Boolean"));
            }
        }
        Instruction::RefCast {
            destination,
            value,
            reference,
            span,
        } => {
            check_heap(reference.heap, defined, *span)?;
            let ValueType::Ref(operand) = require_value(definitions, *value, *span)? else {
                return Err(mir_error(*span, "MIR ref.cast operand must be a reference"));
            };
            if !super::subtype::heap_related(operand.heap, reference.heap, defined) {
                return Err(mir_error(
                    *span,
                    "MIR ref.cast operand and target heaps are unrelated",
                ));
            }
            if value_type(function, *destination) != Some(ValueType::Ref(*reference)) {
                return Err(mir_error(
                    *span,
                    "MIR ref.cast result must match the cast reference type",
                ));
            }
        }
        Instruction::I31New {
            destination,
            value,
            span,
        } => {
            if require_value(definitions, *value, *span)? != ValueType::I32 {
                return Err(mir_error(*span, "MIR i31.new operand must be i32"));
            }
            if !matches!(
                value_type(function, *destination),
                Some(ValueType::Ref(RefType {
                    heap: HeapType::I31,
                    ..
                }))
            ) {
                return Err(mir_error(
                    *span,
                    "MIR i31.new result must be an i31 reference",
                ));
            }
        }
        Instruction::I31GetS {
            destination,
            value,
            span,
        } => {
            if !matches!(
                require_value(definitions, *value, *span)?,
                ValueType::Ref(RefType {
                    heap: HeapType::I31,
                    ..
                })
            ) {
                return Err(mir_error(
                    *span,
                    "MIR i31.get_s operand must be an i31 reference",
                ));
            }
            if value_type(function, *destination) != Some(ValueType::I32) {
                return Err(mir_error(*span, "MIR i31.get_s result must be i32"));
            }
        }
        Instruction::StructNew {
            destination,
            type_index,
            arguments,
            span,
        } => {
            let Some(CompositeType::Struct(fields)) = composite_at(defined, *type_index) else {
                return Err(mir_error(*span, "MIR struct.new type is not a struct"));
            };
            if fields.len() != arguments.len() {
                return Err(mir_error(
                    *span,
                    "MIR struct.new argument count differs from the struct fields",
                ));
            }
            for (argument, field) in arguments.iter().zip(fields) {
                let expected = storage_value_type(&field.storage).ok_or_else(|| {
                    mir_error(*span, "MIR struct field storage is not representable")
                })?;
                let actual = require_value(definitions, *argument, *span)?;
                if !struct_field_type_compatible(actual, expected) {
                    return Err(mir_error(
                        *span,
                        "MIR struct.new argument has the wrong type",
                    ));
                }
            }
            if !is_struct_reference(
                value_type(function, *destination)
                    .ok_or_else(|| mir_error(*span, "MIR struct.new result has no value type"))?,
                *type_index,
                defined,
            ) {
                return Err(mir_error(
                    *span,
                    "MIR struct.new result must be a reference",
                ));
            }
        }
        Instruction::StructGet {
            destination,
            type_index,
            field,
            value,
            span,
        } => {
            let Some(CompositeType::Struct(fields)) = composite_at(defined, *type_index) else {
                return Err(mir_error(*span, "MIR struct.get type is not a struct"));
            };
            let Some(field) = fields.get(*field as usize) else {
                return Err(mir_error(*span, "MIR struct.get field is out of range"));
            };
            if !is_struct_reference(
                require_value(definitions, *value, *span)?,
                *type_index,
                defined,
            ) {
                return Err(mir_error(
                    *span,
                    "MIR struct.get operand must be a reference",
                ));
            }
            let expected = storage_value_type(&field.storage)
                .ok_or_else(|| mir_error(*span, "MIR struct field storage is not representable"))?;
            if !value_type(function, *destination)
                .is_some_and(|actual| struct_field_type_compatible(actual, expected))
            {
                return Err(mir_error(*span, "MIR struct.get result has the wrong type"));
            }
        }
        Instruction::StructSet {
            type_index,
            field,
            value,
            new_value,
            span,
        } => {
            let Some(CompositeType::Struct(fields)) = composite_at(defined, *type_index) else {
                return Err(mir_error(*span, "MIR struct.set type is not a struct"));
            };
            let Some(field) = fields.get(*field as usize) else {
                return Err(mir_error(*span, "MIR struct.set field is out of range"));
            };
            if !field.mutable {
                return Err(mir_error(
                    *span,
                    "MIR struct.set targets an immutable field",
                ));
            }
            if !is_struct_reference(
                require_value(definitions, *value, *span)?,
                *type_index,
                defined,
            ) {
                return Err(mir_error(
                    *span,
                    "MIR struct.set operand must be a reference",
                ));
            }
            let expected = storage_value_type(&field.storage)
                .ok_or_else(|| mir_error(*span, "MIR struct field storage is not representable"))?;
            if !struct_field_type_compatible(
                require_value(definitions, *new_value, *span)?,
                expected,
            ) {
                return Err(mir_error(*span, "MIR struct.set value has the wrong type"));
            }
        }
        Instruction::ArrayNew {
            destination,
            type_index,
            elements,
            span,
        } => {
            let Some(CompositeType::Array(element)) = composite_at(defined, *type_index) else {
                return Err(mir_error(*span, "MIR array.new type is not an array"));
            };
            let expected = storage_value_type(&element.storage).ok_or_else(|| {
                mir_error(*span, "MIR array element storage is not representable")
            })?;
            for element in elements {
                if !value_type_assignable(require_value(definitions, *element, *span)?, expected) {
                    return Err(mir_error(*span, "MIR array.new element has the wrong type"));
                }
            }
            if !is_array_reference(
                value_type(function, *destination)
                    .ok_or_else(|| mir_error(*span, "MIR array.new result has no value type"))?,
                *type_index,
                defined,
            ) {
                return Err(mir_error(*span, "MIR array.new result must be a reference"));
            }
        }
        Instruction::ArrayNewDefault { .. } => {
            arrays::verify_array_new_default(function, instruction, definitions, defined)?;
        }
        Instruction::ArrayGet {
            destination,
            type_index,
            value,
            index,
            span,
        } => {
            let Some(CompositeType::Array(element)) = composite_at(defined, *type_index) else {
                return Err(mir_error(*span, "MIR array.get type is not an array"));
            };
            if !is_array_reference(
                require_value(definitions, *value, *span)?,
                *type_index,
                defined,
            ) {
                return Err(mir_error(
                    *span,
                    "MIR array.get operand must be a reference",
                ));
            }
            if require_value(definitions, *index, *span)? != ValueType::I32 {
                return Err(mir_error(*span, "MIR array.get index must be i32"));
            }
            let expected = storage_value_type(&element.storage).ok_or_else(|| {
                mir_error(*span, "MIR array element storage is not representable")
            })?;
            if !value_type(function, *destination)
                .is_some_and(|destination_type| value_type_assignable(expected, destination_type))
            {
                return Err(mir_error(*span, "MIR array.get result has the wrong type"));
            }
        }
        Instruction::ArrayClone {
            destination,
            type_index,
            value,
            span,
        } => arrays::verify_clone(
            function,
            *destination,
            *type_index,
            *value,
            *span,
            definitions,
            defined,
        )?,
        Instruction::ArraySet {
            type_index,
            value,
            index,
            new_value,
            span,
        } => {
            let Some(CompositeType::Array(element)) = composite_at(defined, *type_index) else {
                return Err(mir_error(*span, "MIR array.set type is not an array"));
            };
            if !element.mutable {
                return Err(mir_error(*span, "MIR array.set targets an immutable array"));
            }
            if !is_array_reference(
                require_value(definitions, *value, *span)?,
                *type_index,
                defined,
            ) {
                return Err(mir_error(
                    *span,
                    "MIR array.set operand must be a reference",
                ));
            }
            if require_value(definitions, *index, *span)? != ValueType::I32 {
                return Err(mir_error(*span, "MIR array.set index must be i32"));
            }
            let expected = storage_value_type(&element.storage).ok_or_else(|| {
                mir_error(*span, "MIR array element storage is not representable")
            })?;
            if !value_type_assignable(require_value(definitions, *new_value, *span)?, expected) {
                return Err(mir_error(*span, "MIR array.set value has the wrong type"));
            }
        }
        Instruction::ArrayLen {
            destination,
            value,
            span,
        } => arrays::verify_len(function, *destination, *value, *span, definitions, defined)?,
        Instruction::ListCopy { .. } => {
            lists::verify_list_copy(function, instruction, definitions, defined)?
        }
        Instruction::ListCopyRecord { .. } => {
            lists::verify_list_copy_record(function, instruction, definitions, defined)?
        }
        Instruction::ListCopyFlags { .. } => {
            lists::verify_list_copy_flags(function, instruction, definitions, defined)?
        }
        Instruction::Load { .. }
        | Instruction::Load8U { .. }
        | Instruction::Store { .. }
        | Instruction::Store8 { .. }
        | Instruction::Store16 { .. }
        | Instruction::StoreI64 { .. }
        | Instruction::StoreF32 { .. }
        | Instruction::StoreF64 { .. }
        | Instruction::WrapI64 { .. }
        | Instruction::WidenI64 { .. } => {
            memory::verify_memory_instruction(function, instruction, definitions)?;
        }
    }
    Ok(())
}
