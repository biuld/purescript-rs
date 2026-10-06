//! Reachability of concrete ABI projections and their storage conversion plans.
use super::*;
use crate::cc::{
    Field, GuestLayout,
    payload::{PayloadPlanner, StoragePayloadPlanner},
};
use psrs_span::TextRange;

/// Adds every representation the instance-aware projection names. The concrete
/// `value` node of an erased parameter field references a nested representation
/// the abstract signature does not reach, so the projection is what keeps a
/// nested `option<record>` payload's struct and array types planned.
pub(super) fn add_projection(
    layout: &GuestLayout,
    table: &RepresentationTable,
    representations: &mut HashSet<ReprId>,
    signatures: &mut HashSet<SignatureId>,
    representation_work: &mut Vec<ReprId>,
    signature_work: &mut Vec<SignatureId>,
) -> Result<(), LayoutError> {
    match layout {
        GuestLayout::Scalar { shape } | GuestLayout::Boxed { shape } => add_value(
            shape,
            representations,
            signatures,
            representation_work,
            signature_work,
        ),
        GuestLayout::Product { repr, fields, .. } => {
            add_representation(*repr, representations, representation_work);
            for field in fields {
                add_field(
                    field,
                    table,
                    representations,
                    signatures,
                    representation_work,
                    signature_work,
                )?;
            }
        }
        GuestLayout::Variant { repr, cases } => {
            add_representation(*repr, representations, representation_work);
            for case in cases {
                for field in &case.fields {
                    add_field(
                        field,
                        table,
                        representations,
                        signatures,
                        representation_work,
                        signature_work,
                    )?;
                }
            }
        }
        GuestLayout::Array { repr, element } => {
            add_representation(*repr, representations, representation_work);
            add_field(
                element,
                table,
                representations,
                signatures,
                representation_work,
                signature_work,
            )?;
        }
    }
    Ok(())
}

fn add_field(
    field: &Field,
    table: &RepresentationTable,
    representations: &mut HashSet<ReprId>,
    signatures: &mut HashSet<SignatureId>,
    representation_work: &mut Vec<ReprId>,
    signature_work: &mut Vec<SignatureId>,
) -> Result<(), LayoutError> {
    add_projection(
        &field.value,
        table,
        representations,
        signatures,
        representation_work,
        signature_work,
    )?;
    if matches!(field.stored, ValueShape::Reference(reference) if reference.heap == RefShape::Erased)
        && matches!(
            field.value,
            GuestLayout::Array { .. } | GuestLayout::Product { .. }
        )
    {
        let plan = StoragePayloadPlanner(table)
            .erase_payload(field.value.shape(), TextRange::new(0, 0))
            .map_err(|errors| LayoutError::InvalidPayloadConversion(errors[0].message.clone()))?;
        assignments::add_conversion(
            &plan,
            &mut HashSet::new(),
            representations,
            representation_work,
        );
    }
    Ok(())
}
