//! Shared adaptation from a closure-valued body to a flattened callable.
use super::super::FunctionLowerer;
use crate::BackendError;
use crate::cc::{
    Assignment, AssignmentKind, RefShape, Reference, SignatureId, ValueId, ValueShape,
};
use psrs_span::TextRange;

pub(in crate::cc::lower) struct EtaApplication {
    pub value: ValueId,
    pub shape: ValueShape,
    pub signature: SignatureId,
    pub arguments: Vec<ValueId>,
    pub result_shape: ValueShape,
    pub span: TextRange,
}

impl FunctionLowerer<'_> {
    pub(in crate::cc::lower) fn apply_eta_result(
        &mut self,
        application: EtaApplication,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let EtaApplication {
            value,
            shape,
            signature,
            arguments,
            result_shape,
            span,
        } = application;
        let closure_shape = ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Closure(signature),
        });
        let callable = if super::is_erased_reference(shape) {
            let closure = self.fresh(closure_shape);
            assignments.push(Assignment {
                destination: closure,
                kind: AssignmentKind::RepresentationCast {
                    destination: closure,
                    value,
                    reference: Reference {
                        nullable: false,
                        heap: RefShape::Closure(signature),
                    },
                },
                span,
            });
            closure
        } else if shape == closure_shape {
            value
        } else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "eta body has an incompatible closure representation",
            )]);
        };
        let result = self.fresh(result_shape);
        assignments.push(Assignment {
            destination: result,
            kind: AssignmentKind::IndirectCall {
                function: callable,
                signature,
                arguments,
            },
            span,
        });
        Ok(result)
    }
}
