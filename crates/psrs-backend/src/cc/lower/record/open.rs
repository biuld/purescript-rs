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
        let label = self.lower_literal(
            AssignmentKind::StringConstant(field.into()),
            expression.span,
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
            span: expression.span,
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
            span: expression.span,
        });
        let plan = self.recover_payload(target, expression.span)?;
        Ok(self.emit_conversion(value, erased, target, plan, expression.span, assignments))
    }
}
