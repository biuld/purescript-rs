//! Lowers a primitive foreign import whose source arity differs from the WIT
//! parameter count. Each `Int`, `Boolean`, `Char`, `Number`, or handle-as-`Int`
//! is one core value. `String` reuses the `(pointer, length)` lowering.

use super::{PendingFree, WitCallLowerer, lower_parameter, unsupported_parameter};
use crate::BackendError;
use crate::abi::{self, FlatSlot, WasiImport};
use crate::mir::BlockId;
use crate::types::ValueId;
use psrs_span::TextRange;

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_primitive_parameters<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    source_signature: &abi::SourceSignature,
    arguments: &[ValueId],
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let mut index = 0;
    for (argument, source) in arguments.iter().zip(&source_signature.parameters) {
        let kind = slot_kind(import, source, &mut index, span)?;
        lower_parameter(
            lowerer, *argument, source, &kind, flat, frees, current, span,
        )?;
    }
    let direct = import
        .parameters
        .len()
        .saturating_sub(usize::from(import.retptr));
    if index != import.flat_slots.len() || index != direct {
        return Err(unsupported_parameter(span));
    }
    Ok(())
}

fn slot_kind(
    import: &WasiImport,
    source: &abi::SourceType,
    index: &mut usize,
    span: TextRange,
) -> Result<abi::WasiParamKind, Vec<BackendError>> {
    let kind = match source {
        abi::SourceType::Int => match import.flat_slots.get(*index) {
            Some(FlatSlot::Int32) => abi::WasiParamKind::Integer32,
            Some(FlatSlot::Int64 { signed }) => abi::WasiParamKind::Scalar64 { signed: *signed },
            Some(FlatSlot::Handle) => import
                .handle_at_flat_index(*index)
                .cloned()
                .map(abi::WasiParamKind::Handle)
                .ok_or_else(|| unsupported_parameter(span))?,
            _ => return Err(unsupported_parameter(span)),
        },
        abi::SourceType::Boolean => match import.flat_slots.get(*index) {
            Some(FlatSlot::Boolean) => abi::WasiParamKind::Boolean,
            _ => return Err(unsupported_parameter(span)),
        },
        abi::SourceType::Char => match import.flat_slots.get(*index) {
            Some(FlatSlot::Char) => abi::WasiParamKind::Char,
            _ => return Err(unsupported_parameter(span)),
        },
        abi::SourceType::Number => match import.flat_slots.get(*index) {
            Some(FlatSlot::Float64) => abi::WasiParamKind::Float64,
            Some(FlatSlot::Float32) => abi::WasiParamKind::Float32,
            _ => return Err(unsupported_parameter(span)),
        },
        abi::SourceType::String => {
            match (
                import.flat_slots.get(*index),
                import.flat_slots.get(*index + 1),
            ) {
                (Some(FlatSlot::Pointer), Some(FlatSlot::Length)) => {
                    *index += 2;
                    return Ok(abi::WasiParamKind::List);
                }
                _ => return Err(unsupported_parameter(span)),
            }
        }
        abi::SourceType::Unit
        | abi::SourceType::Enum { .. }
        | abi::SourceType::Record { .. }
        | abi::SourceType::Resource { .. } => return Err(unsupported_parameter(span)),
    };
    *index += 1;
    Ok(kind)
}
