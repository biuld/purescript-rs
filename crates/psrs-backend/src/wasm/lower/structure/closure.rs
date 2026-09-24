use super::ops::{linear_load, linear_store, linear_value_alignment, memory, ref_cast};
use super::{Structurer, ValueOps, value_type, wasm_error};
use crate::BackendError;
use crate::mir::Instruction as MirInstruction;
use crate::types::{HeapType, RefType, ValueType};
use crate::wasm::{Body, Op};
use wasm_encoder::Instruction;

pub(super) trait ClosureOps {
    fn emit_closure_new(
        &self,
        body: &mut Body,
        instruction: &MirInstruction,
    ) -> Result<(), Vec<BackendError>>;

    fn emit_closure_call(
        &self,
        body: &mut Body,
        instruction: &MirInstruction,
    ) -> Result<(), Vec<BackendError>>;

    fn emit_closure_get_capture(
        &self,
        body: &mut Body,
        instruction: &MirInstruction,
    ) -> Result<(), Vec<BackendError>>;

    fn emit_linear_closure_new(
        &self,
        body: &mut Body,
        instruction: &MirInstruction,
    ) -> Result<(), Vec<BackendError>>;

    fn emit_linear_closure_get_capture(
        &self,
        body: &mut Body,
        instruction: &MirInstruction,
    ) -> Result<(), Vec<BackendError>>;
}

