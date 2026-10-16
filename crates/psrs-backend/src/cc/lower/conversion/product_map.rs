//! Expose field conversions as CC operations before dependency publication.
use super::*;
use crate::cc::Representation;

impl FunctionLowerer<'_> {
    pub(super) fn emit_product_map(
        &mut self,
        value: ValueId,
        source_shape: ValueShape,
        destination_shape: ValueShape,
        plan: &ValueConversion,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Option<ValueId> {
        let ValueConversion::ProductMap {
            source,
            target,
            labels,
            fields,
        } = plan
        else {
            return None;
        };
        let shape = |id| {
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(id),
            })
        };
        let Some(Representation::Product {
            fields: source_fields,
        }) = self.representations.representation(*source)
        else {
            return None;
        };
        let Some(Representation::Product {
            fields: target_fields,
        }) = self.representations.representation(*target)
        else {
            return None;
        };
        // Leave malformed plans intact so the aggregate verifier diagnoses them.
        if source_shape != shape(*source)
            || destination_shape != shape(*target)
            || source_fields.len() != fields.len()
            || target_fields.len() != fields.len()
            || self.representations.product_labels(*source) != Some(labels.as_slice())
            || self.representations.product_labels(*target) != Some(labels.as_slice())
            || fields
                .iter()
                .zip(source_fields)
                .zip(target_fields)
                .any(|((plan, source), target)| plan.output_shape(*source) != *target)
        {
            return None;
        }
        let source_fields = source_fields.clone();
        let target_fields = target_fields.clone();
        let mut arguments = Vec::with_capacity(fields.len());
        for (index, ((plan, source_shape), target_shape)) in fields
            .iter()
            .zip(source_fields)
            .zip(target_fields)
            .enumerate()
        {
            let field = self.fresh(source_shape);
            assignments.push(Assignment {
                destination: field,
                kind: AssignmentKind::ProductGet {
                    destination: field,
                    representation: *source,
                    field: index as u32,
                    value,
                },
                span,
            });
            arguments.push(self.emit_conversion(
                field,
                source_shape,
                target_shape,
                plan.clone(),
                span,
                assignments,
            ));
        }
        let result = self.fresh(destination_shape);
        assignments.push(Assignment {
            destination: result,
            kind: AssignmentKind::ProductNew {
                destination: result,
                representation: *target,
                arguments,
            },
            span,
        });
        Some(result)
    }
}
