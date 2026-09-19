use super::super::{Assignment, AssignmentKind, ValueId, ValueType};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::Expr;

impl FunctionLowerer<'_> {
    pub(super) fn lower_array(
        &mut self,
        expression: &Expr,
        elements: &[Expr],
        ty: ValueType,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(type_index) = self.array_types.get(&expression.ty).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "array expression has no concrete GC array layout",
            )]);
        };
        let values = elements
            .iter()
            .map(|element| self.lower_value(element, assignments))
            .collect::<Result<Vec<_>, _>>()?;
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::ArrayNew {
                destination,
                type_index,
                elements: values,
            },
            span: expression.span,
        });
        Ok(destination)
    }

    pub(super) fn lower_array_update(
        &mut self,
        expression: &Expr,
        array: &Expr,
        index: &Expr,
        new_value: &Expr,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(type_index) = self.array_types.get(&array.ty).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "array update has no concrete GC array layout",
            )]);
        };
        let array = self.lower_value(array, assignments)?;
        let index = self.lower_value(index, assignments)?;
        let new_value = self.lower_value(new_value, assignments)?;
        assignments.push(Assignment {
            destination: array,
            kind: AssignmentKind::ArraySet {
                destination: array,
                type_index,
                value: array,
                index,
                new_value,
            },
            span: expression.span,
        });
        Ok(array)
    }
}
