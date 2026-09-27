//! Canonical ABI lowering for mapped `option`/`result`/`variant` aggregates.
//!
//! A mapped aggregate carries a canonical discriminant followed by the joined
//! payload slots. As a parameter the lowering branches on the source tag and
//! flattens the selected payload; as a result it branches on the return-area
//! discriminant and rebuilds the source value.

mod parameter;
mod result;

pub(super) use parameter::lower_variant_parameter;
pub(super) use result::lower_variant_result;

use super::{BlockId, WitCallLowerer, free_buffer};
use crate::BackendError;
use crate::abi::{self, WasiParamKind};
use crate::cc::{RefShape, Reference, ValueShape};
use crate::mir::UnaryOp;
use crate::mir::instruction::Instruction;
use crate::types::{HeapType, MemoryId, RefType, ValueId, ValueType};
use psrs_span::TextRange;

pub(super) fn unsupported(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        "this WIT aggregate payload shape is not supported by the source ABI yet",
    )]
}

/// The MIR type of a stored erased variant field.
pub(super) fn erased_type() -> ValueType {
    ValueType::Ref(erased_reference())
}

pub(super) fn erased_reference() -> RefType {
    RefType {
        nullable: false,
        heap: HeapType::Eq,
    }
}

/// The abstract aggregate reference used as a `struct.new` destination before a
/// cast to the concrete variant supertype.
pub(super) fn aggregate_type() -> ValueType {
    ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Struct,
    })
}

/// The canonical flat value types produced by lowering `kind`, including its
/// discriminant for a mapped aggregate.
pub(super) fn flat_types(kind: &WasiParamKind) -> Option<Vec<ValueType>> {
    Some(match kind {
        WasiParamKind::Integer32
        | WasiParamKind::IntegerNarrow { .. }
        | WasiParamKind::Boolean
        | WasiParamKind::Char
        | WasiParamKind::Enum { .. }
        | WasiParamKind::Handle(_) => vec![ValueType::I32],
        WasiParamKind::Scalar64 { .. } => vec![ValueType::I64],
        WasiParamKind::Float32 => vec![ValueType::F32],
        WasiParamKind::Float64 => vec![ValueType::F64],
        WasiParamKind::List | WasiParamKind::ValueList { .. } => {
            vec![ValueType::I32, ValueType::I32]
        }
        WasiParamKind::Flags { names } => vec![ValueType::I32; names.len().div_ceil(32)],
        WasiParamKind::Record { fields } => {
            let mut types = Vec::new();
            for field in fields {
                types.extend(flat_types(&field.kind)?);
            }
            types
        }
        WasiParamKind::Option { payload } => {
            let mut types = vec![ValueType::I32];
            types.extend(flat_types(payload)?);
            types
        }
        WasiParamKind::Result { ok, err } => {
            let mut types = vec![ValueType::I32];
            types.extend(join_types(flat_types(ok)?, flat_types(err)?)?);
            types
        }
        WasiParamKind::Variant { cases } => {
            let mut types = vec![ValueType::I32];
            types.extend(joined_cases(
                cases.iter().filter_map(|case| case.kind.as_deref()),
            )?);
            types
        }
        WasiParamKind::Unsupported => return None,
    })
}

/// The tag-ordered payload kinds of a mapped aggregate.
pub(super) fn payload_cases(kind: &WasiParamKind) -> Option<Vec<Option<&WasiParamKind>>> {
    Some(match kind {
        WasiParamKind::Option { payload } => vec![None, Some(payload)],
        WasiParamKind::Result { ok, err } => vec![Some(ok), Some(err)],
        WasiParamKind::Variant { cases } => cases.iter().map(|case| case.kind.as_deref()).collect(),
        _ => return None,
    })
}

/// The joined payload slot types of a mapped aggregate, excluding its tag.
pub(super) fn joined_payload_types(kind: &WasiParamKind) -> Option<Vec<ValueType>> {
    joined_cases(payload_cases(kind)?.into_iter().flatten())
}

