use super::super::{Assignment, AssignmentKind, Function, Signature, ValueId, ValueShape};
use crate::BackendError;
use crate::cc::{RefShape, Reference, ReprId, RepresentationTable, SignatureId};
use psrs_span::TextRange;
use std::collections::HashMap;

pub(super) fn verify_function_reference(
    assignment: &Assignment,
    target: &Function,
    signature: &Signature,
    captures: &[ValueId],
    declared: &HashMap<ValueId, ValueShape>,
) -> Result<(), Vec<BackendError>> {
    let mut parameters = vec![aggregate_shape()];
    parameters.extend(signature.parameters.iter().copied());
    let target_parameters = target
        .parameters
        .iter()
        .filter_map(|parameter| target.values.iter().find(|value| value.id == *parameter))
        .map(|value| value.ty)
        .collect::<Vec<_>>();
    if target_parameters != parameters || target.result_type != signature.result {
        return Err(assignment_error(
            assignment,
            "function reference signature does not match its target",
        ));
    }
    let expected_captures = capture_shapes(target)?;
    if captures.len() != expected_captures.len()
        || captures
            .iter()
            .zip(expected_captures)
            .any(|(capture, expected)| {
                declared
                    .get(capture)
                    .copied()
                    .is_none_or(|actual| !capture_shape_compatible(actual, expected))
            })
    {
        return Err(assignment_error(
            assignment,
            "function reference captures do not match its target",
        ));
    }
    Ok(())
}

fn capture_shape_compatible(actual: ValueShape, expected: ValueShape) -> bool {
    if actual == expected {
        return true;
    }
    matches!(
        (actual, expected),
        (
            ValueShape::Reference(Reference {
                nullable: actual_nullable,
                heap: _,
            }),
            ValueShape::Reference(Reference {
                nullable: expected_nullable,
                heap: RefShape::Erased,
            }),
        ) if actual_nullable == expected_nullable
    )
}

pub(super) fn verify_capture_layout(function: &Function) -> Result<(), Vec<BackendError>> {
    capture_shapes(function).map(|_| ())
}

fn capture_shapes(function: &Function) -> Result<Vec<ValueShape>, Vec<BackendError>> {
    let declared = function
        .values
        .iter()
        .map(|value| (value.id, value.ty))
        .collect::<HashMap<_, _>>();
    let mut captures = HashMap::new();
    collect_capture_shapes(&function.assignments, &declared, &mut captures)?;
    let Some(max) = captures.keys().max().copied() else {
        return Ok(Vec::new());
    };
    (0..=max)
        .map(|index| {
            captures.get(&index).copied().ok_or_else(|| {
                vec![BackendError::invalid_ir(
                    "P8 CC verification",
                    function.span,
                    "closure capture indices must be contiguous",
                )]
            })
        })
        .collect()
}

fn collect_capture_shapes(
    assignments: &[Assignment],
    declared: &HashMap<ValueId, ValueShape>,
    captures: &mut HashMap<u32, ValueShape>,
) -> Result<(), Vec<BackendError>> {
    for assignment in assignments {
        match &assignment.kind {
            AssignmentKind::ClosureGetCapture { index, .. } => {
                let shape = declared
                    .get(&assignment.destination)
                    .copied()
                    .ok_or_else(|| {
                        vec![BackendError::invalid_ir(
                            "P8 CC verification",
                            assignment.span,
                            "closure capture destination has no value declaration",
                        )]
                    })?;
                if captures
                    .insert(*index, shape)
                    .is_some_and(|previous| previous != shape)
                {
                    return Err(vec![BackendError::invalid_ir(
                        "P8 CC verification",
                        assignment.span,
                        "closure capture index is read with incompatible duplicate shapes",
                    )]);
                }
            }
            AssignmentKind::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                collect_capture_shapes(then_assignments, declared, captures)?;
                collect_capture_shapes(else_assignments, declared, captures)?;
            }
            AssignmentKind::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                collect_capture_shapes(default_assignments, declared, captures)?;
                for case in cases {
                    collect_capture_shapes(&case.assignments, declared, captures)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn verify_call_shape(
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
    signature: &Signature,
    arguments: &[ValueId],
) -> Result<(), Vec<BackendError>> {
    if arguments.len() != signature.parameters.len()
        || arguments
            .iter()
            .zip(&signature.parameters)
            .any(|(argument, expected)| declared.get(argument).copied() != Some(*expected))
    {
        return Err(assignment_error(
            assignment,
            "call arguments do not match its signature",
        ));
    }
    require_destination(
        declared,
        assignment,
        signature.result,
        "call result shape does not match its signature",
    )
}

pub(super) fn verify_product_value(
    declared: &HashMap<ValueId, ValueShape>,
    value: ValueId,
    representation: ReprId,
    assignment: &Assignment,
) -> Result<(), Vec<BackendError>> {
    let actual = declared_shape(declared, value, assignment)?;
    if actual != aggregate_shape() && actual != repr_shape(representation) {
        return Err(assignment_error(
            assignment,
            "product operation source has an incompatible shape",
        ));
    }
    Ok(())
}

pub(super) fn verify_array_representation(
    table: &RepresentationTable,
    representation: ReprId,
    assignment: &Assignment,
) -> Result<ValueShape, Vec<BackendError>> {
    let Some(crate::cc::Representation::Array { element }) = table.representation(representation)
    else {
        return Err(assignment_error(
            assignment,
            "array operation requires an array representation",
        ));
    };
    Ok(*element)
}

pub(super) fn verify_array_value(
    declared: &HashMap<ValueId, ValueShape>,
    value: ValueId,
    table: &RepresentationTable,
    expected: Option<ReprId>,
    assignment: &Assignment,
) -> Result<ReprId, Vec<BackendError>> {
    let actual = declared_shape(declared, value, assignment)?;
    let ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(id),
    }) = actual
    else {
        return Err(assignment_error(
            assignment,
            "array operation source must be a non-null representation reference",
        ));
    };
    if !matches!(
        table.representation(id),
        Some(crate::cc::Representation::Array { .. })
    ) {
        return Err(assignment_error(
            assignment,
            "array operation source does not reference an array representation",
        ));
    }
    if expected.is_some_and(|expected| expected != id) {
        return Err(assignment_error(
            assignment,
            "array operation representation does not match its source",
        ));
    }
    Ok(id)
}

pub(super) fn verify_reference_handle(
    table: &RepresentationTable,
    reference: &Reference,
    assignment: &Assignment,
) -> Result<(), Vec<BackendError>> {
    match reference.heap {
        RefShape::Repr(id) if table.representation(id).is_none() => Err(assignment_error(
            assignment,
            "unknown representation handle",
        )),
        RefShape::Closure(id) if table.signature(id).is_none() => Err(assignment_error(
            assignment,
            "unknown closure signature handle",
        )),
        _ => Ok(()),
    }
}

pub(super) fn table_signature<'a>(
    table: &'a RepresentationTable,
    id: SignatureId,
    assignment: &Assignment,
) -> Result<&'a Signature, Vec<BackendError>> {
    table
        .signature(id)
        .ok_or_else(|| assignment_error(assignment, "unknown closure signature handle"))
}

