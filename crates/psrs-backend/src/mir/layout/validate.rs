//! Validation for selected representation and signature handles.

use super::LayoutError;
use crate::cc::{
    RefShape as CcRefShape, ReprId, Representation, RepresentationTable, SignatureId,
    ValueShape as CcValueShape,
};

pub(super) fn validate_selected(
    table: &RepresentationTable,
    repr_ids: &[ReprId],
    signature_ids: &[SignatureId],
) -> Result<(), LayoutError> {
    for id in repr_ids {
        let representation = table
            .representation(*id)
            .ok_or(LayoutError::UnknownRepresentation)?;
        validate_representation(representation, table)?;
    }
    for id in signature_ids {
        let signature = table.signature(*id).ok_or(LayoutError::UnknownSignature)?;
        for parameter in &signature.parameters {
            validate_value_shape(parameter, table)?;
        }
        validate_value_shape(&signature.result, table)?;
    }
    Ok(())
}

fn validate_representation(
    representation: &Representation,
    table: &RepresentationTable,
) -> Result<(), LayoutError> {
    match representation {
        Representation::Box { value } | Representation::Array { element: value } => {
            validate_value_shape(value, table)?;
        }
        Representation::Product { fields } => {
            for field in fields {
                validate_value_shape(field, table)?;
            }
        }
        Representation::Variant { cases } => {
            for case in cases {
                for field in &case.fields {
                    validate_value_shape(field, table)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_value_shape(
    value: &CcValueShape,
    table: &RepresentationTable,
) -> Result<(), LayoutError> {
    let CcValueShape::Reference(reference) = value else {
        return Ok(());
    };
    match reference.heap {
        CcRefShape::Repr(id) if table.representation(id).is_none() => {
            Err(LayoutError::UnknownRepresentation)
        }
        CcRefShape::Closure(id) if table.signature(id).is_none() => {
            Err(LayoutError::UnknownSignature)
        }
        _ => Ok(()),
    }
}