fn joined_cases<'a>(cases: impl IntoIterator<Item = &'a WasiParamKind>) -> Option<Vec<ValueType>> {
    let mut joined: Option<Vec<ValueType>> = None;
    for case in cases {
        let types = flat_types(case)?;
        joined = Some(match joined {
            None => types,
            Some(joined) => join_types(joined, types)?,
        });
    }
    Some(joined.unwrap_or_default())
}

fn join_types(left: Vec<ValueType>, right: Vec<ValueType>) -> Option<Vec<ValueType>> {
    let length = left.len().max(right.len());
    let mut joined = Vec::with_capacity(length);
    for index in 0..length {
        joined.push(match (left.get(index), right.get(index)) {
            (Some(left), Some(right)) => unify(*left, *right)?,
            (Some(value), None) | (None, Some(value)) => *value,
            (None, None) => unreachable!("index is below the maximum length"),
        });
    }
    Some(joined)
}

fn unify(left: ValueType, right: ValueType) -> Option<ValueType> {
    if left == right {
        Some(left)
    } else if matches!(
        (left, right),
        (ValueType::Boolean, ValueType::I32) | (ValueType::I32, ValueType::Boolean)
    ) {
        Some(ValueType::I32)
    } else {
        None
    }
}

/// The source value shape of a directly lowered payload kind.
pub(super) fn direct_shape(kind: &WasiParamKind) -> Option<ValueShape> {
    Some(match kind {
        WasiParamKind::Integer32
        | WasiParamKind::IntegerNarrow { .. }
        | WasiParamKind::Char
        | WasiParamKind::Enum { .. }
        | WasiParamKind::Handle(_) => ValueShape::Integer,
        WasiParamKind::Boolean => ValueShape::Boolean,
        WasiParamKind::List => ValueShape::String,
        _ => return None,
    })
}

/// The concrete variant representation handle of a source aggregate shape.
pub(super) fn shape_repr(shape: &ValueShape) -> Option<crate::cc::ReprId> {
    let ValueShape::Reference(Reference {
        heap: RefShape::Repr(repr),
        ..
    }) = shape
    else {
        return None;
    };
    Some(*repr)
}

/// Boxes an `i32` scalar and erases the box into the stored variant field.
pub(super) fn box_scalar<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let boxed = lowerer
        .wit_boxed_integer()
        .ok_or_else(|| unsupported(span))?;
    let boxed_type = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(boxed),
    });
    let boxed_value = lowerer.fresh_wit_value(boxed_type);
    lowerer.append_wit_instruction(
        block,
        Instruction::StructNew {
            destination: boxed_value,
            type_index: boxed,
            arguments: vec![value],
            span,
        },
        span,
    )?;
    erase_reference(lowerer, boxed_value, block, span)
}

/// Casts a reference to the erased field type.
pub(super) fn erase_reference<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let erased = lowerer.fresh_wit_value(erased_type());
    lowerer.append_wit_instruction(
        block,
        Instruction::RefCast {
            destination: erased,
            value,
            reference: erased_reference(),
            span,
        },
        span,
    )?;
    Ok(erased)
}

/// Casts an erased field to the concrete GC string.
pub(super) fn recover_string<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let string = lowerer
        .wit_string_index()
        .ok_or_else(|| unsupported(span))?;
    let reference = RefType {
        nullable: false,
        heap: HeapType::Index(string),
    };
    let recovered = lowerer.fresh_wit_value(ValueType::Ref(reference));
    lowerer.append_wit_instruction(
        block,
        Instruction::RefCast {
            destination: recovered,
            value,
            reference,
            span,
        },
        span,
    )?;
    Ok(recovered)
}

