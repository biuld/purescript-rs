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
}
