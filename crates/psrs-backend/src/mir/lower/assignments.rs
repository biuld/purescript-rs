use super::*;

impl FunctionLowerer<'_> {
    pub(super) fn lower_assignments(
        &mut self,
        assignments: &[cc::Assignment],
        mut current: BlockId,
    ) -> Result<BlockId, Vec<BackendError>> {
        for assignment in assignments {
            match &assignment.kind {
                AssignmentKind::AggregateConvert {
                    value, conversion, ..
                } => {
                    current = self.lower_aggregate_convert(
                        current,
                        *value,
                        assignment.destination,
                        conversion,
                        assignment.span,
                    )?;
                }
                AssignmentKind::Constant(value) => self.append_instruction(
                    current,
                    Instruction::Constant {
                        destination: assignment.destination,
                        value: *value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::NumberConstant(value) => self.append_instruction(
                    current,
                    Instruction::NumberConstant {
                        destination: assignment.destination,
                        value: value.clone(),
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::StringConstant(bytes) => self.append_instruction(
                    current,
                    Instruction::StringConstant {
                        destination: assignment.destination,
                        bytes: bytes.clone(),
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::Primitive { op, left, right } => {
                    let instruction = self.scalar_helpers.binary_instruction(
                        *op,
                        assignment.destination,
                        *left,
                        *right,
                        assignment.span,
                    )?;
                    self.append_instruction(current, instruction, assignment.span)?;
                }
                AssignmentKind::Unary { op, value } => self.append_instruction(
                    current,
                    Instruction::UnaryPrimitive {
                        destination: assignment.destination,
                        op: (*op).into(),
                        value: *value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::RepresentationTest {
                    destination,
                    value,
                    reference,
                } => self.append_instruction(
                    current,
                    Instruction::RefTest {
                        destination: *destination,
                        value: *value,
                        reference: self
                            .layout
                            .reference(reference)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::RepresentationCast {
                    destination,
                    value,
                    reference,
                } => self.append_instruction(
                    current,
                    Instruction::RefCast {
                        destination: *destination,
                        value: *value,
                        reference: self
                            .layout
                            .reference(reference)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::ProductNew {
                    destination,
                    representation,
                    arguments,
                } => self.append_instruction(
                    current,
                    Instruction::StructNew {
                        destination: *destination,
                        type_index: self
                            .layout
                            .repr_index(*representation)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        arguments: arguments.clone(),
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::ProductGet {
                    destination,
                    representation,
                    field,
                    value,
                } => self.append_instruction(
                    current,
                    Instruction::StructGet {
                        destination: *destination,
                        type_index: self
                            .layout
                            .repr_index(*representation)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        field: *field,
                        value: *value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::VariantNew { .. }
                | AssignmentKind::VariantTag { .. }
                | AssignmentKind::VariantGet { .. } => {
                    self.lower_variant(assignment, current)?;
                }
                AssignmentKind::ArrayNew {
                    destination,
                    representation,
                    elements,
                } => self.append_instruction(
                    current,
                    Instruction::ArrayNew {
                        destination: *destination,
                        type_index: self
                            .layout
                            .repr_index(*representation)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        elements: elements.clone(),
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::ArrayLen { destination, value } => self.append_instruction(
                    current,
                    Instruction::ArrayLen {
                        destination: *destination,
                        value: *value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::ArrayGet {
                    destination,
                    representation,
                    value,
                    index,
                } => {
                    let type_index = self
                        .layout
                        .repr_index(*representation)
                        .map_err(|error| layout_error(assignment.span, error))?;
                    let destination_type = self
                        .values
                        .iter()
                        .find(|candidate| candidate.id == *destination)
                        .map(|candidate| candidate.ty)
                        .ok_or_else(|| {
                            vec![BackendError::new(
                                "P9 MIR lowering",
                                assignment.span,
                                "array.get destination has no value declaration",
                            )]
                        })?;
                    if let ValueType::Ref(reference) = destination_type
                        && !reference.nullable
                    {
                        let temporary = self.fresh(ValueType::Ref(crate::types::RefType {
                            nullable: true,
                            heap: reference.heap,
                        }));
                        self.append_instruction(
                            current,
                            Instruction::ArrayGet {
                                destination: temporary,
                                type_index,
                                value: *value,
                                index: *index,
                                span: assignment.span,
                            },
                            assignment.span,
                        )?;
                        self.append_instruction(
                            current,
                            Instruction::RefCast {
                                destination: *destination,
                                value: temporary,
                                reference,
                                span: assignment.span,
                            },
                            assignment.span,
                        )?;
                    } else {
                        self.append_instruction(
                            current,
                            Instruction::ArrayGet {
                                destination: *destination,
                                type_index,
                                value: *value,
                                index: *index,
                                span: assignment.span,
                            },
                            assignment.span,
                        )?;
                    }
                }
                AssignmentKind::ArrayClone {
                    destination,
                    representation,
                    value,
                } => self.append_instruction(
                    current,
                    Instruction::ArrayClone {
                        destination: *destination,
                        type_index: self
                            .layout
                            .repr_index(*representation)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        value: *value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::ArraySet {
                    representation,
                    value,
                    index,
                    new_value,
                    ..
                } => self.append_instruction(
                    current,
                    Instruction::ArraySet {
                        type_index: self
                            .layout
                            .repr_index(*representation)
                            .map_err(|error| layout_error(assignment.span, error))?,
                        value: *value,
                        index: *index,
                        new_value: *new_value,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
                AssignmentKind::DirectCall {
                    function,
                    arguments,
                } => {
                    if let Some(import) = self.wit_imports.get(function).cloned() {
                        wit::lower(
                            self,
                            &import.import,
                            &import.signature,
                            assignment.destination,
                            arguments,
                            assignment.span,
                            current,
                        )?;
                    } else {
                        self.append_instruction(
                            current,
                            Instruction::Call {
                                destination: assignment.destination,
                                function: *function,
                                arguments: arguments.clone(),
                                span: assignment.span,
                            },
                            assignment.span,
                        )?;
                    }
                }
                AssignmentKind::FunctionRef {
                    function,
                    signature,
                    captures,
                } => {
                    let (closure_type, capture_array_type) = self
                        .layout
                        .closure_layout()
                        .map_err(|error| layout_error(assignment.span, error))?;
                    self.append_instruction(
                        current,
                        Instruction::ClosureNew {
                            destination: assignment.destination,
                            function: *function,
                            type_index: self
                                .layout
                                .signature_index(*signature)
                                .map_err(|error| layout_error(assignment.span, error))?,
                            closure_type,
                            capture_array_type,
                            boxed_integer_type: self.layout.boxed_integer_index(),
                            boxed_f64_type: self.layout.boxed_number_index(),
                            captures: captures.clone(),
                            span: assignment.span,
                        },
                        assignment.span,
                    )?
                }
                AssignmentKind::IndirectCall {
                    function,
                    signature,
                    arguments,
                } => {
                    let (closure_type, capture_array_type) = self
                        .layout
                        .closure_layout()
                        .map_err(|error| layout_error(assignment.span, error))?;
                    self.append_instruction(
                        current,
                        Instruction::ClosureCall {
                            destination: assignment.destination,
                            function: *function,
                            type_index: self
                                .layout
                                .signature_index(*signature)
                                .map_err(|error| layout_error(assignment.span, error))?,
                            closure_type,
                            capture_array_type,
                            arguments: arguments.clone(),
                            span: assignment.span,
                        },
                        assignment.span,
                    )?
                }
                AssignmentKind::ClosureGetCapture { closure, index } => {
                    let (closure_type, capture_array_type) = self
                        .layout
                        .closure_layout()
                        .map_err(|error| layout_error(assignment.span, error))?;
                    self.append_instruction(
                        current,
                        Instruction::ClosureGetCapture {
                            destination: assignment.destination,
                            closure: *closure,
                            closure_type,
                            capture_array_type,
                            boxed_integer_type: self.layout.boxed_integer_index(),
                            boxed_f64_type: self.layout.boxed_number_index(),
                            index: *index,
                            span: assignment.span,
                        },
                        assignment.span,
                    )?
                }
                AssignmentKind::If {
                    condition,
                    then_assignments,
                    then_value,
                    else_assignments,
                    else_value,
                } => {
                    let then_block = self.new_block(Vec::new());
                    let else_block = self.new_block(Vec::new());
                    let merge = self.new_block(vec![assignment.destination]);
                    self.set_terminator(
                        current,
                        Terminator::Branch {
                            condition: *condition,
                            then_block,
                            else_block,
                            merge_block: merge,
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                    let then_end = self.lower_assignments(then_assignments, then_block)?;
                    self.set_terminator(
                        then_end,
                        Terminator::Jump {
                            target: merge,
                            arguments: vec![*then_value],
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                    let else_end = self.lower_assignments(else_assignments, else_block)?;
                    self.set_terminator(
                        else_end,
                        Terminator::Jump {
                            target: merge,
                            arguments: vec![*else_value],
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                    current = merge;
                }
                AssignmentKind::TagSwitch {
                    value,
                    cases,
                    default_assignments,
                    default_value,
                } => {
                    let case_blocks = cases
                        .iter()
                        .map(|_| self.new_block(Vec::new()))
                        .collect::<Vec<_>>();
                    let default_block = self.new_block(Vec::new());
                    let merge = self.new_block(vec![assignment.destination]);
                    self.set_terminator(
                        current,
                        Terminator::Switch {
                            value: *value,
                            cases: cases
                                .iter()
                                .zip(&case_blocks)
                                .map(|(case, block)| (case.tag, *block))
                                .collect(),
                            default: default_block,
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                    for (case, block) in cases.iter().zip(case_blocks) {
                        let end = self.lower_assignments(&case.assignments, block)?;
                        self.set_terminator(
                            end,
                            Terminator::Jump {
                                target: merge,
                                arguments: vec![case.value],
                                span: assignment.span,
                            },
                            assignment.span,
                        )?;
                    }
                    let end = self.lower_assignments(default_assignments, default_block)?;
                    self.set_terminator(
                        end,
                        Terminator::Jump {
                            target: merge,
                            arguments: vec![*default_value],
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                    current = merge;
                }
                AssignmentKind::Unreachable => self.append_instruction(
                    current,
                    Instruction::Unreachable {
                        destination: assignment.destination,
                        span: assignment.span,
                    },
                    assignment.span,
                )?,
            }
        }
        Ok(current)
    }
}
