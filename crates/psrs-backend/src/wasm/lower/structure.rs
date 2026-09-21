use super::{local, value_type, wasm_error};
use crate::BackendError;
use crate::mir::{self, BlockId, Function as MirFunction, Instruction as MirInstruction};
use crate::types::{ValueId, ValueType};
use crate::wasm::convert::heap_type;
use crate::wasm::{Body, Op};
use ops::{linear_load, linear_store, memory, primitive, ref_cast, ref_test};
mod arrays;
mod closure;
mod helpers;
mod ops;
mod region;
use crate::wasm::FunctionIndex;
use closure::ClosureOps;
use helpers::ValueOps;
use psrs_hir::SymbolId;
use region::RegionOps;
use std::collections::{HashMap, HashSet};
use wasm_encoder::Instruction;
pub(super) struct Structurer<'a> {
    pub(super) function: &'a MirFunction,
    pub(super) blocks: HashMap<BlockId, &'a mir::BasicBlock>,
    pub(super) locals: HashMap<ValueId, u32>,
    pub(super) function_indices: &'a HashMap<SymbolId, FunctionIndex>,
    pub(super) string_offsets: &'a HashMap<String, u32>,
    pub(super) linear_allocator: Option<FunctionIndex>,
}
impl Structurer<'_> {
    pub(super) fn emit_region(
        &self,
        current: BlockId,
        stop: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
        body: &mut Body,
    ) -> Result<Option<ValueId>, Vec<BackendError>> {
        RegionOps::emit_region(self, current, stop, visited, body)
    }
}
impl Structurer<'_> {
    fn emit_block_instructions(
        &self,
        instructions: &[MirInstruction],
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        for instruction in instructions {
            match instruction {
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
                MirInstruction::StringConstant {
                    destination,
                    bytes,
                    span,
                } => {
                    let offset =
                        self.string_offsets.get(bytes).copied().ok_or_else(|| {
                            wasm_error(*span, "string constant has no data segment")
                        })?;
                    body.push(Op::Leaf(Instruction::I32Const(offset as i32)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
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
                MirInstruction::Call {
                    destination,
                    function,
                    arguments,
                    span,
                } => {
                    for argument in arguments {
                        body.push(Op::Leaf(Instruction::LocalGet(local(
                            &self.locals,
                            *argument,
                            *span,
                        )?)));
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
                MirInstruction::LinearAlloc {
                    destination,
                    bytes,
                    span,
                } => {
                    let allocator = self.linear_allocator.ok_or_else(|| {
                        wasm_error(*span, "linear allocation has no allocator function")
                    })?;
                    body.push(Op::Leaf(Instruction::I32Const(0)));
                    body.push(Op::Leaf(Instruction::I32Const(0)));
                    body.push(Op::Leaf(Instruction::I32Const(4)));
                    body.push(Op::Leaf(Instruction::I32Const(*bytes as i32)));
                    body.push(Op::Leaf(Instruction::Call(allocator.0)));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::LinearAllocDynamic {
                    destination,
                    bytes,
                    span,
                } => self.emit_linear_alloc_dynamic(body, *destination, *bytes, *span)?,
                MirInstruction::LinearMemoryCopy {
                    destination,
                    source,
                    bytes,
                    destination_offset,
                    source_offset,
                    span,
                } => self.emit_linear_memory_copy(
                    body,
                    *destination,
                    *source,
                    *bytes,
                    *destination_offset,
                    *source_offset,
                    *span,
                )?,
                MirInstruction::LinearLoad {
                    destination,
                    address,
                    offset,
                    ty,
                    span,
                } => {
                    self.load(body, *address, *span)?;
                    body.push(Op::Leaf(linear_load(*ty, *offset)));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::LinearStore {
                    address,
                    value,
                    offset,
                    ty,
                    span,
                } => {
                    self.load(body, *address, *span)?;
                    self.load(body, *value, *span)?;
                    body.push(Op::Leaf(linear_store(*ty, *offset)));
                }
                MirInstruction::LinearClosureNew {
                    destination,
                    table_slot,
                    captures,
                    span,
                    ..
                } => {
                    let allocator = self.linear_allocator.ok_or_else(|| {
                        wasm_error(*span, "linear closure allocation has no allocator function")
                    })?;
                    let bytes = 4u32
                        .checked_add((captures.len() as u32).saturating_mul(8))
                        .ok_or_else(|| wasm_error(*span, "linear closure is too large"))?;
                    body.push(Op::Leaf(Instruction::I32Const(0)));
                    body.push(Op::Leaf(Instruction::I32Const(0)));
                    body.push(Op::Leaf(Instruction::I32Const(4)));
                    body.push(Op::Leaf(Instruction::I32Const(bytes as i32)));
                    body.push(Op::Leaf(Instruction::Call(allocator.0)));
                    self.store(body, *destination, *span)?;
                    self.load(body, *destination, *span)?;
                    body.push(Op::Leaf(Instruction::I32Const(table_slot.0 as i32)));
                    body.push(Op::Leaf(Instruction::I32Store(memory(0))));
                    for (index, capture) in captures.iter().enumerate() {
                        let ty = value_type(self.function, *capture).ok_or_else(|| {
                            wasm_error(*span, "linear closure capture has no value type")
                        })?;
                        if !matches!(ty, ValueType::I32 | ValueType::Boolean | ValueType::F64) {
                            return Err(wasm_error(
                                *span,
                                "linear closure capture has an unsupported value type",
                            ));
                        }
                        self.load(body, *destination, *span)?;
                        self.load(body, *capture, *span)?;
                        body.push(Op::Leaf(linear_store(
                            ty,
                            4 + (index as u32).saturating_mul(8),
                        )));
                    }
                }
                MirInstruction::LinearClosureCall {
                    destination,
                    function,
                    type_index,
                    arguments,
                    span,
                } => {
                    self.load(body, *function, *span)?;
                    for argument in arguments {
                        self.load(body, *argument, *span)?;
                    }
                    self.load(body, *function, *span)?;
                    body.push(Op::Leaf(Instruction::I32Load(memory(0))));
                    body.push(Op::Leaf(Instruction::CallIndirect {
                        type_index: type_index.0,
                        table_index: 0,
                    }));
                    self.store(body, *destination, *span)?;
                }
                MirInstruction::LinearClosureGetCapture {
                    destination,
                    closure,
                    index,
                    ty,
                    span,
                } => {
                    self.load(body, *closure, *span)?;
                    body.push(Op::Leaf(linear_load(*ty, 4 + index.saturating_mul(8))));
                    self.store(body, *destination, *span)?;
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
