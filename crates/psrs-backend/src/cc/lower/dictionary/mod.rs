use super::{Assignment, FunctionLowerer, ValueId, ValueShape};
use crate::BackendError;
use psrs_core::{Expr, ExprKind, dictionary::ClassLayout};

#[cfg(test)]
mod tests;

impl FunctionLowerer<'_> {
    /// Lowers a checked Core dictionary value through the same product path as
    /// every other record. The layout is a verified view of its Core record;
    /// this function adds no class-specific CC operation.
    pub(super) fn lower_dictionary_value(
        &mut self,
        expression: &Expr,
        layout: &ClassLayout,
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        if expression.ty != layout.dictionary_type() {
            return Err(dictionary_error(
                expression,
                "dictionary expression type differs from its class layout",
            ));
        }
        match &expression.kind {
            ExprKind::Record { fields } => {
                layout
                    .validate_record_value(self.module, fields)
                    .map_err(|message| dictionary_error(expression, message))?;
                self.lower_record(expression, fields, layout, ty, assignments)
            }
            _ => self.lower_value_inner(expression, ty, assignments),
        }
    }
}

fn dictionary_error(expression: &Expr, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new(
        "P8 closure conversion",
        expression.span,
        message,
    )]
}