impl ClosureOps for Structurer<'_> {
    fn emit_closure_new(
        &self,
        body: &mut Body,
        instruction: &MirInstruction,
    ) -> Result<(), Vec<BackendError>> {
        let MirInstruction::ClosureNew {
            destination,
            function,
            closure_type,
            capture_array_type,
            boxed_integer_type,
            boxed_f64_type,
            captures,
            span,
            ..
        } = instruction
        else {
            unreachable!("closure.new emitter received another instruction");
        };
        let index = self
            .function_indices
            .get(function)
            .copied()
            .ok_or_else(|| wasm_error(*span, "MIR closure target has no Wasm function index"))?;
        body.push(Op::Leaf(Instruction::RefFunc(index.0)));
        for capture in captures {
            self.load(body, *capture, *span)?;
            match value_type(self.function, *capture) {
                Some(ValueType::I32) => {
                    let Some(boxed_integer_type) = boxed_integer_type else {
                        return Err(wasm_error(
                            *span,
                            "MIR closure has no Int capture box layout",
                        ));
                    };
                    body.push(Op::Leaf(Instruction::StructNew(boxed_integer_type.0)));
                }
                Some(ValueType::Boolean) => {
                    body.push(Op::Leaf(Instruction::RefI31));
                }
                Some(ValueType::F64) => {
                    let Some(boxed_f64_type) = boxed_f64_type else {
                        return Err(wasm_error(
                            *span,
                            "MIR closure has no F64 capture box layout",
                        ));
                    };
                    body.push(Op::Leaf(Instruction::StructNew(boxed_f64_type.0)));
                }
                Some(ValueType::Ref(_)) => {}
                _ => {
                    return Err(wasm_error(
                        *span,
                        "MIR closure capture has an unsupported value type",
                    ));
                }
            }
        }
        body.push(Op::Leaf(Instruction::ArrayNewFixed {
            array_type_index: capture_array_type.0,
            array_size: captures.len() as u32,
        }));
        body.push(Op::Leaf(Instruction::StructNew(closure_type.0)));
        self.store(body, *destination, *span)
    }

    fn emit_closure_call(
        &self,
        body: &mut Body,
        instruction: &MirInstruction,
    ) -> Result<(), Vec<BackendError>> {
        let MirInstruction::ClosureCall {
            destination,
            function,
            type_index,
            closure_type,
            arguments,
            span,
            ..
        } = instruction
        else {
            unreachable!("closure.call emitter received another instruction");
        };
        self.load(body, *function, *span)?;
        for argument in arguments {
            self.load(body, *argument, *span)?;
        }
        self.load(body, *function, *span)?;
        body.push(Op::Leaf(ref_cast(RefType {
            nullable: false,
            heap: HeapType::Index(*closure_type),
        })));
        body.push(Op::Leaf(Instruction::StructGet {
            struct_type_index: closure_type.0,
            field_index: 0,
        }));
        body.push(Op::Leaf(ref_cast(RefType {
            nullable: false,
            heap: HeapType::Index(*type_index),
        })));
        body.push(Op::Leaf(Instruction::CallRef(type_index.0)));
        self.store(body, *destination, *span)
    }

    fn emit_closure_get_capture(
        &self,
        body: &mut Body,
        instruction: &MirInstruction,
    ) -> Result<(), Vec<BackendError>> {
        let MirInstruction::ClosureGetCapture {
            destination,
            closure,
            closure_type,
            capture_array_type,
            boxed_integer_type,
            boxed_f64_type,
            index,
            span,
        } = instruction
        else {
            unreachable!("closure.get_capture emitter received another instruction");
        };
        self.load(body, *closure, *span)?;
        body.push(Op::Leaf(ref_cast(RefType {
            nullable: false,
            heap: HeapType::Index(*closure_type),
        })));
        body.push(Op::Leaf(Instruction::StructGet {
            struct_type_index: closure_type.0,
            field_index: 1,
        }));
        body.push(Op::Leaf(Instruction::I32Const(*index as i32)));
        body.push(Op::Leaf(Instruction::ArrayGet(capture_array_type.0)));
        match value_type(self.function, *destination) {
            Some(ValueType::I32) => {
                let Some(boxed_integer_type) = boxed_integer_type else {
                    return Err(wasm_error(
                        *span,
                        "MIR closure has no Int capture box layout",
                    ));
                };
                body.push(Op::Leaf(ref_cast(RefType {
                    nullable: false,
                    heap: HeapType::Index(*boxed_integer_type),
                })));
                body.push(Op::Leaf(Instruction::StructGet {
                    struct_type_index: boxed_integer_type.0,
                    field_index: 0,
                }));
            }
            Some(ValueType::Boolean) => {
                body.push(Op::Leaf(ref_cast(RefType {
                    nullable: true,
                    heap: HeapType::I31,
                })));
                body.push(Op::Leaf(Instruction::I31GetS));
            }
            Some(ValueType::F64) => {
                let Some(boxed_f64_type) = boxed_f64_type else {
                    return Err(wasm_error(
                        *span,
                        "MIR closure has no F64 capture box layout",
                    ));
                };
                body.push(Op::Leaf(ref_cast(RefType {
                    nullable: false,
                    heap: HeapType::Index(*boxed_f64_type),
                })));
                body.push(Op::Leaf(Instruction::StructGet {
                    struct_type_index: boxed_f64_type.0,
                    field_index: 0,
                }));
            }
            Some(ValueType::Ref(reference)) => {
                body.push(Op::Leaf(ref_cast(reference)));
            }
            _ => {
                return Err(wasm_error(
                    *span,
                    "MIR closure capture has an unsupported result type",
                ));
            }
        }
        self.store(body, *destination, *span)
    }

    fn emit_linear_closure_new(
        &self,
        body: &mut Body,
        instruction: &MirInstruction,
    ) -> Result<(), Vec<BackendError>> {
        let MirInstruction::LinearClosureNew {
            destination,
            table_slot,
            captures,
            capture_offsets,
            allocation_bytes,
            allocation_alignment,
            span,
            ..
        } = instruction
        else {
            unreachable!("linear closure.new emitter received another instruction");
        };
        if captures.len() != capture_offsets.len() {
            return Err(wasm_error(
                *span,
                "linear closure capture layout is incomplete",
            ));
        }
        let allocator = self.linear_allocator.ok_or_else(|| {
            wasm_error(*span, "linear closure allocation has no allocator function")
        })?;
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::I32Const(
            *allocation_alignment as i32,
        )));
        body.push(Op::Leaf(Instruction::I32Const(*allocation_bytes as i32)));
        body.push(Op::Leaf(Instruction::Call(allocator.0)));
        self.store(body, *destination, *span)?;
        self.load(body, *destination, *span)?;
        body.push(Op::Leaf(Instruction::I32Const(table_slot.0 as i32)));
        body.push(Op::Leaf(Instruction::I32Store(memory(0))));
        for (capture, offset) in captures.iter().zip(capture_offsets) {
            let ty = value_type(self.function, *capture)
                .ok_or_else(|| wasm_error(*span, "linear closure capture has no value type"))?;
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
                *offset,
                0,
                linear_value_alignment(ty),
            )));
        }
        Ok(())
    }

    fn emit_linear_closure_get_capture(
        &self,
        body: &mut Body,
        instruction: &MirInstruction,
    ) -> Result<(), Vec<BackendError>> {
        let MirInstruction::LinearClosureGetCapture {
            destination,
            closure,
            offset,
            ty,
            span,
            ..
        } = instruction
        else {
            unreachable!("linear closure.get_capture emitter received another instruction");
        };
        self.load(body, *closure, *span)?;
        body.push(Op::Leaf(linear_load(
            *ty,
            *offset,
            0,
            linear_value_alignment(*ty),
        )));
        self.store(body, *destination, *span)
    }
}
