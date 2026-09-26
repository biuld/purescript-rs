use super::*;

impl Structurer<'_> {
    pub(super) fn emit_block_instructions(
        &self,
        instructions: &[MirInstruction],
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        for instruction in instructions {
            match instruction {
                MirInstruction::Copy {
                    destination,
                    value,
                    span,
                } => {
                    self.load(body, *value, *span)?;
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::Constant {
                    destination,
                    value,
                    span,
                } => {
                    body.push(Op::Leaf(Instruction::I32Const(*value)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::NumberConstant {
                    destination,
                    value,
                    span,
                } => {
                    let value = value
                        .parse::<f64>()
                        .map_err(|_| wasm_error(*span, "invalid Number literal in MIR"))?;
                    body.push(Op::Leaf(Instruction::F64Const(value.into())));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::ArrayNewData {
                    destination,
                    type_index,
                    data_index,
                    span,
                } => {
                    let length = self
                        .string_lengths
                        .get(data_index)
                        .copied()
                        .ok_or_else(|| wasm_error(*span, "string literal has no data segment"))?;
                    body.push(Op::Leaf(Instruction::I32Const(0)));
                    body.push(Op::Leaf(Instruction::I32Const(length as i32)));
                    body.push(Op::Leaf(Instruction::ArrayNewData {
                        array_type_index: type_index.0,
                        array_data_index: data_index.0,
                    }));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::Primitive {
                    destination,
                    op,
                    left,
                    right,
                    span,
                } => {
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *left,
                        *span,
                    )?)));
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *right,
                        *span,
                    )?)));
                    body.push(Op::Leaf(primitive(*op)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::UnaryPrimitive {
                    destination,
                    op,
                    value,
                    span,
                } => self.emit_unary_primitive(*destination, *op, *value, *span, body)?,
                instruction @ MirInstruction::TrapIf { .. } => self.trap_if(body, instruction)?,
                MirInstruction::Unreachable { .. } => {
                    body.push(Op::Leaf(Instruction::Unreachable));
                }
                MirInstruction::Call {
                    destination,
                    function,
                    arguments,
                    span,
                } => {
                    for argument in arguments {
                        self.load(body, *argument, *span)?;
                    }
                    let index = self
                        .function_indices
                        .get(function)
                        .copied()
                        .ok_or_else(|| {
                            wasm_error(*span, "MIR call target has no Wasm function index")
                        })?;
                    body.push(Op::Leaf(Instruction::Call(index.0)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::RefFunc {
                    destination,
                    function,
                    span,
                    ..
                } => {
                    let index = self
                        .function_indices
                        .get(function)
                        .copied()
                        .ok_or_else(|| {
                            wasm_error(*span, "MIR ref.func target has no Wasm function index")
                        })?;
                    body.push(Op::Leaf(Instruction::RefFunc(index.0)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::ClosureNew { .. } => self.emit_closure_new(body, instruction)?,
                MirInstruction::CallRef {
                    destination,
                    function,
                    type_index,
                    arguments,
                    span,
                } => {
                    for argument in arguments {
                        self.load(body, *argument, *span)?;
                    }
                    self.load(body, *function, *span)?;
                    body.push(Op::Leaf(Instruction::CallRef(type_index.0)));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::ClosureCall { .. } => self.emit_closure_call(body, instruction)?,
                MirInstruction::ClosureGetCapture { .. } => {
                    self.emit_closure_get_capture(body, instruction)?
                }
                MirInstruction::RefNull {
                    destination,
                    heap,
                    span,
                } => {
                    body.push(Op::Leaf(Instruction::RefNull(heap_type(*heap))));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::RefIsNull {
                    destination,
                    value,
                    span,
                } => {
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(Instruction::RefIsNull));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::RefTest {
                    destination,
                    value,
                    reference,
                    span,
                } => {
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(ref_test(*reference)));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::RefCast {
                    destination,
                    value,
                    reference,
                    span,
                } => {
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(ref_cast(*reference)));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::I31New {
                    destination,
                    value,
                    span,
                } => {
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(Instruction::RefI31));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::I31GetS {
                    destination,
                    value,
                    span,
                } => {
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(Instruction::I31GetS));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::StructNew {
                    destination,
                    type_index,
                    arguments,
                    span,
                } => {
                    for argument in arguments {
                        self.load(body, *argument, *span)?;
                    }
                    body.push(Op::Leaf(Instruction::StructNew(type_index.0)));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::StructGet {
                    destination,
                    type_index,
                    field,
                    value,
                    span,
                } => {
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(Instruction::StructGet {
                        struct_type_index: type_index.0,
                        field_index: *field,
                    }));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::StructSet {
                    type_index,
                    field,
                    value,
                    new_value,
                    span,
                } => {
                    self.load(body, *value, *span)?;
                    self.load(body, *new_value, *span)?;
                    body.push(Op::Leaf(Instruction::StructSet {
                        struct_type_index: type_index.0,
                        field_index: *field,
                    }));
                }
                MirInstruction::ArrayNew {
                    destination,
                    type_index,
                    elements,
                    span,
                } => self.emit_array_new(body, *destination, *type_index, elements, *span)?,
                MirInstruction::ArrayNewDefault {
                    destination,
                    type_index,
                    length,
                    span,
                    ..
                } => {
                    self.emit_array_new_default(body, *destination, *type_index, *length, *span)?
                }
                MirInstruction::ArrayGet {
                    destination,
                    type_index,
                    value,
                    index,
                    span,
                } => self.emit_array_get(body, *destination, *type_index, *value, *index, *span)?,
                MirInstruction::ArrayClone {
                    destination,
                    type_index,
                    value,
                    span,
                } => self.emit_array_clone(body, *destination, *type_index, *value, *span)?,
                MirInstruction::ArraySet {
                    type_index,
                    value,
                    index,
                    new_value,
                    span,
                } => self.emit_array_set(body, *type_index, *value, *index, *new_value, *span)?,
                MirInstruction::ArrayLen {
                    destination,
                    value,
                    span,
                } => self.emit_array_len(body, *destination, *value, *span)?,
                MirInstruction::Load {
                    destination,
                    address,
                    offset,
                    span,
                    ..
                } => {
                    self.load(body, *address, *span)?;
                    body.push(Op::Leaf(Instruction::I32Load(memory(*offset))));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::Load8U {
                    destination,
                    address,
                    offset,
                    span,
                    ..
                } => {
                    self.load(body, *address, *span)?;
                    body.push(Op::Leaf(Instruction::I32Load8U(memory_with_align(
                        *offset, 0,
                    ))));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::Store {
                    address,
                    value,
                    offset,
                    span,
                    ..
                } => {
                    self.load(body, *address, *span)?;
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(Instruction::I32Store(memory(*offset))));
                }
                MirInstruction::Store8 {
                    address,
                    value,
                    offset,
                    span,
                    ..
                } => {
                    self.load(body, *address, *span)?;
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(Instruction::I32Store8(memory_with_align(
                        *offset, 0,
                    ))));
                }
                MirInstruction::Store16 {
                    address,
                    value,
                    offset,
                    span,
                    ..
                } => {
                    self.load(body, *address, *span)?;
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(Instruction::I32Store16(memory_with_align(
                        *offset, 1,
                    ))));
                }
                MirInstruction::StoreI64 {
                    address,
                    value,
                    offset,
                    span,
                    ..
                } => {
                    self.load(body, *address, *span)?;
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(Instruction::I64Store(memory_with_align(
                        *offset, 3,
                    ))));
                }
                MirInstruction::StoreF32 {
                    address,
                    value,
                    offset,
                    span,
                    ..
                } => {
                    self.load(body, *address, *span)?;
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(Instruction::F32Store(memory_with_align(
                        *offset, 2,
                    ))));
                }
                MirInstruction::StoreF64 {
                    address,
                    value,
                    offset,
                    span,
                    ..
                } => {
                    self.load(body, *address, *span)?;
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(Instruction::F64Store(memory_with_align(
                        *offset, 3,
                    ))));
                }
                MirInstruction::WrapI64 {
                    destination,
                    value,
                    span,
                } => {
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(Instruction::I32WrapI64));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::WidenI64 {
                    destination,
                    value,
                    signed,
                    span,
                } => {
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(if *signed {
                        Instruction::I64ExtendI32S
                    } else {
                        Instruction::I64ExtendI32U
                    }));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::CallVoid {
                    function,
                    arguments,
                    span,
                } => {
                    for argument in arguments {
                        self.load(body, *argument, *span)?;
                    }
                    let index = self
                        .function_indices
                        .get(function)
                        .copied()
                        .ok_or_else(|| {
                            wasm_error(*span, "MIR call target has no Wasm function index")
                        })?;
                    body.push(Op::Leaf(Instruction::Call(index.0)));
                }
            }
        }
        Ok(())
    }
}
