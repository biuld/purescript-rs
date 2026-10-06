//! Target-neutral operation and representation-table checks for CC.

use super::super::{Assignment, AssignmentKind, Function, Signature, ValueId, ValueShape};
use crate::BackendError;
use crate::cc::{RefShape, Reference, Representation, RepresentationTable};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

use super::adaptation::verify_erased_adaptation;
use super::helpers::*;
use super::scalar::{verify_binary_operation, verify_unary_operation};
use super::variant::verify_variant_assignment;
use tag_switch::verify_tag_switch;

mod aggregate;
mod arrays;
mod string_bytes;
mod table;
mod tag_switch;

pub(super) use table::verify_table;

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_assignments(
    assignments: &[Assignment],
    available: &mut HashSet<ValueId>,
    declared: &HashMap<ValueId, ValueShape>,
    signatures: &HashMap<SymbolId, Signature>,
    table: &RepresentationTable,
    functions: Option<&HashMap<SymbolId, &Function>>,
    function_span: TextRange,
    function_name: &str,
) -> Result<(), Vec<BackendError>> {
    for assignment in assignments {
        let mut uses = Vec::new();
        match &assignment.kind {
            AssignmentKind::AggregateConvert {
                destination,
                value,
                conversion,
            } => {
                if assignment.destination != *destination {
                    return Err(assignment_error(
                        assignment,
                        "aggregate conversion destination disagrees with its assignment",
                    ));
                }
                let source = declared_shape(declared, *value, assignment)?;
                let target = declared_shape(declared, *destination, assignment)?;
                aggregate::verify_conversion(assignment, conversion, source, target, table)?;
                if functions.is_some() {
                    aggregate::verify_adapter_functions(assignment, &conversion.plan, signatures)?;
                }
                uses.push(*value);
            }
            AssignmentKind::Constant(value) => {
                let destination = declared_shape(declared, assignment.destination, assignment)?;
                if !matches!(destination, ValueShape::Integer | ValueShape::Boolean)
                    || (destination == ValueShape::Boolean && !matches!(value, 0 | 1))
                {
                    return Err(assignment_error(
                        assignment,
                        "integer constant has an incompatible result shape",
                    ));
                }
            }
            AssignmentKind::NumberConstant(_) => {
                require_destination(
                    declared,
                    assignment,
                    ValueShape::Number,
                    "number constant has an incompatible result shape",
                )?;
            }
            AssignmentKind::StringConstant(_) => {
                require_destination(
                    declared,
                    assignment,
                    ValueShape::String,
                    "string constant has an incompatible result shape",
                )?;
            }
            AssignmentKind::Primitive { op, left, right } => {
                verify_binary_operation(*op, *left, *right, assignment, declared)?;
                uses.extend([*left, *right]);
            }
            AssignmentKind::Unary { op, value } => {
                verify_unary_operation(*op, *value, assignment, declared)?;
                uses.push(*value);
            }
            AssignmentKind::DirectCall {
                function,
                arguments,
            } => {
                let Some(signature) = signatures.get(function) else {
                    return Err(assignment_error(
                        assignment,
                        "direct call references an unknown function",
                    ));
                };
                let callee = functions
                    .and_then(|functions| functions.get(function))
                    .map(|function| function.name.clone())
                    .unwrap_or_else(|| format!("symbol {function:?}"));
                verify_call_shape(
                    assignment,
                    declared,
                    signature,
                    arguments,
                    &callee,
                    function_name,
                    function_span,
                )?;
                uses.extend(arguments.iter().copied());
            }
            AssignmentKind::FunctionRef {
                function,
                signature,
                captures,
            } => {
                let target_signature = table_signature(table, *signature, assignment)?;
                require_destination(
                    declared,
                    assignment,
                    closure_shape(*signature),
                    "function reference has an incompatible result shape",
                )?;
                if let Some(functions) = functions {
                    let Some(target) = functions.get(function) else {
                        return Err(assignment_error(
                            assignment,
                            "function reference targets an unknown function",
                        ));
                    };
                    verify_function_reference(
                        assignment,
                        target,
                        target_signature,
                        captures,
                        declared,
                    )?;
                }
                uses.extend(captures.iter().copied());
            }
            AssignmentKind::IndirectCall {
                function,
                signature,
                arguments,
            } => {
                let signature_id = *signature;
                let signature = table_signature(table, signature_id, assignment)?;
                require_value_shape(declared, *function, closure_shape(signature_id), assignment)?;
                verify_call_shape(
                    assignment,
                    declared,
                    signature,
                    arguments,
                    &format!("indirect call with signature {signature_id:?}"),
                    function_name,
                    function_span,
                )?;
                uses.push(*function);
                uses.extend(arguments.iter().copied());
            }
            AssignmentKind::ClosureGetCapture { closure, .. } => {
                require_value_shape(declared, *closure, aggregate_shape(), assignment)?;
                uses.push(*closure);
            }
            AssignmentKind::RepresentationTest {
                destination,
                value,
                reference,
            } => {
                verify_embedded_destination(assignment, *destination)?;
                require_reference_value(declared, *value, assignment)?;
                require_destination(
                    declared,
                    assignment,
                    ValueShape::Boolean,
                    "representation test must produce Boolean",
                )?;
                verify_reference_handle(table, reference, assignment)?;
                verify_erased_adaptation(assignment, declared, *value, reference)?;
                uses.push(*value);
            }
            AssignmentKind::RepresentationCast {
                destination,
                value,
                reference,
            } => {
                verify_embedded_destination(assignment, *destination)?;
                require_reference_value(declared, *value, assignment)?;
                require_destination(
                    declared,
                    assignment,
                    ValueShape::Reference(*reference),
                    "representation cast has an incompatible result shape",
                )?;
                verify_reference_handle(table, reference, assignment)?;
                verify_erased_adaptation(assignment, declared, *value, reference)?;
                uses.push(*value);
            }
            AssignmentKind::ProductNew {
                destination,
                representation,
                arguments,
            } => {
                verify_embedded_destination(assignment, *destination)?;
                let representation_id = *representation;
                let Some(representation) = table.representation(representation_id) else {
                    return Err(assignment_error(
                        assignment,
                        "unknown representation handle",
                    ));
                };
                let expected = match representation {
                    Representation::Box { value } => std::slice::from_ref(value),
                    Representation::Product { fields } => fields,
                    _ => {
                        return Err(assignment_error(
                            assignment,
                            "product construction requires a product or box representation",
                        ));
                    }
                };
                if arguments.len() != expected.len()
                    || arguments.iter().zip(expected).any(|(argument, expected)| {
                        declared.get(argument).copied() != Some(*expected)
                    })
                {
                    return Err(assignment_error(
                        assignment,
                        "product construction arguments have incompatible shapes",
                    ));
                }
                let allowed = match representation {
                    Representation::Box { .. } => {
                        declared.get(&assignment.destination)
                            == Some(&repr_shape(representation_id))
                    }
                    Representation::Product { .. } => {
                        matches!(
                            declared.get(&assignment.destination),
                            Some(ValueShape::Reference(Reference {
                                nullable: false,
                                heap: RefShape::Aggregate,
                            }))
                        ) || declared.get(&assignment.destination)
                            == Some(&repr_shape(representation_id))
                    }
                    _ => false,
                };
                if !allowed {
                    return Err(assignment_error(
                        assignment,
                        "product construction has an incompatible result shape",
                    ));
                }
                uses.extend(arguments.iter().copied());
            }
            AssignmentKind::ProductGet {
                destination,
                representation,
                field,
                value,
            } => {
                verify_embedded_destination(assignment, *destination)?;
                let representation_id = *representation;
                let Some(representation) = table.representation(representation_id) else {
                    return Err(assignment_error(
                        assignment,
                        "unknown representation handle",
                    ));
                };
                let (expected, source, boxed) = match representation {
                    Representation::Box { value: expected } if *field == 0 => {
                        (*expected, Some(repr_shape(representation_id)), true)
                    }
                    Representation::Product { fields } => {
                        let Some(expected) = fields.get(*field as usize).copied() else {
                            return Err(assignment_error(
                                assignment,
                                "product projection field is out of range",
                            ));
                        };
                        (expected, None, false)
                    }
                    Representation::Box { .. } => {
                        return Err(assignment_error(
                            assignment,
                            "box projection field is out of range",
                        ));
                    }
                    _ => {
                        return Err(assignment_error(
                            assignment,
                            "product projection requires a product or box representation",
                        ));
                    }
                };
                if let Some(source) = source {
                    require_value_shape(declared, *value, source, assignment)?;
                } else {
                    verify_product_value(declared, *value, representation_id, assignment)?;
                }
                // The shared i32 box stores an `Integer` field but is also the
                // erased home of `Boolean`. Their checks are performed
                // explicitly by the boxing/unboxing path, so a projection may
                // declare either i32-family shape.
                let destination_shape = declared.get(&assignment.destination).copied();
                let result_ok = match (boxed, expected) {
                    (true, ValueShape::Integer) => matches!(
                        destination_shape,
                        Some(ValueShape::Integer | ValueShape::Boolean)
                    ),
                    _ => destination_shape == Some(expected),
                };
                if !result_ok {
                    return Err(assignment_error(
                        assignment,
                        "product projection has an incompatible result shape",
                    ));
                }
                uses.push(*value);
            }
            AssignmentKind::VariantNew { .. }
            | AssignmentKind::VariantTag { .. }
            | AssignmentKind::VariantGet { .. } => {
                uses.extend(verify_variant_assignment(assignment, declared, table)?);
            }
            AssignmentKind::ArrayNew { .. }
            | AssignmentKind::ArrayLen { .. }
            | AssignmentKind::ArrayGet { .. }
            | AssignmentKind::ArrayClone { .. }
            | AssignmentKind::ArraySet { .. }
            | AssignmentKind::ArrayAppend { .. }
            | AssignmentKind::ArrayFill { .. } => {
                arrays::verify_array_assignment(assignment, declared, table, &mut uses)?;
            }
            AssignmentKind::StringToBytes {
                representation,
                value,
                ..
            }
            | AssignmentKind::BytesToString {
                representation,
                value,
                ..
            } => {
                let from_string = matches!(assignment.kind, AssignmentKind::StringToBytes { .. });
                string_bytes::verify_conversion(
                    assignment,
                    *representation,
                    *value,
                    declared,
                    table,
                    &mut uses,
                    from_string,
                )?;
            }
            AssignmentKind::If {
                condition,
                then_assignments,
                then_value,
                else_assignments,
                else_value,
            } => {
                require_value_shape(declared, *condition, ValueShape::Boolean, assignment)?;
                uses.push(*condition);
                let mut then_available = available.clone();
                verify_assignments(
                    then_assignments,
                    &mut then_available,
                    declared,
                    signatures,
                    table,
                    functions,
                    function_span,
                    function_name,
                )?;
                let mut else_available = available.clone();
                verify_assignments(
                    else_assignments,
                    &mut else_available,
                    declared,
                    signatures,
                    table,
                    functions,
                    function_span,
                    function_name,
                )?;
                if !then_available.contains(then_value) || !else_available.contains(else_value) {
                    return Err(undef_error(assignment.span, function_span));
                }
                let then_shape = declared_shape(declared, *then_value, assignment)?;
                let else_shape = declared_shape(declared, *else_value, assignment)?;
                if then_shape != else_shape
                    || declared.get(&assignment.destination).copied() != Some(then_shape)
                {
                    return Err(assignment_error(
                        assignment,
                        "if branches have incompatible result shapes",
                    ));
                }
            }
            AssignmentKind::TagSwitch {
                value,
                cases,
                default_assignments,
                default_value,
            } => {
                verify_tag_switch(
                    assignment,
                    *value,
                    cases,
                    default_assignments,
                    *default_value,
                    available,
                    declared,
                    signatures,
                    table,
                    functions,
                    function_span,
                    function_name,
                )?;
                uses.push(*value);
            }
            AssignmentKind::Unreachable => {}
        }
        if uses.iter().any(|value| !available.contains(value)) {
            return Err(undef_error(assignment.span, function_span));
        }
        if !declared.contains_key(&assignment.destination) {
            return Err(assignment_error(
                assignment,
                "CC assignment destination has no value declaration",
            ));
        }
        let preserves_existing_value = matches!(
            &assignment.kind,
            AssignmentKind::ArraySet {
                destination,
                value,
                ..
            } if *destination == *value && *destination == assignment.destination
        );
        if !preserves_existing_value && !available.insert(assignment.destination) {
            return Err(assignment_error(
                assignment,
                "CC assignment redefines a value",
            ));
        }
    }
    Ok(())
}
