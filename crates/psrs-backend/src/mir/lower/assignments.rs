use super::*;

impl FunctionLowerer<'_> {
    pub(super) fn lower_assignments(
        &mut self,
        assignments: &[cc::Assignment],
        mut current: BlockId,
    ) -> Result<BlockId, Vec<BackendError>> {
        for assignment in assignments {
            if self.is_terminated(current, assignment.span)? {
                break;
            }
            match &assignment.kind {
                AssignmentKind::Protect {
                    body,
                    exception,
                    handler,
                } => {
                    current = self.lower_protect(
                        current,
                        assignment.destination,
                        body,
                        *exception,
                        handler,
                        assignment.span,
                    )?;
                }
                AssignmentKind::ControlEvidence {
                    value,
                    implementation,
                    contract,
                    arguments,
                } => {
                    let proof = self
                        .layout
                        .representation_table()
                        .control_contracts()
                        .get(contract.0 as usize)
                        .ok_or_else(|| {
                            vec![BackendError::invalid_ir(
                                "P9 MIR lowering",
                                assignment.span,
                                "control evidence has no checked source contract",
                            )]
                        })?;
                    if proof.arguments().len() != arguments.len() {
                        return Err(vec![BackendError::invalid_ir(
                            "P9 MIR lowering",
                            assignment.span,
                            "control evidence argument count disagrees with its checked source contract",
                        )]);
                    }
                    self.append_instruction(
                        current,
                        Instruction::ControlEvidence {
                            destination: assignment.destination,
                            value: *value,
                            implementation: *implementation,
                            arguments: arguments.clone(),
                            contract: *contract,
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                }
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
                AssignmentKind::StringConstant(bytes) => {
                    let string_type = self.layout.string_index().ok_or_else(|| {
                        layout_error(assignment.span, LayoutError::UnknownRepresentation)
                    })?;
                    let data_index = self
                        .literals
                        .as_mut()
                        .ok_or_else(|| {
                            vec![BackendError::invalid_ir(
                                "P9 MIR lowering",
                                assignment.span,
                                "string literal lowering has no literal pool",
                            )]
                        })?
                        .intern(bytes);
                    self.append_instruction(
                        current,
                        Instruction::ArrayNewData {
                            destination: assignment.destination,
                            type_index: string_type,
                            data_index,
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                }
                AssignmentKind::Primitive { op, left, right } => {
                    if *op == cc::BinaryOp::StringEq {
                        current = self.lower_string_eq(
                            current,
                            assignment.destination,
                            *left,
                            *right,
                            assignment.span,
                        )?;
                        continue;
                    }
                    let operation = super::super::NumericOp::try_from(*op).map_err(|op| {
                        vec![BackendError::invalid_ir(
                            "P9 MIR lowering",
                            assignment.span,
                            format!("unsupported scalar operation {op:?}"),
                        )]
                    })?;
                    let instruction = Instruction::Primitive {
                        destination: assignment.destination,
                        op: operation,
                        left: *left,
                        right: *right,
                        span: assignment.span,
                    };
                    self.append_instruction(current, instruction, assignment.span)?;
                }
                AssignmentKind::RuntimeCall {
                    intrinsic,
                    arguments,
                } => {
                    self.lower_runtime_call(
                        current,
                        assignment.destination,
                        *intrinsic,
                        arguments,
                        assignment.span,
                    )?;
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
                AssignmentKind::Record { .. } => {
                    current = self.lower_record_assignment(current, assignment)?;
                }
                AssignmentKind::RecordPrimitive { .. } => {
                    current = self.lower_record_primitive_assignment(current, assignment)?;
                }
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
                AssignmentKind::ArrayNew { .. }
                | AssignmentKind::ArrayLen { .. }
                | AssignmentKind::ArrayAppend { .. }
                | AssignmentKind::ArrayFill { .. }
                | AssignmentKind::StringToBytes { .. }
                | AssignmentKind::BytesToString { .. }
                | AssignmentKind::ArrayGet { .. }
                | AssignmentKind::ArrayClone { .. }
                | AssignmentKind::ArraySet { .. } => {
                    current = self.lower_array_assignment(current, assignment)?;
                }
                AssignmentKind::DirectCall {
                    contract,
                    function,
                    arguments,
                } => {
                    self.layout
                        .representation_table()
                        .verify_control_call(*function, *contract)
                        .map_err(|message| {
                            vec![BackendError::invalid_ir(
                                "P9 MIR lowering",
                                assignment.span,
                                message,
                            )]
                        })?;
                    if let Some(import) = self.wit_imports.get(function).cloned() {
                        current = self.lower_wit_call(&import, assignment, current)?;
                    } else if let Some(import) = self.runtime.static_cells.get(function).cloned() {
                        self.lower_static_cell(
                            current,
                            assignment.destination,
                            &import,
                            arguments,
                            assignment.span,
                        )?;
                    } else if let Some(import) = self.runtime.imports.get(function).cloned() {
                        self.lower_storage_call(
                            current,
                            assignment.destination,
                            &import,
                            arguments,
                            assignment.span,
                        )?;
                    } else if let Some(provider) =
                        self.runtime.exception_providers.get(function).cloned()
                    {
                        current = self.lower_exception_call(
                            current,
                            assignment.destination,
                            &provider,
                            arguments,
                            assignment.span,
                        )?;
                    } else {
                        self.append_instruction(
                            current,
                            Instruction::Call {
                                contract: *contract,
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
                    let merge = self.new_block(self.control_values(assignment.destination));
                    self.set_terminator(
                        current,
                        Terminator::Branch {
                            condition: *condition,
                            then_block,
                            else_block,
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                    let then_end = self.lower_assignments(then_assignments, then_block)?;
                    self.terminate_if_open(
                        then_end,
                        Terminator::Jump {
                            target: merge,
                            arguments: self.control_values(*then_value),
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                    let else_end = self.lower_assignments(else_assignments, else_block)?;
                    self.terminate_if_open(
                        else_end,
                        Terminator::Jump {
                            target: merge,
                            arguments: self.control_values(*else_value),
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
                    let merge = self.new_block(self.control_values(assignment.destination));
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
                        self.terminate_if_open(
                            end,
                            Terminator::Jump {
                                target: merge,
                                arguments: self.control_values(case.value),
                                span: assignment.span,
                            },
                            assignment.span,
                        )?;
                    }
                    let end = self.lower_assignments(default_assignments, default_block)?;
                    self.terminate_if_open(
                        end,
                        Terminator::Jump {
                            target: merge,
                            arguments: self.control_values(*default_value),
                            span: assignment.span,
                        },
                        assignment.span,
                    )?;
                    current = merge;
                }
                AssignmentKind::RaiseSourceError { message } => {
                    self.lower_source_error(current, *message, assignment.span)?;
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
