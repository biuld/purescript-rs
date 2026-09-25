use super::super::{
    Assignment, AssignmentKind, Representation, ValueConversion, ValueId, ValueShape,
};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::Expr;
use psrs_hir::SymbolId;

impl FunctionLowerer<'_> {
    pub(super) fn lower_constructor(
        &mut self,
        expression: &Expr,
        symbol: SymbolId,
        arguments: &[Expr],
        result_shape: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let constructor = self
            .module
            .constructors
            .iter()
            .find(|constructor| constructor.symbol == symbol)
            .cloned()
            .ok_or_else(|| {
                vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "constructor value has no declaration",
                )]
            })?;
        if constructor.field_count != arguments.len() {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "constructor application is not saturated",
            )]);
        }
        if self.newtype_ids.contains(&constructor.type_id) {
            if constructor.field_count != 1 {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "newtype constructor must have exactly one field",
                )]);
            }
            let value = self.lower_value(&arguments[0], assignments)?;
            let Some(template_type) = constructor.field_types.first().copied() else {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "newtype constructor has no field type",
                )]);
            };
            let source_shape = self.value_shape(arguments[0].ty, expression.span)?;
            let template_shape = self.value_shape(template_type, expression.span)?;
            let conversion = self.typed_conversion(
                arguments[0].ty,
                template_type,
                source_shape,
                template_shape,
                expression.span,
            )?;
            if template_shape != result_shape {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "newtype field conversion does not match its result representation",
                )]);
            }
            let value = self.emit_conversion(
                value,
                source_shape,
                result_shape,
                conversion,
                expression.span,
                assignments,
            );
            return Ok(value);
        }
        let tag = self.constructor_tags.get(&symbol).copied().ok_or_else(|| {
            vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "constructor value has no known tag",
            )]
        })?;
        let destination = self.fresh(result_shape);
        if self.aggregate_types.contains(&constructor.type_id) {
            let representation = self
                .constructor_types
                .get(&symbol)
                .copied()
                .ok_or_else(|| {
                    vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "aggregate constructor has no representation requirement",
                    )]
                })?;
            let mut values = Vec::with_capacity(arguments.len());
            for (index, argument) in arguments.iter().enumerate() {
                let Some(template_type) = constructor.field_types.get(index).copied() else {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "constructor field has no declared type",
                    )]);
                };
                let stored = variant_field_shape(
                    self.representations.representation(representation),
                    tag,
                    index,
                    expression.span,
                )?;
                let value = self.lower_value(argument, assignments)?;
                let source_shape = self.value_shape(argument.ty, argument.span)?;
                let template_shape = self.value_shape(template_type, expression.span)?;
                if template_shape != stored
                    && (stored != super::conversion::erased_shape()
                        || !matches!(template_shape, ValueShape::Reference(_)))
                {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "constructor field storage does not match its normalized template",
                    )]);
                }
                let conversion = self.typed_conversion(
                    argument.ty,
                    template_type,
                    source_shape,
                    template_shape,
                    expression.span,
                )?;
                let value = self.emit_conversion(
                    value,
                    source_shape,
                    template_shape,
                    conversion,
                    expression.span,
                    assignments,
                );
                let value = if template_shape == stored {
                    value
                } else {
                    self.emit_conversion(
                        value,
                        template_shape,
                        stored,
                        ValueConversion::EraseReference,
                        expression.span,
                        assignments,
                    )
                };
                values.push(value);
            }
            assignments.push(Assignment {
                destination,
                kind: AssignmentKind::VariantNew {
                    destination,
                    representation,
                    case: tag,
                    fields: values,
                },
                span: expression.span,
            });
        } else {
            assignments.push(Assignment {
                destination,
                kind: AssignmentKind::Constant(i32::try_from(tag).map_err(|_| {
                    vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "constructor tag exceeds i32",
                    )]
                })?),
                span: expression.span,
            });
        }
        Ok(destination)
    }
}

fn variant_field_shape(
    representation: Option<&Representation>,
    tag: u32,
    field: usize,
    span: psrs_span::TextRange,
) -> Result<ValueShape, Vec<BackendError>> {
    let Some(Representation::Variant { cases }) = representation else {
        return Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "aggregate constructor has no variant representation",
        )]);
    };
    cases
        .iter()
        .find(|case| case.tag == tag)
        .and_then(|case| case.fields.get(field))
        .copied()
        .ok_or_else(|| {
            vec![BackendError::new(
                "P8 closure conversion",
                span,
                "constructor field has no storage shape",
            )]
        })
}
