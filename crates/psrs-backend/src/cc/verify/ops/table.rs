use super::super::helpers::{table_error, verify_value_shape};
use crate::BackendError;
use crate::cc::{Representation, RepresentationTable};
use psrs_span::TextRange;
use std::collections::HashSet;

pub(crate) fn verify_table(
    table: &RepresentationTable,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
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
