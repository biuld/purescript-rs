use super::super::{Assignment, AssignmentKind, Representation, ValueId, ValueShape};
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
            return self.lower_value(&arguments[0], assignments);
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
                let stored = variant_field_shape(
                    self.representations.representation(representation),
                    tag,
                    index,
                    expression.span,
                )?;
                let value = self.lower_value(argument, assignments)?;
                values.push(self.adapt_to_storage_shape(
                    value,
                    stored,
                    expression.span,
                    assignments,
                )?);
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
