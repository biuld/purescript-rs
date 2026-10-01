//! Canonical ABI lowering for mapped `option`/`result`/`variant` aggregates.
//!
//! A mapped aggregate carries a canonical discriminant followed by the joined
//! payload slots. As a parameter the lowering branches on the source tag and
//! flattens the selected payload; as a result it branches on the return-area
//! discriminant and rebuilds the source value.

mod collections;
mod decode;
mod memory;
mod parameter;
mod result;

pub(super) use parameter::lower_variant_parameter;
pub(super) use result::lower_record_result;
pub(super) use result::lower_variant_result;
pub(super) use result::retptr_buffer;

use super::{BlockId, PendingFree, WitCallLowerer, free_buffer};
use crate::BackendError;
use crate::abi;
use crate::abi::canonical::{
    CanonicalType, canonical_case_for_tag, payload_cases, source_tag_for_case, swaps_case_tags,
};
use crate::cc::{GuestLayout, RefShape, ValueShape};
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

/// The canonical flat value types produced by lowering `ty`, including its
/// discriminant for a mapped aggregate.
pub(super) fn flat_types(ty: &CanonicalType) -> Option<Vec<ValueType>> {
    Some(match ty {
        CanonicalType::Bool => vec![ValueType::Boolean],
        CanonicalType::Int { width: 64, .. } => vec![ValueType::I64],
        CanonicalType::Int { .. }
        | CanonicalType::Char
        | CanonicalType::Enum(_)
        | CanonicalType::Handle { .. } => vec![ValueType::I32],
        CanonicalType::Float { width: 32 } => vec![ValueType::F32],
        CanonicalType::Float { .. } => vec![ValueType::F64],
        CanonicalType::String | CanonicalType::List(_) => vec![ValueType::I32, ValueType::I32],
        CanonicalType::FixedList { element, .. } if element.is_byte() => {
            vec![ValueType::I32, ValueType::I32]
        }
        CanonicalType::FixedList { element, length } => {
            let mut types = Vec::new();
            for _ in 0..*length {
                types.extend(flat_types(element)?);
            }
            types
        }
        CanonicalType::Flags(names) => vec![ValueType::I32; names.len().div_ceil(32)],
        CanonicalType::Record(fields) => {
            let mut types = Vec::new();
            for field in fields {
                types.extend(flat_types(&field.ty)?);
            }
            types
        }
        CanonicalType::Option(payload) => {
            let mut types = vec![ValueType::I32];
            types.extend(flat_types(payload)?);
            types
        }
        CanonicalType::Result { ok, err } => {
            let mut types = vec![ValueType::I32];
            let ok = match ok.as_deref() {
                Some(ok) => flat_types(ok)?,
                None => Vec::new(),
            };
            let err = match err.as_deref() {
                Some(err) => flat_types(err)?,
                None => Vec::new(),
            };
            types.extend(join_types(ok, err)?);
            types
        }
        CanonicalType::Variant(cases) => {
            let mut types = vec![ValueType::I32];
            types.extend(joined_cases(
                cases.iter().filter_map(|case| case.payload.as_deref()),
            )?);
            types
        }
    })
}

/// The joined payload slot types of a mapped aggregate, excluding its tag.
pub(super) fn joined_payload_types(ty: &CanonicalType) -> Option<Vec<ValueType>> {
    joined_cases(payload_cases(ty)?.into_iter().flatten())
}

