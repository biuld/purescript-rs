use super::super::{Assignment, AssignmentKind, ValueId, ValueShape};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::Expr;
use psrs_span::TextRange;

/// Lowers the `String`/`Array Int` byte conversions.
///
/// `stringToBytes` produces an `Array Int`, so it names that representation the
/// way an array literal does; `bytesToString` names its `Array Int` argument.
/// The bytes cross as uninterpreted `Int`s in `0..255`, so arbitrary bytes
/// stay out of the Unicode text type
/// ([DEC-16](../../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
impl FunctionLowerer<'_> {
    pub(super) fn lower_string_to_bytes(
        &mut self,
        expression: &Expr,
        value: &Expr,
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(representation) = self.array_types.get(&expression.ty).copied() else {
            return Err(missing_representation(
                expression.span,
                "stringToBytes result has no representation requirement",
            ));
        };
        let value = self.lower_value(value, assignments)?;
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::StringToBytes {
                destination,
                representation,
                value,
            },
            span: expression.span,
        });
        Ok(destination)
    }

    pub(super) fn lower_bytes_to_string(
        &mut self,
        expression: &Expr,
        value: &Expr,
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(representation) = self.array_types.get(&value.ty).copied() else {
            return Err(missing_representation(
                expression.span,
                "bytesToString argument has no representation requirement",
            ));
        };
        let value = self.lower_value(value, assignments)?;
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::BytesToString {
                destination,
                representation,
                value,
            },
            span: expression.span,
        });
        Ok(destination)
    }
}

fn missing_representation(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
}
