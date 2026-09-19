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
}
