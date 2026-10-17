use super::*;
use crate::cc::RecordOperation;

impl FunctionLowerer<'_> {
    pub(super) fn lower_open_record_field(
        &mut self,
        expression: &Expr,
        record: &Expr,
        field: &str,
        target: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        if self.module.record_field(record.ty, field).is_none() {
            return Err(record_error(
                expression.span,
                "open record field is absent from the checked prefix",
            ));
        }
        let record = self.lower_value(record, assignments)?;
        self.project_labelled_field(record, field, target, expression.span, assignments)
    }

    /// Find one known field by its literal label and read the erased slot,
    /// then recover the requested payload through the common plan.
    ///
    /// This is the kernel behind open-row field access and record patterns.
    /// Both consumers are checked against the record type's prefix; the literal
    /// label keys the runtime find, so no canonical index is assumed at the
    /// labelled store.
    pub(in crate::cc) fn project_labelled_field(
        &mut self,
        record: ValueId,
        field: &str,
        target: ValueShape,
        span: psrs_span::TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let label = self.lower_literal(
            AssignmentKind::StringConstant(field.into()),
            span,
            ValueShape::String,
            assignments,
        );
        let index = self.fresh(ValueShape::Integer);
        assignments.push(Assignment {
            destination: index,
            kind: AssignmentKind::Record {
                destination: index,
                record,
                operation: RecordOperation::Find { label },
            },
            span,
        });
        let erased = crate::cc::payload::erased_shape();
        let value = self.fresh(erased);
        assignments.push(Assignment {
            destination: value,
            kind: AssignmentKind::Record {
                destination: value,
                record,
                operation: RecordOperation::Read { index },
            },
            span,
        });
        let plan = self.recover_payload(target, span)?;
        Ok(self.emit_conversion(value, erased, target, plan, span, assignments))
    }

    /// Replace labelled slots in one fresh store. The base and every update
    /// expression lower exactly once, in source order; each replacement
    /// payload erases through the common planner into a private slot keyed by
    /// its literal label, and the kernel keeps the store's complete
    /// label/value sequence, including the unknown tail.
    pub(super) fn lower_open_record_update(
        &mut self,
        expression: &Expr,
        record: &Expr,
        fields: &[(String, Expr)],
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(_) = self.representations.record_storage() else {
            return Err(record_error(
                expression.span,
                "open record update has no storage schema",
            ));
        };
        let mut labels = Vec::with_capacity(fields.len());
        for (label, _) in fields {
            if self.module.record_field(record.ty, label).is_none() {
                return Err(record_error(
                    expression.span,
                    "open record update field is absent from the checked prefix",
                ));
            }
            if labels.contains(label) {
                return Err(record_error(
                    expression.span,
                    "open record update contains a duplicate field",
                ));
            }
            labels.push(label.clone());
        }
        // A local keeps the layout its binder established; other producers
        // convert into the store at this checked use boundary.
        let source_type = match &record.kind {
            psrs_core::ExprKind::Local(local) => self.local_types.get(local).copied(),
            _ => None,
        }
        .unwrap_or(record.ty);
        let record_type = record.ty;
        let base = self.lower_value(record, assignments)?;
        let base_shape = self.value_shape(source_type, expression.span)?;
        let expected = self.value_shape(record_type, expression.span)?;
        let base = if base_shape == expected {
            base
        } else {
            let plan = self.typed_conversion(
                source_type,
                record_type,
                base_shape,
                expected,
                expression.span,
            )?;
            self.emit_conversion(
                base,
                base_shape,
                expected,
                plan,
                expression.span,
                assignments,
            )
        };
        let erased = crate::cc::payload::erased_shape();
        let mut replacements = Vec::with_capacity(fields.len());
        for (label, value) in fields {
            let label_id = self.lower_literal(
                AssignmentKind::StringConstant(label.clone()),
                expression.span,
                ValueShape::String,
                assignments,
            );
            let value_id = self.lower_value(value, assignments)?;
            let source_shape = self.value_shape(value.ty, value.span)?;
            let plan = self.erase_payload(source_shape, expression.span)?;
            let payload = self.emit_conversion(
                value_id,
                source_shape,
                erased,
                plan,
                expression.span,
                assignments,
            );
            replacements.push((label_id, payload));
        }
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::Record {
                destination,
                record: base,
                operation: RecordOperation::Update { replacements },
            },
            span: expression.span,
        });
        Ok(destination)
    }
}
