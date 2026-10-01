//! Validates generated adapter factories after every CC function is available.

use super::super::super::helpers::assignment_error;
use crate::BackendError;
use crate::cc::{Assignment, Signature, ValueConversion};
use psrs_hir::SymbolId;
use std::collections::HashMap;

pub(in crate::cc::verify::ops) fn verify_adapter_functions(
    assignment: &Assignment,
    plan: &ValueConversion,
    signatures: &HashMap<SymbolId, Signature>,
) -> Result<(), Vec<BackendError>> {
    match plan {
        ValueConversion::FunctionAdapter {
            function,
            source,
            destination,
        } => {
            if !signatures.get(function).is_some_and(|signature| {
                signature.parameters == [*source] && signature.result == *destination
            }) {
                return Err(assignment_error(
                    assignment,
                    "function adapter factory does not match its conversion endpoints",
                ));
            }
        }
        ValueConversion::Sequence(steps) => {
            for step in steps {
                verify_adapter_functions(assignment, step, signatures)?;
            }
        }
        ValueConversion::ArrayMap { element, .. } => {
            verify_adapter_functions(assignment, element, signatures)?;
        }
        ValueConversion::ProductMap { fields, .. } => {
            for field in fields {
                verify_adapter_functions(assignment, field, signatures)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc::{AssignmentKind, RefShape, Reference, SignatureId, ValueShape};
    use crate::types::ValueId;
    use psrs_hir::ModuleId;
    use psrs_span::TextRange;

    #[test]
    fn nested_adapter_factories_require_exact_callable_endpoints() {
        let function = SymbolId::new(ModuleId(0), 0);
        let source = ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Closure(SignatureId(0)),
        });
        let destination = ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Closure(SignatureId(1)),
        });
        let plan = ValueConversion::Sequence(vec![ValueConversion::FunctionAdapter {
            function,
            source,
            destination,
        }]);
        let assignment = Assignment {
            destination: ValueId(0),
            kind: AssignmentKind::Unreachable,
            span: TextRange::new(10, 20),
        };
        let mut signatures = HashMap::new();
        assert!(verify_adapter_functions(&assignment, &plan, &signatures).is_err());
        signatures.insert(
            function,
            Signature {
                parameters: vec![destination],
                result: destination,
            },
        );
        assert!(verify_adapter_functions(&assignment, &plan, &signatures).is_err());
        signatures.insert(
            function,
            Signature {
                parameters: vec![source],
                result: source,
            },
        );
        assert!(verify_adapter_functions(&assignment, &plan, &signatures).is_err());
        signatures.insert(
            function,
            Signature {
                parameters: vec![source],
                result: destination,
            },
        );
        verify_adapter_functions(&assignment, &plan, &signatures).unwrap();
    }
}
