use super::super::{Assignment, AssignmentKind, ValueId, ValueType};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::{Expr, Type};

impl FunctionLowerer<'_> {
    pub(super) fn lower_record(
        &mut self,
        expression: &Expr,
        fields: &[(String, Expr)],
        ty: ValueType,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(type_index) = self.record_types.get(&expression.ty).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "record expression has no concrete GC struct layout",
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
        for label in labels {
            let Some((_, value)) = fields.iter().find(|(field, _)| field == &label) else {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "record expression is missing a typed field",
                )]);
            };
            arguments.push(self.lower_value(value, assignments)?);
        }
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::StructNew {
                destination,
                type_index,
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
        ty: ValueType,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(type_index) = self.record_types.get(&record.ty).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "record field access has no concrete GC struct layout",
            )]);
        };
        let Some(field_index) =
            self.module
                .types
                .get(record.ty.0 as usize)
                .and_then(|ty| match ty {
                    Type::Record(fields) => fields.iter().position(|(label, _)| label == field),
                    _ => None,
                })
        else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "record field is not present in its type",
            )]);
        };
        let record = self.lower_value(record, assignments)?;
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::StructGet {
                destination,
                type_index,
                field: field_index as u32,
                value: record,
            },
            span: expression.span,
        });
        Ok(destination)
    }
}
