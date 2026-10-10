//! Lowers a primitive foreign import whose source arity differs from the WIT
//! parameter count. Each `Int`, `Boolean`, `Char`, `Number`, or handle-as-`Int`
//! is one core value. `String` reuses the `(pointer, length)` lowering.

use super::{PendingFree, WitCallLowerer, lower_parameter, unsupported_parameter};
use crate::BackendError;
use crate::abi::WasiImport;
use crate::abi::canonical::{CanonicalType, FlatLeaf, flat_leaves_of};
use crate::cc::ValueShape;
use crate::mir::BlockId;
use crate::types::ValueId;
use psrs_span::TextRange;

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_primitive_parameters<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    guest_parameters: &[ValueShape],
    arguments: &[ValueId],
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    entry: BlockId,
    span: TextRange,
) -> Result<BlockId, Vec<BackendError>> {
    let leaves = flat_leaves_of(&import.params);
    let mut index = 0;
    let mut current = entry;
    for (argument, shape) in arguments.iter().zip(guest_parameters) {
        let ty = slot_type(import, &leaves, shape, &mut index, span)?;
        current = lower_parameter(lowerer, *argument, None, &ty, flat, frees, current, span)?;
    }
    let direct = import
        .parameters
        .len()
        .saturating_sub(usize::from(import.abi.retptr));
    if index != leaves.len() || index != direct {
        return Err(unsupported_parameter(span));
    }
    Ok(current)
}

fn slot_type(
    import: &WasiImport,
    leaves: &[FlatLeaf],
    shape: &ValueShape,
    index: &mut usize,
    span: TextRange,
) -> Result<CanonicalType, Vec<BackendError>> {
    let leaf = leaves
        .get(*index)
        .copied()
        .ok_or_else(|| unsupported_parameter(span))?;
    let ty = match shape {
        ValueShape::State => return Err(unsupported_parameter(span)),
        ValueShape::Integer => match leaf {
            FlatLeaf::Int32 => CanonicalType::Int {
                width: 32,
                signed: false,
            },
            FlatLeaf::Int64 { signed } => CanonicalType::Int { width: 64, signed },
            FlatLeaf::Handle => import
                .handle_at_flat_index(*index)
                .cloned()
                .ok_or_else(|| unsupported_parameter(span))?,
            _ => return Err(unsupported_parameter(span)),
        },
        ValueShape::Boolean => match leaf {
            FlatLeaf::Boolean => CanonicalType::Bool,
            _ => return Err(unsupported_parameter(span)),
        },
        ValueShape::Number => match leaf {
            FlatLeaf::Float64 => CanonicalType::Float { width: 64 },
            FlatLeaf::Float32 => CanonicalType::Float { width: 32 },
            _ => return Err(unsupported_parameter(span)),
        },
        ValueShape::String => {
            if let (FlatLeaf::Pointer, Some(FlatLeaf::Length)) = (leaf, leaves.get(*index + 1)) {
                *index += 2;
                return Ok(CanonicalType::String);
            }
            return Err(unsupported_parameter(span));
        }
        ValueShape::Reference(_) => return Err(unsupported_parameter(span)),
    };
    *index += 1;
    Ok(ty)
}
