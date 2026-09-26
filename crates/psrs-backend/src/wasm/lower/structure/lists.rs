//! Wasm loops for canonical non-byte lists. The MIR instruction stays
//! straight-line so the extent checker can see allocator provenance without
//! following a dynamic index.

use super::super::wasm_error;
use super::Structurer;
use super::helpers::ValueOps;
use crate::BackendError;
use crate::abi::{self, ListElement};
use crate::mir::{Instruction as MirInstruction, ListDirection};
use crate::types::ValueId;
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

impl Structurer<'_> {
    pub(super) fn emit_list_copy(
        &self,
        instruction: &MirInstruction,
    ) -> Result<Body, Vec<BackendError>> {
        let MirInstruction::ListCopy {
            direction,
            array,
            array_type,
            pointer,
            length,
            element,
            span,
        } = instruction
        else {
            unreachable!("list copy received another instruction")
        };
        let Some((index_local, scratch_local)) = self.list_locals else {
            return Err(wasm_error(*span, "canonical list copy has no loop locals"));
        };
        let mut body = Body::new();
        if *direction == ListDirection::Load {
            self.load(&mut body, *length, *span)?;
            body.push(Op::Leaf(Instruction::ArrayNewDefault(array_type.0)));
            self.store(&mut body, *array, *span)?;
        }
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::LocalSet(index_local)));
        let mut loop_body = Body::new();
        loop_body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        self.load(&mut loop_body, *length, *span)?;
        loop_body.push(Op::Leaf(Instruction::I32GeU));
        loop_body.push(Op::Leaf(Instruction::BrIf(1)));
        self.emit_list_element(
            &mut loop_body,
            *direction,
            *array,
            array_type.0,
            *pointer,
            *element,
            index_local,
            scratch_local,
            *span,
        )?;
        loop_body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        loop_body.push(Op::Leaf(Instruction::I32Const(1)));
        loop_body.push(Op::Leaf(Instruction::I32Add));
        loop_body.push(Op::Leaf(Instruction::LocalSet(index_local)));
        loop_body.push(Op::Leaf(Instruction::Br(0)));
        body.push(Op::Block {
            body: vec![Op::Loop {
                body: loop_body,
                result: None,
                span: *span,
            }],
            result: None,
            span: *span,
        });
        Ok(body)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_list_element(
        &self,
        body: &mut Body,
        direction: ListDirection,
        array: ValueId,
        array_type: u32,
        pointer: ValueId,
        element: ListElement,
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let (size, _) = abi::element_layout(element);
        if direction == ListDirection::FreeStrings {
            self.element_address(body, pointer, index_local, size, span)?;
            body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
            self.free_string_at(body, scratch_local, span)?;
            return Ok(());
        }
        if element == ListElement::String {
            return self.emit_string_element(
                body,
                direction,
                array,
                array_type,
                pointer,
                index_local,
                scratch_local,
                span,
            );
        }
        match direction {
            ListDirection::Store => {
                self.element_address(body, pointer, index_local, size, span)?;
                self.array_get(body, array, array_type, index_local, span)?;
                self.store_scalar(body, element);
            }
            ListDirection::Load => {
                self.load(body, array, span)?;
                body.push(Op::Leaf(Instruction::LocalGet(index_local)));
                self.element_address(body, pointer, index_local, size, span)?;
                self.load_scalar(body, element);
                body.push(Op::Leaf(Instruction::ArraySet(array_type)));
            }
            ListDirection::FreeStrings => unreachable!("string payloads are freed above"),
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_string_element(
        &self,
        body: &mut Body,
        direction: ListDirection,
        array: ValueId,
        array_type: u32,
        pointer: ValueId,
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        match direction {
            ListDirection::Store => {
                let encode = self.function_index(abi::STRING_TO_BYTES_SYMBOL, span)?;
                self.array_get(body, array, array_type, index_local, span)?;
                body.push(Op::Leaf(Instruction::RefAsNonNull));
                body.push(Op::Leaf(Instruction::Call(encode)));
                body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
                self.element_address(body, pointer, index_local, 8, span)?;
                body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                body.push(Op::Leaf(Instruction::I32Const(4)));
                body.push(Op::Leaf(Instruction::I32Add));
                body.push(Op::Leaf(Instruction::I32Store(super::ops::memory(0))));
                self.element_address(body, pointer, index_local, 8, span)?;
                body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                body.push(Op::Leaf(Instruction::I32Load(super::ops::memory(0))));
                body.push(Op::Leaf(Instruction::I32Store(super::ops::memory(4))));
            }
            ListDirection::Load => {
                let decode = self.function_index(abi::BYTES_TO_STRING_SYMBOL, span)?;
                self.element_address(body, pointer, index_local, 8, span)?;
                body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
                self.load(body, array, span)?;
                body.push(Op::Leaf(Instruction::LocalGet(index_local)));
                body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                body.push(Op::Leaf(Instruction::I32Load(super::ops::memory(0))));
                body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                body.push(Op::Leaf(Instruction::I32Load(super::ops::memory(4))));
                body.push(Op::Leaf(Instruction::Call(decode)));
                body.push(Op::Leaf(Instruction::ArraySet(array_type)));
                self.free_string_at(body, scratch_local, span)?;
            }
            ListDirection::FreeStrings => unreachable!("freed before scalar elements"),
        }
        Ok(())
    }

    fn free_string_at(
        &self,
        body: &mut Body,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let realloc = self.function_index(abi::REALLOC_SYMBOL, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
        body.push(Op::Leaf(Instruction::I32Load(super::ops::memory(0))));
        body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
        body.push(Op::Leaf(Instruction::I32Load(super::ops::memory(4))));
        body.push(Op::Leaf(Instruction::I32Const(1)));
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::Call(realloc)));
        body.push(Op::Leaf(Instruction::Drop));
        Ok(())
    }

    fn element_address(
        &self,
        body: &mut Body,
        pointer: ValueId,
        index_local: u32,
        size: i32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.load(body, pointer, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        body.push(Op::Leaf(Instruction::I32Const(size)));
        body.push(Op::Leaf(Instruction::I32Mul));
        body.push(Op::Leaf(Instruction::I32Add));
        Ok(())
    }

    fn array_get(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        index_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.load(body, array, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        body.push(Op::Leaf(Instruction::ArrayGet(array_type)));
        Ok(())
    }

    fn store_scalar(&self, body: &mut Body, element: ListElement) {
        let mem = super::ops::memory_with_align;
        match element {
            ListElement::Word => body.push(Op::Leaf(Instruction::I32Store(mem(0, 2)))),
            ListElement::Boolean | ListElement::Narrow { bits: 8, .. } => {
                body.push(Op::Leaf(Instruction::I32Store8(mem(0, 0))));
            }
            ListElement::Narrow { bits: 16, .. } => {
                body.push(Op::Leaf(Instruction::I32Store16(mem(0, 1))));
            }
            ListElement::Narrow { .. } => body.push(Op::Leaf(Instruction::I32Store(mem(0, 2)))),
            ListElement::Scalar64 { signed } => {
                body.push(Op::Leaf(if signed {
                    Instruction::I64ExtendI32S
                } else {
                    Instruction::I64ExtendI32U
                }));
                body.push(Op::Leaf(Instruction::I64Store(mem(0, 3))));
            }
            ListElement::Float32 => {
                body.push(Op::Leaf(Instruction::F32DemoteF64));
                body.push(Op::Leaf(Instruction::F32Store(mem(0, 2))));
            }
            ListElement::Float64 => body.push(Op::Leaf(Instruction::F64Store(mem(0, 3)))),
            ListElement::String => unreachable!("strings are stored as a pair of i32s"),
        }
    }

    fn load_scalar(&self, body: &mut Body, element: ListElement) {
        let mem = super::ops::memory_with_align;
        match element {
            ListElement::Word => body.push(Op::Leaf(Instruction::I32Load(mem(0, 2)))),
            ListElement::Boolean
            | ListElement::Narrow {
                bits: 8,
                signed: false,
            } => {
                body.push(Op::Leaf(Instruction::I32Load8U(mem(0, 0))));
            }
            ListElement::Narrow {
                bits: 8,
                signed: true,
            } => {
                body.push(Op::Leaf(Instruction::I32Load8S(mem(0, 0))));
            }
            ListElement::Narrow {
                bits: 16,
                signed: false,
            } => {
                body.push(Op::Leaf(Instruction::I32Load16U(mem(0, 1))));
            }
            ListElement::Narrow {
                bits: 16,
                signed: true,
            } => {
                body.push(Op::Leaf(Instruction::I32Load16S(mem(0, 1))));
            }
            ListElement::Narrow { .. } => body.push(Op::Leaf(Instruction::I32Load(mem(0, 2)))),
            ListElement::Scalar64 { .. } => {
                body.push(Op::Leaf(Instruction::I64Load(mem(0, 3))));
                body.push(Op::Leaf(Instruction::I32WrapI64));
            }
            ListElement::Float32 => {
                body.push(Op::Leaf(Instruction::F32Load(mem(0, 2))));
                body.push(Op::Leaf(Instruction::F64PromoteF32));
            }
            ListElement::Float64 => body.push(Op::Leaf(Instruction::F64Load(mem(0, 3)))),
            ListElement::String => unreachable!("strings are loaded as a pair of i32s"),
        }
    }

    fn function_index(
        &self,
        symbol: psrs_hir::SymbolId,
        span: TextRange,
    ) -> Result<u32, Vec<BackendError>> {
        self.function_indices
            .get(&symbol)
            .map(|index| index.0)
            .ok_or_else(|| wasm_error(span, "canonical list copy is missing a codec function"))
    }
}

pub(crate) fn uses_list_copy(function: &crate::mir::Function) -> bool {
    function.blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, MirInstruction::ListCopy { .. }))
    })
}