/// Unboxes an erased variant field to its source value for a parameter.
pub(super) fn recover_payload<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    kind: &WasiParamKind,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    match kind {
        WasiParamKind::Integer32
        | WasiParamKind::IntegerNarrow { .. }
        | WasiParamKind::Char
        | WasiParamKind::Enum { .. }
        | WasiParamKind::Handle(_) => unbox_scalar(lowerer, value, false, block, span),
        WasiParamKind::Boolean => unbox_scalar(lowerer, value, true, block, span),
        WasiParamKind::List => recover_string(lowerer, value, block, span),
        _ => Err(unsupported(span)),
    }
}

fn unbox_scalar<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    boolean: bool,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let boxed = lowerer
        .wit_boxed_integer()
        .ok_or_else(|| unsupported(span))?;
    let reference = RefType {
        nullable: false,
        heap: HeapType::Index(boxed),
    };
    let boxed_value = lowerer.fresh_wit_value(ValueType::Ref(reference));
    lowerer.append_wit_instruction(
        block,
        Instruction::RefCast {
            destination: boxed_value,
            value,
            reference,
            span,
        },
        span,
    )?;
    let integer = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        block,
        Instruction::StructGet {
            destination: integer,
            type_index: boxed,
            field: 0,
            value: boxed_value,
            span,
        },
        span,
    )?;
    if boolean {
        let result = lowerer.fresh_wit_value(ValueType::Boolean);
        lowerer.append_wit_instruction(
            block,
            Instruction::UnaryPrimitive {
                destination: result,
                op: UnaryOp::I32ToBool,
                value: integer,
                span,
            },
            span,
        )?;
        Ok(result)
    } else {
        Ok(integer)
    }
}

/// Reads a directly lowered payload from the canonical return area and erases
/// it into the stored variant field.
pub(super) fn read_payload<L: WitCallLowerer>(
    lowerer: &mut L,
    kind: &WasiParamKind,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    if matches!(kind, WasiParamKind::List) {
        let pointer = load(lowerer, address, offset, block, span)?;
        let length = load(lowerer, address, offset + 4, block, span)?;
        let string = lowerer
            .wit_string_index()
            .ok_or_else(|| unsupported(span))?;
        let reference = RefType {
            nullable: false,
            heap: HeapType::Index(string),
        };
        let value = lowerer.fresh_wit_value(ValueType::Ref(reference));
        lowerer.append_wit_instruction(
            block,
            Instruction::Call {
                destination: value,
                function: abi::BYTES_TO_STRING_SYMBOL,
                arguments: vec![pointer, length],
                span,
            },
            span,
        )?;
        free_buffer(lowerer, pointer, length, 1, block, span)?;
        return erase_reference(lowerer, value, block, span);
    }
    // A resource handle nested in a result would need ownership tracking the
    // result path does not model, so it is rejected rather than leaked.
    if !matches!(
        kind,
        WasiParamKind::Integer32
            | WasiParamKind::IntegerNarrow { .. }
            | WasiParamKind::Boolean
            | WasiParamKind::Char
            | WasiParamKind::Enum { .. }
    ) {
        return Err(unsupported(span));
    }
    let layout = abi::layout::parameter_layout(kind).ok_or_else(|| unsupported(span))?;
    let slot = layout.slots.first().ok_or_else(|| unsupported(span))?;
    let scalar = match slot.kind {
        abi::layout::SlotKind::Byte => load8(lowerer, address, offset + slot.offset, block, span)?,
        abi::layout::SlotKind::Word => load(lowerer, address, offset + slot.offset, block, span)?,
        _ => return Err(unsupported(span)),
    };
    box_scalar(lowerer, scalar, block, span)
}

fn load<L: WitCallLowerer>(
    lowerer: &mut L,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let destination = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        block,
        Instruction::Load {
            destination,
            address,
            memory: MemoryId(0),
            offset,
            span,
        },
        span,
    )?;
    Ok(destination)
}

fn load8<L: WitCallLowerer>(
    lowerer: &mut L,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let destination = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        block,
        Instruction::Load8U {
            destination,
            address,
            memory: MemoryId(0),
            offset,
            span,
        },
        span,
    )?;
    Ok(destination)
}