pub(super) fn verify_value_shape(
    value: &ValueShape,
    table: &RepresentationTable,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    if let ValueShape::Reference(reference) = value {
        match reference.heap {
            RefShape::Repr(id) if table.representation(id).is_none() => {
                return Err(table_error(
                    span,
                    "CC value refers to an unknown representation",
                ));
            }
            RefShape::Closure(id) if table.signature(id).is_none() => {
                return Err(table_error(
                    span,
                    "CC value refers to an unknown closure signature",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn declared_shape(
    declared: &HashMap<ValueId, ValueShape>,
    value: ValueId,
    assignment: &Assignment,
) -> Result<ValueShape, Vec<BackendError>> {
    declared
        .get(&value)
        .copied()
        .ok_or_else(|| assignment_error(assignment, "CC operation uses an undeclared value"))
}

pub(super) fn require_value_shape(
    declared: &HashMap<ValueId, ValueShape>,
    value: ValueId,
    expected: ValueShape,
    assignment: &Assignment,
) -> Result<(), Vec<BackendError>> {
    if declared_shape(declared, value, assignment)? != expected {
        return Err(assignment_error(
            assignment,
            "CC operation operand has an incompatible shape",
        ));
    }
    Ok(())
}

pub(super) fn require_reference_value(
    declared: &HashMap<ValueId, ValueShape>,
    value: ValueId,
    assignment: &Assignment,
) -> Result<(), Vec<BackendError>> {
    if !matches!(
        declared_shape(declared, value, assignment)?,
        ValueShape::Reference(_)
    ) {
        return Err(assignment_error(
            assignment,
            "representation operation requires a reference operand",
        ));
    }
    Ok(())
}

pub(super) fn require_destination(
    declared: &HashMap<ValueId, ValueShape>,
    assignment: &Assignment,
    expected: ValueShape,
    message: &'static str,
) -> Result<(), Vec<BackendError>> {
    if declared.get(&assignment.destination).copied() != Some(expected) {
        return Err(assignment_error(assignment, message));
    }
    Ok(())
}

pub(super) fn verify_embedded_destination(
    assignment: &Assignment,
    embedded: ValueId,
) -> Result<(), Vec<BackendError>> {
    if embedded != assignment.destination {
        return Err(assignment_error(
            assignment,
            "operation destination disagrees with its assignment destination",
        ));
    }
    Ok(())
}

pub(super) fn closure_shape(signature: SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}

pub(super) fn aggregate_shape() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    })
}

pub(super) fn repr_shape(representation: ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(representation),
    })
}

pub(super) fn table_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::invalid_ir(
        "P8 CC verification",
        span,
        message,
    )]
}

pub(super) fn undef_error(span: TextRange, fallback: TextRange) -> Vec<BackendError> {
    vec![BackendError::invalid_ir(
        "P8 CC verification",
        if span == TextRange::new(0, 0) {
            fallback
        } else {
            span
        },
        "CC assignment uses a value before it is defined",
    )]
}

pub(super) fn assignment_error(
    assignment: &Assignment,
    message: &'static str,
) -> Vec<BackendError> {
    vec![BackendError::invalid_ir(
        "P8 CC verification",
        assignment.span,
        message,
    )]
}
