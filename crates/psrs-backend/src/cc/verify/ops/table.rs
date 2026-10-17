use super::super::helpers::{table_error, verify_value_shape};
use crate::BackendError;
use crate::cc::{Representation, RepresentationTable};
use psrs_span::TextRange;
use std::collections::HashSet;

pub(crate) fn verify_table(
    table: &RepresentationTable,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    if let Err(message) = table.verify_record_storage() {
        return Err(table_error(span, message));
    }
    let mut roles = HashSet::new();
    let mut helpers = HashSet::new();
    for helper in table.control_helpers() {
        if !roles.insert(helper.role())
            || !helpers.insert(helper.function())
            || table.signature(helper.signature()).is_none()
        {
            return Err(table_error(
                span,
                "control helper contract has a duplicate identity or missing signature",
            ));
        }
    }
    if !table.control_helpers().is_empty()
        && crate::cc::ControlHelperRole::ALL
            .iter()
            .any(|role| !roles.contains(role))
    {
        return Err(table_error(
            span,
            "control machine is missing a required semantic helper",
        ));
    }
    if let Err(message) = table.verify_control_schema() {
        return Err(table_error(span, message));
    }
    if !table.control_helpers().is_empty() {
        let valid = table.control_resumption().is_some_and(|resume| {
            !helpers.contains(&resume.function())
                && table.signature(resume.signature()).is_some()
                && table.control_checkpoint_schema().is_some()
                && table
                    .control_helpers()
                    .iter()
                    .find(|helper| helper.role() == crate::cc::ControlHelperRole::Once)
                    .and_then(|helper| table.signature(helper.signature()))
                    .and_then(|signature| signature.parameters.first())
                    == Some(&crate::cc::ValueShape::Reference(crate::cc::Reference {
                        nullable: false,
                        heap: crate::cc::RefShape::Closure(resume.signature()),
                    }))
        });
        if !valid {
            return Err(table_error(
                span,
                "control machine has no valid raw resumption binding",
            ));
        }
    } else if table.control_resumption().is_some() {
        return Err(table_error(
            span,
            "raw resumption binding has no control helper schema",
        ));
    }
    for proof in table.control_contracts() {
        if proof.argument_identities().len() != proof.arguments().len()
            || std::iter::once(&proof.result_identity())
                .chain(proof.argument_identities())
                .any(|id| table.source_type_identities().get(id.0 as usize).is_none())
        {
            return Err(table_error(
                span,
                "control contract has no canonical source type identity",
            ));
        }
    }
    for (index, representation) in table.representations.iter().enumerate() {
        match representation {
            Representation::Box { value } | Representation::Array { element: value } => {
                verify_value_shape(value, table, span)?;
            }
            Representation::Product { fields } => {
                for field in fields {
                    verify_value_shape(field, table, span)?;
                }
                let invalid_labels = table
                    .product_labels(crate::cc::ReprId(index as u32))
                    .is_some_and(|labels| {
                        labels.len() != fields.len()
                            || labels.iter().collect::<HashSet<_>>().len() != labels.len()
                    });
                if invalid_labels {
                    return Err(table_error(
                        span,
                        "CC product labels must be unique and match the product arity",
                    ));
                }
            }
            Representation::Variant { cases } => {
                if cases.is_empty() {
                    return Err(table_error(span, "CC variant has no cases"));
                }
                let mut tags = HashSet::new();
                for case in cases {
                    if !tags.insert(case.tag) {
                        return Err(table_error(span, "CC variant tags must be unique"));
                    }
                    for field in &case.fields {
                        verify_value_shape(field, table, span)?;
                    }
                }
            }
        }
    }
    for id in table.product_labels.keys() {
        if !matches!(
            table.representation(*id),
            Some(Representation::Product { .. })
        ) {
            return Err(table_error(
                span,
                "CC product labels reference a non-product layout",
            ));
        }
    }
    for signature in &table.signatures {
        for parameter in &signature.parameters {
            verify_value_shape(parameter, table, span)?;
        }
        verify_value_shape(&signature.result, table, span)?;
    }
    Ok(())
}
