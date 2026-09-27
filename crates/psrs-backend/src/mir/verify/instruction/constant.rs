//! Constant instruction checks for MIR verification.

use super::super::util::{mir_error, value_type};
use crate::BackendError;
use crate::mir::{Function, Instruction, ValueId, ValueType};
use std::collections::HashMap;

pub(super) fn verify_constant(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
) -> Result<(), Vec<BackendError>> {
    let _ = definitions;
    match instruction {
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
        _ => unreachable!("constant verifier received another instruction"),
    }
    Ok(())
}
