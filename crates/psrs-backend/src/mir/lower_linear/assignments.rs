use super::*;
use crate::cc::{self, AssignmentKind, RefShape, ValueShape};

impl LinearFunctionLowerer<'_> {
    pub(super) fn lower_assignments(
        &mut self,
        assignments: &[cc::Assignment],
        mut current: BlockId,
    ) -> Result<BlockId, Vec<BackendError>> {
        for assignment in assignments {
            let span = assignment.span;
            match &assignment.kind {
                AssignmentKind::Constant(value) => self.append(
                    current,
                    Instruction::Constant {
                        destination: assignment.destination,
                        value: *value,
                        span,
                    },
                    span,
                )?,
                AssignmentKind::NumberConstant(value) => self.append(
                    current,
                    Instruction::NumberConstant {
                        destination: assignment.destination,
                        value: value.clone(),
                        span,
                    },
                    span,
                )?,
                AssignmentKind::StringConstant(bytes) => self.append(
                    current,
                    Instruction::StringConstant {
                        destination: assignment.destination,
                        bytes: bytes.clone(),
                        span,
                    },
                    span,
                )?,
                AssignmentKind::Primitive { op, left, right } => {
                    let instruction = self.scalar_helpers.binary_instruction(
                        *op,
                        assignment.destination,
                        *left,
                        *right,
                        span,
                    )?;
                    self.append(current, instruction, span)?;
                }
                AssignmentKind::Unary { op, value } => self.append(
                    current,
                    Instruction::UnaryPrimitive {
                        destination: assignment.destination,
                        op: (*op).into(),
                        value: *value,
                        span,
                    },
                    span,
                )?,
                AssignmentKind::ProductNew {
                    destination,
                    representation,
                    arguments,
                } => {
                    let bytes = self
                        .layout
                        .representation_size(*representation)
                        .map_err(|error| layout_error(span, error))?;
                    self.append(
                        current,
                        Instruction::LinearAlloc {
                            destination: *destination,
                            bytes,
                            alignment: self
                                .layout
                                .representation_alignment(*representation)
                                .map_err(|error| layout_error(span, error))?,
                            span,
                        },
                        span,
                    )?;
                    for (field, argument) in arguments.iter().enumerate() {
                        let (offset, shape) = self
                            .layout
                            .field(*representation, field as u32)
                            .map_err(|error| layout_error(span, error))?;
                        self.append(
                            current,
                            Instruction::LinearStore {
                                memory: crate::types::MemoryId(0),
                                alignment: LinearMemoryLayout::value_alignment(shape),
                                address: *destination,
                                value: *argument,
                                offset,
                                object_bytes: bytes,
                                ty: LinearMemoryLayout::value_type(shape),
                                span,
                            },
                            span,
                        )?;
                    }
                }
                AssignmentKind::ProductGet {
                    destination,
                    representation,
                    field,
                    value,
                } => {
                    let object_bytes = self
                        .layout
                        .representation_size(*representation)
                        .map_err(|error| layout_error(span, error))?;
                    let (offset, shape) = self
                        .layout
                        .field(*representation, *field)
                        .map_err(|error| layout_error(span, error))?;
                    self.append(
                        current,
                        Instruction::LinearLoad {
                            memory: crate::types::MemoryId(0),
                            alignment: LinearMemoryLayout::value_alignment(shape),
                            destination: *destination,
                            address: *value,
                            offset,
                            object_bytes,
                            ty: LinearMemoryLayout::value_type(shape),
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::VariantNew { .. }
                | AssignmentKind::VariantTag { .. }
                | AssignmentKind::VariantGet { .. } => self.lower_variant(assignment, current)?,
                AssignmentKind::ArrayNew {
                    destination,
                    representation,
                    elements,
                } => {
                    let (element, stride, element_offset, alignment) = self
                        .layout
                        .array(*representation)
                        .map_err(|error| layout_error(span, error))?;
                    let length = u32::try_from(elements.len()).map_err(|_| {
                        unsupported(span, "linear-memory array has too many elements")
                    })?;
                    let bytes = stride
                        .checked_mul(length)
                        .and_then(|payload| element_offset.checked_add(payload))
                        .ok_or_else(|| {
                            unsupported(span, "linear-memory array allocation is too large")
                        })?;
                    self.append(
                        current,
                        Instruction::LinearAlloc {
                            destination: *destination,
                            bytes,
                            alignment,
                            span,
                        },
                        span,
                    )?;
                    let length = self.fresh(ValueType::I32);
                    self.append(
                        current,
                        Instruction::Constant {
                            destination: length,
                            value: i32::try_from(elements.len()).map_err(|_| {
                                unsupported(span, "linear-memory array length exceeds i32")
                            })?,
                            span,
                        },
                        span,
                    )?;
                    self.append(
                        current,
                        Instruction::LinearStore {
                            memory: crate::types::MemoryId(0),
                            alignment: 4,
                            address: *destination,
                            value: length,
                            offset: 0,
                            object_bytes: bytes,
                            ty: ValueType::I32,
                            span,
                        },
                        span,
                    )?;
                    for (index, element_value) in elements.iter().enumerate() {
                        self.append(
                            current,
                            Instruction::LinearStore {
                                memory: crate::types::MemoryId(0),
                                alignment,
                                address: *destination,
                                value: *element_value,
                                offset: element_offset + stride * index as u32,
                                object_bytes: bytes,
                                ty: LinearMemoryLayout::value_type(element),
                                span,
                            },
                            span,
                        )?;
                    }
                }
                AssignmentKind::ArrayLen { destination, value } => {
                    let shape = self.shape(*value, span)?;
                    let ValueShape::Reference(reference) = shape else {
                        return Err(unsupported(
                            span,
                            "linear-memory array.len requires an array pointer",
                        ));
                    };
                    let RefShape::Repr(representation) = reference.heap else {
                        return Err(unsupported(
                            span,
                            "linear-memory array.len requires an array representation",
                        ));
                    };
                    let (_, _, object_bytes, _) = self
                        .layout
                        .array(representation)
                        .map_err(|error| layout_error(span, error))?;
                    self.append(
                        current,
                        Instruction::LinearLoad {
                            memory: crate::types::MemoryId(0),
                            alignment: 4,
                            destination: *destination,
                            address: *value,
                            offset: 0,
                            object_bytes,
                            ty: ValueType::I32,
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::ArrayGet {
                    destination,
                    representation,
                    value,
                    index,
                } => {
                    let (element, stride, element_offset, alignment) = self
                        .layout
                        .array(*representation)
                        .map_err(|error| layout_error(span, error))?;
                    helpers::check_array_index(
                        self,
                        current,
                        *value,
                        *index,
                        element_offset,
                        span,
                    )?;
                    let address =
                        self.index_address(current, *value, *index, stride, element_offset, span)?;
                    self.append(
                        current,
                        Instruction::LinearLoad {
                            memory: crate::types::MemoryId(0),
                            alignment,
                            destination: *destination,
                            address,
                            offset: 0,
                            object_bytes: stride,
                            ty: LinearMemoryLayout::value_type(element),
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::ArrayClone {
                    destination,
                    representation,
                    value,
                } => {
                    self.lower_array_clone(current, *destination, *representation, *value, span)?
                }
                AssignmentKind::ArraySet {
                    representation,
                    value,
                    index,
                    new_value,
                    ..
                } => {
                    let (element, stride, element_offset, alignment) = self
                        .layout
                        .array(*representation)
                        .map_err(|error| layout_error(span, error))?;
                    helpers::check_array_index(
                        self,
                        current,
                        *value,
                        *index,
                        element_offset,
                        span,
                    )?;
                    let address =
                        self.index_address(current, *value, *index, stride, element_offset, span)?;
                    self.append(
                        current,
                        Instruction::LinearStore {
                            memory: crate::types::MemoryId(0),
                            alignment,
                            address,
                            value: *new_value,
                            offset: 0,
                            object_bytes: stride,
                            ty: LinearMemoryLayout::value_type(element),
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::DirectCall {
                    function,
                    arguments,
                } => {
                    if let Some(import) = self.wit_imports.get(function).cloned() {
                        crate::mir::wit::lower(
                            self,
                            &import.import,
                            &import.signature,
                            assignment.destination,
                            arguments,
                            span,
                            current,
                        )?;
                    } else {
                        self.append(
                            current,
                            Instruction::Call {
                                destination: assignment.destination,
                                function: *function,
                                arguments: arguments.clone(),
                                span,
                            },
                            span,
                        )?;
                    }
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
                            span,
                        },
                        span,
                    )?;
                    let then_end = self.lower_assignments(then_assignments, then_block)?;
                    self.set_terminator(
                        then_end,
                        Terminator::Jump {
                            target: merge,
                            arguments: vec![*then_value],
                            span,
                        },
                        span,
                    )?;
                    let else_end = self.lower_assignments(else_assignments, else_block)?;
                    self.set_terminator(
                        else_end,
                        Terminator::Jump {
                            target: merge,
                            arguments: vec![*else_value],
                            span,
                        },
                        span,
                    )?;
                    current = merge;
                }
                AssignmentKind::FunctionRef {
                    function,
                    signature,
                    captures,
                } => {
                    let table_slot = self.table_slots.get(function).copied().ok_or_else(|| {
                        unsupported(span, "linear closure target has no function-table slot")
                    })?;
                    let type_index = self
                        .layout
                        .signature_index(*signature)
                        .map_err(|error| layout_error(span, error))?;
                    let (capture_offsets, allocation_bytes) =
                        helpers::closure_layout(captures.len(), span)?;
                    self.append(
                        current,
                        Instruction::LinearClosureNew {
                            destination: assignment.destination,
                            function: *function,
                            table_slot,
                            type_index,
                            captures: captures.clone(),
                            capture_offsets,
                            allocation_bytes,
                            allocation_alignment: 8,
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::IndirectCall {
                    function,
                    signature,
                    arguments,
                } => {
                    let type_index = self
                        .layout
                        .signature_index(*signature)
                        .map_err(|error| layout_error(span, error))?;
                    self.append(
                        current,
                        Instruction::LinearClosureCall {
                            destination: assignment.destination,
                            function: *function,
                            type_index,
                            arguments: arguments.clone(),
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::ClosureGetCapture { closure, index } => {
                    let shape = self.shape(assignment.destination, span)?;
                    let offset = helpers::closure_capture_offset(*index, span)?;
                    let ty = LinearMemoryLayout::value_type(shape);
                    let object_bytes = offset
                        .checked_add(LinearMemoryLayout::value_bytes(shape))
                        .ok_or_else(|| {
                        super::unsupported(span, "linear closure capture bound overflows")
                    })?;
                    self.append(
                        current,
                        Instruction::LinearClosureGetCapture {
                            destination: assignment.destination,
                            closure: *closure,
                            offset,
                            object_bytes,
                            ty,
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::RepresentationCast {
                    destination, value, ..
                } => {
                    if !matches!(self.shape(*value, span)?, ValueShape::Reference(_))
                        || !matches!(self.shape(*destination, span)?, ValueShape::Reference(_))
                    {
                        return Err(unsupported(span, "linear erased cast requires references"));
                    }
                    self.append(
                        current,
                        Instruction::Copy {
                            destination: *destination,
                            value: *value,
                            span,
                        },
                        span,
                    )?;
                }
                AssignmentKind::RepresentationTest { .. } => {
                    return Err(unsupported(
                        span,
                        "linear-memory target does not support erased representation tests yet",
                    ));
                }
            }
        }
        Ok(current)
    }
}
