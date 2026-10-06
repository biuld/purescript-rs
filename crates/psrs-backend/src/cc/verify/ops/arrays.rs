//! CC verification of the array operations.
//!
//! Each rule pairs the named array representation with the value it reads or
//! writes, so an `array.new`, `array.len`, `array.get`, `array.clone`, or
//! `array.set` can only name an array that actually holds those elements.

use super::super::helpers::*;
use crate::BackendError;
use crate::cc::Representation;
use crate::cc::{Assignment, AssignmentKind, RepresentationTable, ValueId, ValueShape};
use std::collections::HashMap;

/// Verifies one array assignment and records the values it reads.
pub(super) fn verify_array_assignment(
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
    table: &RepresentationTable,
    uses: &mut Vec<ValueId>,
    signatures: &HashMap<psrs_hir::SymbolId, crate::cc::Signature>,
    complete: bool,
) -> Result<(), Vec<BackendError>> {
    match &assignment.kind {
        AssignmentKind::ArrayNew {
            destination,
            representation,
            elements,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            let Some(Representation::Array { element }) = table.representation(*representation)
            else {
                return Err(assignment_error(
                    assignment,
                    "array construction requires an array representation",
                ));
            };
            if elements
                .iter()
                .any(|element_id| declared.get(element_id).copied() != Some(*element))
            {
                return Err(assignment_error(
                    assignment,
                    "array construction elements have incompatible shapes",
                ));
            }
            require_destination(
                declared,
                assignment,
                repr_shape(*representation),
                "array construction has an incompatible result shape",
            )?;
            uses.extend(elements.iter().copied());
        }
        AssignmentKind::ArrayLen { destination, value } => {
            verify_embedded_destination(assignment, *destination)?;
            verify_array_value(declared, *value, table, None, assignment)?;
            require_destination(
                declared,
                assignment,
                ValueShape::Integer,
                "array length must produce Integer",
            )?;
            uses.push(*value);
        }
        AssignmentKind::ArrayGet {
            destination,
            representation,
            value,
            index,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            let element = verify_array_representation(table, *representation, assignment)?;
            verify_array_value(declared, *value, table, Some(*representation), assignment)?;
            require_value_shape(declared, *index, ValueShape::Integer, assignment)?;
            require_destination(
                declared,
                assignment,
                element,
                "array projection has an incompatible result shape",
            )?;
            uses.extend([*value, *index]);
        }
        AssignmentKind::ArrayClone {
            destination,
            representation,
            value,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            verify_array_representation(table, *representation, assignment)?;
            verify_array_value(declared, *value, table, Some(*representation), assignment)?;
            require_destination(
                declared,
                assignment,
                repr_shape(*representation),
                "array clone has an incompatible result shape",
            )?;
            uses.push(*value);
        }
        AssignmentKind::ArraySet {
            destination,
            representation,
            value,
            index,
            new_value,
        } => {
            if assignment.destination != *destination || assignment.destination != *value {
                return Err(assignment_error(
                    assignment,
                    "array update destination must be the updated array",
                ));
            }
            let element = verify_array_representation(table, *representation, assignment)?;
            verify_array_value(declared, *value, table, Some(*representation), assignment)?;
            require_value_shape(declared, *index, ValueShape::Integer, assignment)?;
            require_value_shape(declared, *new_value, element, assignment)?;
            uses.extend([*value, *index, *new_value]);
        }
        AssignmentKind::ArrayAppend {
            destination,
            representation,
            left,
            right,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            verify_array_representation(table, *representation, assignment)?;
            verify_array_value(declared, *left, table, Some(*representation), assignment)?;
            verify_array_value(declared, *right, table, Some(*representation), assignment)?;
            require_destination(
                declared,
                assignment,
                repr_shape(*representation),
                "array append has an incompatible result shape",
            )?;
            uses.extend([*left, *right]);
        }
        AssignmentKind::ArrayApply {
            destination,
            functions,
            values,
            functions_representation,
            values_representation,
            result_representation,
            signature,
            invoker,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            let callback =
                verify_array_representation(table, *functions_representation, assignment)?;
            let argument = verify_array_representation(table, *values_representation, assignment)?;
            let result = verify_array_representation(table, *result_representation, assignment)?;
            let contract = table_signature(table, *signature, assignment)?;
            if callback != closure_shape(*signature)
                || contract.parameters.first() != Some(&argument)
            {
                return Err(assignment_error(
                    assignment,
                    "arrayApply callback ABI does not match its array elements",
                ));
            }
            if complete {
                let expected = crate::cc::Signature {
                    parameters: vec![callback, argument],
                    result,
                };
                if signatures.get(invoker) != Some(&expected) {
                    return Err(assignment_error(
                        assignment,
                        "arrayApply invoker is missing or has an incompatible ABI",
                    ));
                }
            }
            verify_array_value(
                declared,
                *functions,
                table,
                Some(*functions_representation),
                assignment,
            )?;
            verify_array_value(
                declared,
                *values,
                table,
                Some(*values_representation),
                assignment,
            )?;
            require_destination(
                declared,
                assignment,
                repr_shape(*result_representation),
                "arrayApply has an incompatible result shape",
            )?;
            uses.extend([*functions, *values]);
        }
        _ => unreachable!("array verifier received another assignment"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc::{ReprId, Signature, SignatureId};
    use psrs_hir::{ModuleId, SymbolId};
    use psrs_span::TextRange;

    #[test]
    fn array_apply_requires_a_real_invoker_with_the_checked_abi() {
        let callback = closure_shape(SignatureId(0));
        let table = RepresentationTable {
            signatures: vec![Signature {
                parameters: vec![ValueShape::Integer],
                result: ValueShape::Number,
            }],
            representations: vec![
                Representation::Array { element: callback },
                Representation::Array {
                    element: ValueShape::Integer,
                },
                Representation::Array {
                    element: ValueShape::Number,
                },
            ],
            ..Default::default()
        };
        let invoker = SymbolId::new(ModuleId(9), 1);
        let assignment = Assignment {
            destination: ValueId(2),
            span: TextRange::new(0, 1),
            kind: AssignmentKind::ArrayApply {
                destination: ValueId(2),
                functions: ValueId(0),
                values: ValueId(1),
                functions_representation: ReprId(0),
                values_representation: ReprId(1),
                result_representation: ReprId(2),
                signature: SignatureId(0),
                invoker,
            },
        };
        let declared = (0..3)
            .map(|id| (ValueId(id), repr_shape(ReprId(id))))
            .collect();
        let mut signatures = HashMap::new();
        assert!(
            verify_array_assignment(
                &assignment,
                &declared,
                &table,
                &mut Vec::new(),
                &signatures,
                true
            )
            .is_err()
        );
        signatures.insert(
            invoker,
            Signature {
                parameters: vec![callback, ValueShape::Integer],
                result: ValueShape::Integer,
            },
        );
        assert!(
            verify_array_assignment(
                &assignment,
                &declared,
                &table,
                &mut Vec::new(),
                &signatures,
                true
            )
            .is_err()
        );
        signatures.get_mut(&invoker).unwrap().result = ValueShape::Number;
        verify_array_assignment(
            &assignment,
            &declared,
            &table,
            &mut Vec::new(),
            &signatures,
            true,
        )
        .unwrap();
    }
}
