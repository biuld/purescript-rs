use super::ops::ref_cast;
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
        body.push(Op::Leaf(Instruction::RefFunc(index)));
        for capture in captures {
            self.load(body, *capture, *span)?;
            match value_type(self.function, *capture) {
                Some(ValueType::I32 | ValueType::Boolean) => {
                    body.push(Op::Leaf(Instruction::RefI31));
                }
                Some(ValueType::F64) => {
                    let Some(boxed_f64_type) = boxed_f64_type else {
                        return Err(wasm_error(
                            *span,
                            "MIR closure has no F64 capture box layout",
                        ));
                    };
                    body.push(Op::Leaf(Instruction::StructNew(*boxed_f64_type)));
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
            array_type_index: *capture_array_type,
            array_size: captures.len() as u32,
        }));
        body.push(Op::Leaf(Instruction::StructNew(*closure_type)));
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
            struct_type_index: *closure_type,
            field_index: 0,
        }));
        body.push(Op::Leaf(ref_cast(RefType {
            nullable: false,
            heap: HeapType::Index(*type_index),
        })));
        body.push(Op::Leaf(Instruction::CallRef(*type_index)));
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
            struct_type_index: *closure_type,
            field_index: 1,
        }));
        body.push(Op::Leaf(Instruction::I32Const(*index as i32)));
        body.push(Op::Leaf(Instruction::ArrayGet(*capture_array_type)));
        match value_type(self.function, *destination) {
            Some(ValueType::I32 | ValueType::Boolean) => {
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
                    struct_type_index: *boxed_f64_type,
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
}
