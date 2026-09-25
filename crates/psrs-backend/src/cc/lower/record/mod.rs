use super::super::layout::erased_field_recovery_family;
use super::super::{Assignment, AssignmentKind, Representation, ValueId, ValueShape};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::{Expr, dictionary::ClassLayout};

#[cfg(test)]
mod tests;

impl FunctionLowerer<'_> {
    pub(super) fn lower_record(
        &mut self,
        expression: &Expr,
        fields: &[(String, Expr)],
        layout: &ClassLayout,
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(representation) = self.record_types.get(&layout.dictionary_type()).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "record expression has no representation requirement",
            )]);
        };
        let mut arguments = Vec::with_capacity(layout.fields().len());
        for field in layout.fields() {
            let Some((_, value)) = fields.iter().find(|(label, _)| label == &field.label) else {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "record expression is missing a typed field",
                )]);
            };
            let value = self.lower_value(value, assignments)?;
            let stored = product_field_shape(
                self.representations.representation(representation),
                field.index as usize,
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
        let layout = ClassLayout::from_record_type(self.module, record.ty).map_err(|message| {
            vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                message,
            )]
        })?;
        let base = self.lower_value(record, assignments)?;
        let mut updates = Vec::with_capacity(fields.len());
        for (label, value) in fields {
            let value = self.lower_value(value, assignments)?;
            updates.push((label, value));
        }

        let mut arguments = Vec::with_capacity(layout.fields().len());
        for field in layout.fields() {
            let stored = product_field_shape(
                self.representations.representation(representation),
                field.index as usize,
                expression.span,
            )?;
            if let Some((_, value)) = updates.iter().find(|(name, _)| *name == &field.label) {
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
                    field: field.index,
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
        let layout = ClassLayout::from_record_type(self.module, record.ty).map_err(|message| {
            vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                message,
            )]
        })?;
        let Some(field_layout) = layout.field(field) else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "record field is not present in its type",
            )]);
        };
        let stored = product_field_shape(
            self.representations.representation(representation),
            field_layout.index as usize,
            expression.span,
        )?;
        if let Some(family) = erased_field_recovery_family(
            self.module,
            field_layout.ty,
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
                field: field_layout.index,
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
