//! Recover machine fields at the explicit erased carrier/callback boundary.

use super::{FunctionLowerer, control_error};
use crate::BackendError;
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, AssignmentKind, ReprId, Representation, ValueId, ValueShape};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    pub(super) fn control_field_shape(
        &self,
        representation: ReprId,
        case: u32,
        field: u32,
    ) -> ValueShape {
        let Some(Representation::Variant { cases }) =
            self.representations.representation(representation)
        else {
            unreachable!("machine fields require a predeclared variant");
        };
        cases
            .iter()
            .find(|candidate| candidate.tag == case)
            .and_then(|case| case.fields.get(field as usize))
            .copied()
            .expect("machine projection must name a predeclared field")
    }

    pub(super) fn control_fields(
        &mut self,
        representation: ReprId,
        case: u32,
        fields: Vec<ValueId>,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<Vec<ValueId>, Vec<BackendError>> {
        let Some(Representation::Variant { cases }) =
            self.representations.representation(representation)
        else {
            return Err(control_error(
                span,
                "control frame has no variant representation",
            ));
        };
        let shapes = cases
            .iter()
            .find(|candidate| candidate.tag == case)
            .map(|case| case.fields.clone())
            .ok_or_else(|| control_error(span, "control frame has an unknown case"))?;
        if shapes.len() != fields.len() {
            return Err(control_error(
                span,
                "control frame has an incompatible field count",
            ));
        }
        fields
            .into_iter()
            .zip(shapes)
            .map(|(value, shape)| {
                // Source payloads and generated helpers use the erased protocol.
                // Storage is nominal; recovery is explicit and checked at runtime.
                let source = self
                    .values
                    .iter()
                    .find(|declaration| declaration.id == value)
                    .map(|declaration| declaration.ty)
                    .ok_or_else(|| control_error(span, "control field has no value declaration"))?;
                if source == shape {
                    return Ok(value);
                }
                if source != erased_shape() {
                    return Err(control_error(
                        span,
                        "control field has an incompatible representation",
                    ));
                }
                // These are references to existing machine objects, not source
                // Array/Function payloads. Payload recovery would adapt their
                // contents and could copy a shared mutable status cell.
                let ValueShape::Reference(reference) = shape else {
                    return Err(control_error(
                        span,
                        "control field requires a reference shape",
                    ));
                };
                let destination = self.fresh(shape);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::RepresentationCast {
                        destination,
                        value,
                        reference,
                    },
                    span,
                });
                Ok(destination)
            })
            .collect()
    }
}