fn joined_cases<'a>(cases: impl IntoIterator<Item = &'a CanonicalType>) -> Option<Vec<ValueType>> {
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

/// The source value shape of a directly lowered payload type.
pub(super) fn direct_shape(ty: &CanonicalType) -> Option<ValueShape> {
    Some(match ty {
        CanonicalType::Int { .. }
        | CanonicalType::Char
        | CanonicalType::Enum(_)
        | CanonicalType::Handle { .. } => ValueShape::Integer,
        CanonicalType::Bool => ValueShape::Boolean,
        CanonicalType::Float { .. } => ValueShape::Number,
        CanonicalType::String => ValueShape::String,
        CanonicalType::List(inner) if inner.is_byte() => ValueShape::String,
        CanonicalType::FixedList { element, .. } if element.is_byte() => ValueShape::String,
        _ => return None,
    })
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

/// Whether a projected field's storage slot is erased. The erased aggregate
/// protocol boxes or casts such a field.
pub(super) fn is_erased(shape: ValueShape) -> bool {
    match shape {
        ValueShape::Reference(reference) => !matches!(reference.heap, crate::cc::RefShape::Repr(_)),
        _ => false,
    }
}

/// The concrete MIR reference type of a source reference shape.
pub(super) fn reference_type<L: WitCallLowerer>(
    lowerer: &L,
    shape: &ValueShape,
) -> Option<RefType> {
    let ValueShape::Reference(reference) = shape else {
        return None;
    };
    Some(RefType {
        nullable: reference.nullable,
        heap: match reference.heap {
            RefShape::Repr(repr) => HeapType::Index(lowerer.wit_repr_index(repr)?),
            RefShape::Aggregate => HeapType::Struct,
            RefShape::Erased => HeapType::Eq,
            RefShape::Closure(_) => return None,
        },
    })
}

/// Casts an erased variant field to a concrete reference shape.
pub(super) fn cast_reference<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    shape: &ValueShape,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let reference = reference_type(lowerer, shape).ok_or_else(|| unsupported(span))?;
    let destination = lowerer.fresh_wit_value(ValueType::Ref(reference));
    lowerer.append_wit_instruction(
        block,
        Instruction::RefCast {
            destination,
            value,
            reference,
            span,
        },
        span,
    )?;
    Ok(destination)
}

/// Unboxes an erased variant field into the concrete source value for a
/// parameter. Returns the value and its concrete projection. `kind` is the
/// canonical payload, which recovers a direct scalar when the projection is the
/// storage fallback (`value` is an erased or representation placeholder).
pub(super) fn recover_payload<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    field: &crate::cc::Field,
    kind: &CanonicalType,
    block: BlockId,
    span: TextRange,
) -> Result<(ValueId, crate::cc::GuestLayout), Vec<BackendError>> {
    // A concrete storage slot already stores the source value.
    if !is_erased(field.stored) {
        return Ok((value, field.value.clone()));
    }
    // An erased slot holds a reference (scalar box or erased aggregate).
    // Recover the concrete source value from the projected node; when the node
    // is a storage placeholder, the canonical payload names the direct scalar.
    let concrete = if is_storage_placeholder(&field.value) {
        direct_shape(kind)
            .map(|shape| crate::cc::GuestLayout::Scalar { shape })
            .unwrap_or_else(|| field.value.clone())
    } else {
        field.value.clone()
    };
    let recovered = match &concrete {
        crate::cc::GuestLayout::Scalar { shape } => match shape {
            ValueShape::Integer => unbox_scalar(lowerer, value, false, block, span)?,
            ValueShape::Boolean => unbox_scalar(lowerer, value, true, block, span)?,
            ValueShape::Number => unbox_number(lowerer, value, block, span)?,
            ValueShape::String => recover_string(lowerer, value, block, span)?,
            ValueShape::Reference(_) => cast_reference(lowerer, value, shape, block, span)?,
        },
        other => cast_reference(lowerer, value, &other.shape(), block, span)?,
    };
    Ok((recovered, concrete))
}

/// Whether a projected node is the storage fallback rather than a concrete
/// scalar: a scalar whose shape is a reference (erased or representation
/// placeholder) still needs the canonical payload to recover its value.
fn is_storage_placeholder(node: &GuestLayout) -> bool {
    matches!(
        node,
        GuestLayout::Scalar {
            shape: ValueShape::Reference(_)
        }
    )
}

/// Boxes an `f64` and erases the box into the stored variant field.
pub(super) fn box_number<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let boxed = lowerer
        .wit_boxed_number()
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

fn unbox_number<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let boxed = lowerer
        .wit_boxed_number()
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
    let number = lowerer.fresh_wit_value(ValueType::F64);
    lowerer.append_wit_instruction(
        block,
        Instruction::StructGet {
            destination: number,
            type_index: boxed,
            field: 0,
            value: boxed_value,
            span,
        },
        span,
    )?;
    Ok(number)
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
