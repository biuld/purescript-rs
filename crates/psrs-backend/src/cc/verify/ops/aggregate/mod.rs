use super::super::helpers::{assignment_error, repr_shape};
use crate::BackendError;
use crate::cc::{
    AggregateConvert, Assignment, BoxKind, RecoveryEvidence, RefShape, Reference, Representation,
    RepresentationTable, ValueConversion, ValueShape,
};

pub(super) fn verify_conversion(
    assignment: &Assignment,
    conversion: &AggregateConvert,
    source: ValueShape,
    destination: ValueShape,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    if conversion.source != source || conversion.destination != destination {
        return Err(assignment_error(
            assignment,
            "aggregate conversion endpoints disagree with their typed values",
        ));
    }
    let result = verify_plan(assignment, &conversion.plan, source, table)?;
    if result != destination {
        return Err(assignment_error(
            assignment,
            "aggregate conversion plan produces the wrong destination shape",
        ));
    }
    Ok(())
}

fn verify_plan(
    assignment: &Assignment,
    plan: &ValueConversion,
    source: ValueShape,
    table: &RepresentationTable,
) -> Result<ValueShape, Vec<BackendError>> {
    match plan {
        ValueConversion::Identity => Ok(source),
        ValueConversion::BoxScalar {
            kind,
            representation,
        } => {
            let Some(Representation::Box { value }) = table.representation(*representation) else {
                return Err(assignment_error(
                    assignment,
                    "scalar box plan has no box layout",
                ));
            };
            let valid = match kind {
                BoxKind::Integer => {
                    matches!(source, ValueShape::Integer | ValueShape::Boolean)
                        && *value == ValueShape::Integer
                }
                BoxKind::Number => source == ValueShape::Number && *value == ValueShape::Number,
            };
            if !valid {
                return Err(assignment_error(
                    assignment,
                    "scalar box plan has incompatible shapes",
                ));
            }
            Ok(repr_shape(*representation))
        }
        ValueConversion::UnboxScalar {
            kind,
            representation,
            destination,
        } => {
            let Some(Representation::Box { value }) = table.representation(*representation) else {
                return Err(assignment_error(
                    assignment,
                    "scalar unbox plan has no box layout",
                ));
            };
            if source != erased_shape() {
                return Err(assignment_error(
                    assignment,
                    "scalar unbox input must be erased",
                ));
            }
            match kind {
                BoxKind::Integer
                    if *value == ValueShape::Integer
                        && matches!(destination, ValueShape::Integer | ValueShape::Boolean) =>
                {
                    Ok(*destination)
                }
                BoxKind::Number
                    if *value == ValueShape::Number && *destination == ValueShape::Number =>
                {
                    Ok(*destination)
                }
                _ => Err(assignment_error(
                    assignment,
                    "scalar unbox plan has incompatible shapes",
                )),
            }
        }
        ValueConversion::EraseReference => {
            if !matches!(source, ValueShape::Reference(reference) if reference != erased_reference())
            {
                return Err(assignment_error(
                    assignment,
                    "reference erasure requires a typed reference",
                ));
            }
            Ok(erased_shape())
        }
        ValueConversion::RecoverReference {
            destination,
            evidence,
        } => {
            if source != erased_shape() || !matches!(destination, ValueShape::Reference(_)) {
                return Err(assignment_error(
                    assignment,
                    "reference recovery has incompatible shapes",
                ));
            }
            if let RecoveryEvidence::ErasedVariantField {
                variant,
                tag,
                field,
                template,
            } = evidence
            {
                let Some(Representation::Variant { cases }) = table.representation(*variant) else {
                    return Err(assignment_error(
                        assignment,
                        "recovery evidence has no variant layout",
                    ));
                };
                let valid = cases
                    .iter()
                    .find(|case| case.tag == *tag)
                    .and_then(|case| case.fields.get(*field as usize))
                    == Some(&erased_shape())
                    && template == destination;
                if !valid {
                    return Err(assignment_error(
                        assignment,
                        "erased variant recovery evidence is invalid",
                    ));
                }
            }
            Ok(*destination)
        }
        ValueConversion::Sequence(plans) => {
            let mut shape = source;
            for plan in plans {
                shape = verify_plan(assignment, plan, shape, table)?;
            }
            Ok(shape)
        }
        ValueConversion::ArrayMap {
            source: source_id,
            target,
            element,
        } => {
            let Some(Representation::Array {
                element: source_element,
            }) = table.representation(*source_id)
            else {
                return Err(assignment_error(
                    assignment,
                    "array map has no source array layout",
                ));
            };
            let Some(Representation::Array {
                element: target_element,
            }) = table.representation(*target)
            else {
                return Err(assignment_error(
                    assignment,
                    "array map has no target array layout",
                ));
            };
            if source != repr_shape(*source_id)
                || verify_plan(assignment, element, *source_element, table)? != *target_element
            {
                return Err(assignment_error(
                    assignment,
                    "array map element conversion is invalid",
                ));
            }
            Ok(repr_shape(*target))
        }
        ValueConversion::ProductMap {
            source: source_id,
            target,
            labels,
            fields,
        } => {
            let Some(Representation::Product {
                fields: source_fields,
            }) = table.representation(*source_id)
            else {
                return Err(assignment_error(
                    assignment,
                    "product map has no source product layout",
                ));
            };
            let Some(Representation::Product {
                fields: target_fields,
            }) = table.representation(*target)
            else {
                return Err(assignment_error(
                    assignment,
                    "product map has no target product layout",
                ));
            };
            if source != repr_shape(*source_id)
                || labels.len() != source_fields.len()
                || labels.len() != target_fields.len()
                || fields.len() != labels.len()
                || table.product_labels(*source_id) != Some(labels.as_slice())
                || table.product_labels(*target) != Some(labels.as_slice())
            {
                return Err(assignment_error(
                    assignment,
                    "product map labels or arity are invalid",
                ));
            }
            for ((plan, source_field), target_field) in
                fields.iter().zip(source_fields).zip(target_fields)
            {
                if verify_plan(assignment, plan, *source_field, table)? != *target_field {
                    return Err(assignment_error(
                        assignment,
                        "product map field conversion is invalid",
                    ));
                }
            }
            Ok(repr_shape(*target))
        }
        ValueConversion::FunctionAdapter {
            source: from,
            target,
        } => {
            let from_shape = closure_shape(*from);
            let target_shape = closure_shape(*target);
            if source != from_shape
                || table.signature(*from).is_none()
                || table.signature(*target).is_none()
            {
                return Err(assignment_error(
                    assignment,
                    "function adapter has incompatible signatures",
                ));
            }
            Ok(target_shape)
        }
    }
}

fn erased_shape() -> ValueShape {
    ValueShape::Reference(erased_reference())
}

fn erased_reference() -> Reference {
    Reference {
        nullable: false,
        heap: RefShape::Erased,
    }
}

fn closure_shape(signature: crate::cc::SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}

#[cfg(test)]
mod tests;
