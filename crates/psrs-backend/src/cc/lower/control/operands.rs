//! Lower each source operand into its predeclared storage role.

use super::{FunctionLowerer, control_error};
use crate::BackendError;
use crate::cc::control::ControlImplementation;
use crate::cc::payload::{PayloadPlanner, erased_shape};
use crate::cc::{
    Assignment, ControlContractId, RefShape, Reference, Representation, ValueId, ValueShape,
};
use psrs_core::Expr;

impl FunctionLowerer<'_> {
    pub(super) fn lower_control_operands(
        &mut self,
        contract: ControlContractId,
        arguments: &[Expr],
        assignments: &mut Vec<Assignment>,
    ) -> Result<Vec<ValueId>, Vec<BackendError>> {
        let proof = &self.representations.control_contracts[contract.0 as usize];
        let shapes = match proof.implementation {
            ControlImplementation::Frame {
                representation,
                case,
            } => {
                let Some(Representation::Variant { cases }) =
                    self.representations.representation(representation)
                else {
                    return Err(control_error(
                        self.module.span,
                        "control operand plan has no variant",
                    ));
                };
                cases
                    .iter()
                    .find(|candidate| candidate.tag == case)
                    .map(|case| case.fields.clone())
                    .ok_or_else(|| {
                        control_error(self.module.span, "control operand plan has no case")
                    })?
            }
            ControlImplementation::Call { signature, .. } => self
                .representations
                .signature(signature)
                .ok_or_else(|| {
                    control_error(self.module.span, "control operand plan has no signature")
                })?
                .parameters
                .iter()
                .take(arguments.len())
                .copied()
                .collect(),
        };
        if shapes.len() != arguments.len() {
            return Err(control_error(
                self.module.span,
                "control operand plan has an incompatible count",
            ));
        }
        arguments
            .iter()
            .zip(shapes)
            .map(|(argument, target)| {
                if target == erased_shape() {
                    return self.lower_array_element(argument, assignments);
                }
                let source = self.value_shape(argument.ty, argument.span)?;
                let value = self.lower_value(argument, assignments)?;
                if let ValueShape::Reference(Reference {
                    heap: RefShape::Closure(_),
                    ..
                }) = target
                {
                    let ValueShape::Reference(Reference {
                        heap: RefShape::Closure(signature),
                        ..
                    }) = source
                    else {
                        return Err(control_error(
                            argument.span,
                            "control callback has no callable representation",
                        ));
                    };
                    let plan = self.payload_callable(signature, true, argument.span)?;
                    if plan.output_shape(source) != target {
                        return Err(control_error(
                            argument.span,
                            "control callback disagrees with its unary protocol",
                        ));
                    }
                    return Ok(self.emit_conversion(
                        value,
                        source,
                        target,
                        plan,
                        argument.span,
                        assignments,
                    ));
                }
                if source != target {
                    return Err(control_error(
                        argument.span,
                        "control carrier disagrees with its nominal representation",
                    ));
                }
                Ok(value)
            })
            .collect()
    }
}
