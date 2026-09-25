use super::super::{
    Assignment, AssignmentKind, ReprId, Representation, RepresentationTable, ValueId, ValueShape,
};
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
        let labels = record_labels(self.representations, representation, expression.span)?;
        let mut arguments = Vec::with_capacity(labels.len());
        for (index, label) in labels.iter().enumerate() {
            let Some(field_layout) = layout.field(label) else {
                return Err(record_error(
                    expression.span,
                    "canonical record field is missing from its checked class layout",
                ));
            };
            let Some((_, expression_value)) = fields.iter().find(|(field, _)| field == label)
            else {
                return Err(record_error(
                    expression.span,
                    "record expression is missing a typed field",
                ));
            };
            let value = self.lower_value(expression_value, assignments)?;
            let stored = product_field_shape(
                self.representations.representation(representation),
                index,
                expression.span,
            )?;
            let source_shape = self.value_shape(expression_value.ty, expression_value.span)?;
            let template_shape = self.value_shape(field_layout.ty, expression.span)?;
            if template_shape != stored {
                return Err(record_error(
                    expression.span,
                    "record field storage does not match its canonical generic layout",
                ));
            }
            let conversion = self.typed_conversion(
                expression_value.ty,
                field_layout.ty,
                source_shape,
                template_shape,
                expression.span,
            )?;
            arguments.push(self.emit_conversion(
                value,
                source_shape,
                stored,
                conversion,
                expression.span,
                assignments,
            ));
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
        let canonical_labels =
            record_labels(self.representations, representation, expression.span)?;
        let base = self.lower_value(record, assignments)?;
        let mut updates = Vec::with_capacity(fields.len());
        for (label, value) in fields {
            let Some(field_layout) = layout.field(label) else {
                return Err(record_error(
                    expression.span,
                    "updated record field is missing from its checked class layout",
                ));
            };
            let value_id = self.lower_value(value, assignments)?;
            let source_shape = self.value_shape(value.ty, value.span)?;
            let target_shape = self.value_shape(field_layout.ty, expression.span)?;
            updates.push((label, value_id, value.ty, source_shape, target_shape));
        }

        let mut arguments = Vec::with_capacity(canonical_labels.len());
        for (field_index, label) in canonical_labels.iter().enumerate() {
            let Some(field_layout) = layout.field(label) else {
                return Err(record_error(
                    expression.span,
                    "canonical record field is missing from its checked class layout",
                ));
            };
            let stored = product_field_shape(
                self.representations.representation(representation),
                field_index,
                expression.span,
            )?;
            if let Some((_, value, source_type, source_shape, target_shape)) =
                updates.iter().find(|(name, ..)| *name == label)
            {
                if *target_shape != stored {
                    return Err(record_error(
                        expression.span,
                        "updated field does not match its canonical record layout",
                    ));
                }
                let conversion = self.typed_conversion(
                    *source_type,
                    field_layout.ty,
                    *source_shape,
                    *target_shape,
                    expression.span,
                )?;
                arguments.push(self.emit_conversion(
                    *value,
                    *source_shape,
                    stored,
                    conversion,
                    expression.span,
                    assignments,
                ));
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
        let labels = record_labels(self.representations, representation, expression.span)?;
        let Some(field_index) = labels.iter().position(|label| label == field) else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "record field has no canonical product index",
            )]);
        };
        let stored = product_field_shape(
            self.representations.representation(representation),
            field_index,
            expression.span,
        )?;
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
        let target_type = expression.ty;
        let target_shape = self.value_shape(target_type, expression.span)?;
        let conversion = self.typed_conversion(
            field_layout.ty,
            target_type,
            stored,
            target_shape,
            expression.span,
        )?;
        Ok(self.emit_conversion(
            projected,
            stored,
            ty,
            conversion,
            expression.span,
            assignments,
        ))
    }
}

fn record_labels(
    representations: &RepresentationTable,
    representation: ReprId,
    span: psrs_span::TextRange,
) -> Result<Vec<String>, Vec<BackendError>> {
    representations
        .product_labels(representation)
        .map(<[String]>::to_vec)
        .ok_or_else(|| {
            vec![BackendError::new(
                "P8 closure conversion",
                span,
                "record representation has no canonical field labels",
            )]
        })
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

fn record_error(span: psrs_span::TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
}
