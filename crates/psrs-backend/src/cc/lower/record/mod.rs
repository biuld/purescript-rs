use super::super::layout::erased_field_recovery_family;
use super::super::{Assignment, AssignmentKind, Representation, ValueId, ValueShape};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::{Expr, Type};

#[cfg(test)]
mod tests;

impl FunctionLowerer<'_> {
    pub(super) fn lower_record(
        &mut self,
        expression: &Expr,
        fields: &[(String, Expr)],
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(representation) = self.record_types.get(&expression.ty).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "record expression has no representation requirement",
            )]);
        };
        let labels = match self.module.types.get(expression.ty.0 as usize) {
            Some(Type::Record(fields)) => fields
                .iter()
                .map(|(label, _)| label.clone())
                .collect::<Vec<_>>(),
            _ => {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "record expression has no record type",
                )]);
            }
        };
        let mut arguments = Vec::with_capacity(labels.len());
        for (index, label) in labels.into_iter().enumerate() {
            let Some((_, value)) = fields.iter().find(|(field, _)| field == &label) else {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "record expression is missing a typed field",
                )]);
            };
            let value = self.lower_value(value, assignments)?;
            let stored = product_field_shape(
                self.representations.representation(representation),
                index,
                expression.span,
            )?;
            arguments.push(self.adapt_to_storage_shape(
                value,
                stored,
                expression.span,
                assignments,
            )?);
        }
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::ProductNew {
                destination,
                representation,
                arguments,
            },
            span: expression.span,
        });
        Ok(destination)
    }

    pub(super) fn lower_record_update(
        &mut self,
        expression: &Expr,
        record: &Expr,
        fields: &[(String, Expr)],
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(representation) = self.record_types.get(&record.ty).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "record update has no representation requirement",
            )]);
        };
        let labels = match self.module.types.get(record.ty.0 as usize) {
            Some(Type::Record(fields)) => fields.clone(),
            _ => {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "record update has no record type",
                )]);
            }
        };
        let base = self.lower_value(record, assignments)?;
        let mut updates = Vec::with_capacity(fields.len());
        for (label, value) in fields {
            let value = self.lower_value(value, assignments)?;
            updates.push((label, value));
        }

        let mut arguments = Vec::with_capacity(labels.len());
        for (field_index, (label, _)) in labels.iter().enumerate() {
            let stored = product_field_shape(
                self.representations.representation(representation),
                field_index,
                expression.span,
            )?;
            if let Some((_, value)) = updates.iter().find(|(name, _)| *name == label) {
                arguments.push(self.adapt_to_storage_shape(
                    *value,
                    stored,
                    expression.span,
                    assignments,
                )?);
                continue;
            }
            let value = self.fresh(stored);
            assignments.push(Assignment {
                destination: value,
                kind: AssignmentKind::ProductGet {
                    destination: value,
                    representation,
                    field: field_index as u32,
                    value: base,
                },
                span: expression.span,
            });
            arguments.push(value);
        }
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::ProductNew {
                destination,
                representation,
                arguments,
            },
            span: expression.span,
        });
        Ok(destination)
    }

    pub(super) fn lower_field_access(
        &mut self,
        expression: &Expr,
        record: &Expr,
        field: &str,
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(representation) = self.record_types.get(&record.ty).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "record field access has no representation requirement",
            )]);
        };
        let Some((field_index, field_type)) =
            self.module
                .types
                .get(record.ty.0 as usize)
                .and_then(|ty| match ty {
                    Type::Record(fields) => fields
                        .iter()
                        .enumerate()
                        .find(|(_, (label, _))| label == field)
                        .map(|(index, (_, field_type))| (index, *field_type)),
                    _ => None,
                })
        else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "record field is not present in its type",
            )]);
        };
        let stored = product_field_shape(
            self.representations.representation(representation),
            field_index,
            expression.span,
        )?;
        if let Some(family) = erased_field_recovery_family(
            self.module,
            field_type,
            stored,
            ty,
            self.array_types,
            self.record_types,
        ) {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                format!(
                    "unsupported generic {family} field recovery: its nominal runtime layout depends on a type variable"
                ),
            )]);
        }
        let record = self.lower_value(record, assignments)?;
        let projected = self.fresh(stored);
        assignments.push(Assignment {
            destination: projected,
            kind: AssignmentKind::ProductGet {
                destination: projected,
                representation,
                field: field_index as u32,
                value: record,
            },
            span: expression.span,
        });
        self.adapt_to_storage_shape(projected, ty, expression.span, assignments)
    }
}

fn product_field_shape(
    representation: Option<&Representation>,
    field: usize,
    span: psrs_span::TextRange,
) -> Result<ValueShape, Vec<BackendError>> {
    let Some(Representation::Product { fields }) = representation else {
        return Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "record has no product representation",
        )]);
    };
    fields.get(field).copied().ok_or_else(|| {
        vec![BackendError::new(
            "P8 closure conversion",
            span,
            "record field has no storage shape",
        )]
    })
}
