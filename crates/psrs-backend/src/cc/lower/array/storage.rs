//! Low-level initialized allocation and unsafe writes; library code owns loops.
use super::super::FunctionLowerer;
use crate::BackendError;
use crate::cc::{Assignment, AssignmentKind, ValueId, ValueShape};
use psrs_core::Expr;

impl FunctionLowerer<'_> {
    pub(in crate::cc::lower) fn lower_array_element(
        &mut self,
        value: &Expr,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let shape = self.value_shape(value.ty, value.span)?;
        let lowered = self.lower_value(value, assignments)?;
        let plan = self.erase_payload(shape, value.span)?;
        Ok(self.emit_conversion(
            lowered,
            shape,
            crate::cc::payload::erased_shape(),
            plan,
            value.span,
            assignments,
        ))
    }
    pub(in crate::cc::lower) fn lower_array_fill(
        &mut self,
        expression: &Expr,
        length: &Expr,
        value: &Expr,
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let representation = *self.array_types.get(&expression.ty).ok_or_else(|| {
            vec![BackendError::invalid_ir(
                "P8 closure conversion",
                expression.span,
                "arrayFill has no checked array representation",
            )]
        })?;
        let length = self.lower_value(length, assignments)?;
        let value = self.lower_array_element(value, assignments)?;
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            span: expression.span,
            kind: AssignmentKind::ArrayFill {
                destination,
                representation,
                length,
                value,
            },
        });
        Ok(destination)
    }
    pub(in crate::cc::lower) fn lower_array_write(
        &mut self,
        expression: &Expr,
        array: &Expr,
        index: &Expr,
        new_value: &Expr,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let representation = *self.array_types.get(&array.ty).ok_or_else(|| {
            vec![BackendError::invalid_ir(
                "P8 closure conversion",
                expression.span,
                "arrayWrite has no checked array representation",
            )]
        })?;
        let value = self.lower_value(array, assignments)?;
        let index = self.lower_value(index, assignments)?;
        let new_value = self.lower_array_element(new_value, assignments)?;
        assignments.push(Assignment {
            destination: value,
            span: expression.span,
            kind: AssignmentKind::ArraySet {
                destination: value,
                representation,
                value,
                index,
                new_value,
            },
        });
        Ok(value)
    }
}
