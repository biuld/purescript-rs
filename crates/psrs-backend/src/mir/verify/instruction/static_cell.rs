//! Static cells always use the common mutable erased-element array.
use super::*;
use crate::types::StorageType;
pub(super) fn verify(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::StaticCell {
        destination,
        type_index,
        initial,
        span,
        ..
    } = instruction
    else {
        unreachable!("static cell verifier")
    };
    let erased = RefType {
        nullable: true,
        heap: HeapType::Eq,
    };
    if !matches!(composite_at(defined, *type_index), Some(CompositeType::Array(field))
        if field.mutable && field.storage == StorageType::Ref(erased))
    {
        return Err(mir_error(
            *span,
            "static cell requires mutable erased-element array storage",
        ));
    }
    if !value_type_assignable(
        require_value(definitions, *initial, *span)?,
        ValueType::Ref(erased),
    ) || value_type(function, *destination)
        != Some(ValueType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(*type_index),
        }))
    {
        return Err(mir_error(
            *span,
            "static cell initializer or result has incompatible storage",
        ));
    }
    Ok(())
}
