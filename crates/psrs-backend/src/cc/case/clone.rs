use super::super::{Assignment, AssignmentKind, ValueId};
use super::FunctionLowerer;
use std::collections::HashMap;

pub(super) trait AssignmentCloning {
    fn clone_assignments(
        &mut self,
        assignments: &[Assignment],
        result: ValueId,
    ) -> (Vec<Assignment>, ValueId);
}

impl AssignmentCloning for FunctionLowerer<'_> {
    fn clone_assignments(
        &mut self,
        assignments: &[Assignment],
        result: ValueId,
    ) -> (Vec<Assignment>, ValueId) {
        let mut mapping = HashMap::new();
        let cloned = self.clone_list(assignments, &mut mapping);
        (cloned, remap(result, &mapping))
    }
}

impl FunctionLowerer<'_> {
    fn clone_list(
        &mut self,
        assignments: &[Assignment],
        mapping: &mut HashMap<ValueId, ValueId>,
    ) -> Vec<Assignment> {
        assignments
            .iter()
            .map(|assignment| {
                let preserves_value = matches!(assignment.kind, AssignmentKind::ArraySet { .. });
                let destination = if preserves_value {
                    remap(assignment.destination, mapping)
                } else {
                    let ty = self
                        .values
                        .iter()
                        .find(|value| value.id == assignment.destination)
                        .map(|value| value.ty)
                        .expect("CC assignment destination has a value type");
                    let destination = self.fresh(ty);
                    mapping.insert(assignment.destination, destination);
                    destination
                };
                Assignment {
                    destination,
                    kind: self.clone_kind(&assignment.kind, mapping),
                    span: assignment.span,
                }
            })
            .collect()
    }

    fn clone_kind(
        &mut self,
        kind: &AssignmentKind,
        mapping: &mut HashMap<ValueId, ValueId>,
    ) -> AssignmentKind {
        match kind {
            AssignmentKind::Constant(value) => AssignmentKind::Constant(*value),
            AssignmentKind::NumberConstant(value) => AssignmentKind::NumberConstant(value.clone()),
            AssignmentKind::StringConstant(value) => AssignmentKind::StringConstant(value.clone()),
            AssignmentKind::Primitive { op, left, right } => AssignmentKind::Primitive {
                op: *op,
                left: remap(*left, mapping),
                right: remap(*right, mapping),
            },
            AssignmentKind::Unary { op, value } => AssignmentKind::Unary {
                op: *op,
                value: remap(*value, mapping),
            },
            AssignmentKind::DirectCall {
                function,
                arguments,
            } => AssignmentKind::DirectCall {
                function: *function,
                arguments: arguments
                    .iter()
                    .map(|value| remap(*value, mapping))
                    .collect(),
            },
            AssignmentKind::FunctionRef {
                function,
                signature,
                captures,
            } => AssignmentKind::FunctionRef {
                function: *function,
                signature: *signature,
                captures: captures
                    .iter()
                    .map(|value| remap(*value, mapping))
                    .collect(),
            },
            AssignmentKind::IndirectCall {
                function,
                signature,
                arguments,
            } => AssignmentKind::IndirectCall {
                function: remap(*function, mapping),
                signature: *signature,
                arguments: arguments
                    .iter()
                    .map(|value| remap(*value, mapping))
                    .collect(),
            },
            AssignmentKind::ClosureGetCapture { closure, index } => {
                AssignmentKind::ClosureGetCapture {
                    closure: remap(*closure, mapping),
                    index: *index,
                }
            }
            AssignmentKind::RepresentationTest {
                destination,
                value,
                reference,
            } => AssignmentKind::RepresentationTest {
                destination: remap(*destination, mapping),
                value: remap(*value, mapping),
                reference: *reference,
            },
            AssignmentKind::RepresentationCast {
                destination,
                value,
                reference,
            } => AssignmentKind::RepresentationCast {
                destination: remap(*destination, mapping),
                value: remap(*value, mapping),
                reference: *reference,
            },
            AssignmentKind::ProductNew {
                destination,
                representation,
                arguments,
            } => AssignmentKind::ProductNew {
                destination: remap(*destination, mapping),
                representation: *representation,
                arguments: arguments
                    .iter()
                    .map(|value| remap(*value, mapping))
                    .collect(),
            },
            AssignmentKind::ProductGet {
                destination,
                representation,
                field,
                value,
            } => AssignmentKind::ProductGet {
                destination: remap(*destination, mapping),
                representation: *representation,
                field: *field,
                value: remap(*value, mapping),
            },
            AssignmentKind::VariantNew {
                destination,
                representation,
                case,
                fields,
            } => AssignmentKind::VariantNew {
                destination: remap(*destination, mapping),
                representation: *representation,
                case: *case,
                fields: fields.iter().map(|value| remap(*value, mapping)).collect(),
            },
            AssignmentKind::VariantTag {
                destination,
                representation,
                value,
            } => AssignmentKind::VariantTag {
                destination: remap(*destination, mapping),
                representation: *representation,
                value: remap(*value, mapping),
            },
            AssignmentKind::VariantGet {
                destination,
                representation,
                case,
                field,
                value,
            } => AssignmentKind::VariantGet {
                destination: remap(*destination, mapping),
                representation: *representation,
                case: *case,
                field: *field,
                value: remap(*value, mapping),
            },
            AssignmentKind::ArrayNew {
                destination,
                representation,
                elements,
            } => AssignmentKind::ArrayNew {
                destination: remap(*destination, mapping),
                representation: *representation,
                elements: elements
                    .iter()
                    .map(|value| remap(*value, mapping))
                    .collect(),
            },
            AssignmentKind::ArrayLen { destination, value } => AssignmentKind::ArrayLen {
                destination: remap(*destination, mapping),
                value: remap(*value, mapping),
            },
            AssignmentKind::ArrayGet {
                destination,
                representation,
                value,
                index,
            } => AssignmentKind::ArrayGet {
                destination: remap(*destination, mapping),
                representation: *representation,
                value: remap(*value, mapping),
                index: remap(*index, mapping),
            },
            AssignmentKind::ArrayClone {
                destination,
                representation,
                value,
            } => AssignmentKind::ArrayClone {
                destination: remap(*destination, mapping),
                representation: *representation,
                value: remap(*value, mapping),
            },
            AssignmentKind::ArraySet {
                destination,
                representation,
                value,
                index,
                new_value,
            } => AssignmentKind::ArraySet {
                destination: remap(*destination, mapping),
                representation: *representation,
                value: remap(*value, mapping),
                index: remap(*index, mapping),
                new_value: remap(*new_value, mapping),
            },
            AssignmentKind::If {
                condition,
                then_assignments,
                then_value,
                else_assignments,
                else_value,
            } => AssignmentKind::If {
                condition: remap(*condition, mapping),
                then_assignments: self.clone_list(then_assignments, mapping),
                then_value: remap(*then_value, mapping),
                else_assignments: self.clone_list(else_assignments, mapping),
                else_value: remap(*else_value, mapping),
            },
        }
    }
}

fn remap(value: ValueId, mapping: &HashMap<ValueId, ValueId>) -> ValueId {
    mapping.get(&value).copied().unwrap_or(value)
}
