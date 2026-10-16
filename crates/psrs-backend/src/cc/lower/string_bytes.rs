use super::super::{
    Assignment, AssignmentKind, RefShape, Reference, ReprId, Representation, ValueConversion,
    ValueId, ValueShape,
};
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
        let Some(source_representation) = self.array_types.get(&expression.ty).copied() else {
            return Err(missing_representation(
                expression.span,
                "stringToBytes result has no representation requirement",
            ));
        };
        let representation = self.byte_buffer_representation(expression.span)?;
        let buffer_shape = ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(representation),
        });
        let value = self.lower_value(value, assignments)?;
        let destination = self.fresh(buffer_shape);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::StringToBytes {
                destination,
                representation,
                value,
            },
            span: expression.span,
        });
        let element = self.erase_payload(ValueShape::Integer, expression.span)?;
        let plan = ValueConversion::ArrayMap {
            source: representation,
            target: source_representation,
            element: Box::new(element),
        };
        Ok(self.emit_conversion(
            destination,
            buffer_shape,
            ty,
            plan,
            expression.span,
            assignments,
        ))
    }

    pub(super) fn lower_bytes_to_string(
        &mut self,
        expression: &Expr,
        value: &Expr,
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(source_representation) = self.array_types.get(&value.ty).copied() else {
            return Err(missing_representation(
                expression.span,
                "bytesToString argument has no representation requirement",
            ));
        };
        let representation = self.byte_buffer_representation(expression.span)?;
        let source_shape = self.value_shape(value.ty, value.span)?;
        let buffer_shape = ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(representation),
        });
        let value = self.lower_value(value, assignments)?;
        let element = self.recover_payload(ValueShape::Integer, expression.span)?;
        let plan = ValueConversion::ArrayMap {
            source: source_representation,
            target: representation,
            element: Box::new(element),
        };
        let value = self.emit_conversion(
            value,
            source_shape,
            buffer_shape,
            plan,
            expression.span,
            assignments,
        );
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

    fn byte_buffer_representation(&self, span: TextRange) -> Result<ReprId, Vec<BackendError>> {
        self.representations.representations.iter().position(|representation|
            matches!(representation, Representation::Array { element } if *element == ValueShape::Integer))
            .map(|index| ReprId(index as u32))
            .ok_or_else(|| missing_representation(span, "text codec has no private integer buffer representation"))
    }
}

fn missing_representation(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
}
