//! Nested non-byte list and fixed-length list element copies.
//!
//! A nested list element is itself a GC array; copying one allocates or reads
//! an inner `(pointer, length)` buffer and recurses through its elements. A
//! fixed-length list has a static element count and no length prefix.
#![allow(clippy::vec_init_then_push)]

use super::super::wasm_error;
use super::{ListLoop, LocalRef, Structurer};
use crate::BackendError;
use crate::abi;
use crate::abi::canonical::CanonicalType;
use crate::cc::ValueShape;
use crate::types::HeapType;
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

impl Structurer<'_> {
    /// Stores a nested non-byte list element: allocate the inner buffer and
    /// copy recursively.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_nested_list_store(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        inner: &CanonicalType,
        repr: crate::cc::ReprId,
        element_shape: ValueShape,
        path: &[super::Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let inner_guest = self.resolve_guest(element_shape, span)?;
        let inner_array_type = self.repr_index(repr, span)?;
        let layout = abi::canonical::size_align(inner);
        let inner_stride = layout.size as i32;
        let array_local = self.node_locals(context.depth).array_local;
        let inner_locals = self.node_locals(context.depth + 1);
        self.emit_project(body, context, path, span)?;
        body.push(Op::Leaf(Instruction::LocalSet(array_local)));
        // realloc(0, 0, align, len * stride)
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::I32Const(layout.align as i32)));
        body.push(Op::Leaf(Instruction::LocalGet(array_local)));
        body.push(Op::Leaf(Instruction::ArrayLen));
        if inner_stride != 1 {
            body.push(Op::Leaf(Instruction::I32Const(inner_stride)));
            body.push(Op::Leaf(Instruction::I32Mul));
        }
        let realloc = self.realloc_index(span)?;
        body.push(Op::Leaf(Instruction::Call(realloc)));
        body.push(Op::Leaf(Instruction::LocalSet(inner_locals.scratch_local)));
        // Write (pointer, length) into the outer buffer.
        self.element_address(body, context, offset, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(inner_locals.scratch_local)));
        body.push(Op::Leaf(Instruction::I32Store(super::super::ops::memory(
            0,
        ))));
        self.element_address(body, context, offset + 4, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(array_local)));
        body.push(Op::Leaf(Instruction::ArrayLen));
        body.push(Op::Leaf(Instruction::I32Store(super::super::ops::memory(
            0,
        ))));
        // Copy each inner element.
        let inner_context = ListLoop {
            array: LocalRef {
                index: array_local,
                non_null: false,
            },
            array_type: inner_array_type,
            pointer: LocalRef {
                index: inner_locals.scratch_local,
                non_null: false,
            },
            index_local: inner_locals.index_local,
            stride: inner_stride,
            depth: context.depth + 1,
        };
        let mut loop_body = Body::new();
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::LocalGet(array_local)));
        loop_body.push(Op::Leaf(Instruction::ArrayLen));
        loop_body.push(Op::Leaf(Instruction::I32GeU));
        loop_body.push(Op::Leaf(Instruction::BrIf(1)));
        self.emit_store_node(
            &mut loop_body,
            &inner_context,
            0,
            inner,
            &inner_guest,
            &[],
            span,
        )?;
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::I32Const(1)));
        loop_body.push(Op::Leaf(Instruction::I32Add));
        loop_body.push(Op::Leaf(Instruction::LocalSet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::Br(0)));
        body.push(Op::Block {
            body: vec![Op::Loop {
                body: loop_body,
                result: None,
                span,
            }],
            result: None,
            span,
        });
        Ok(())
    }

    /// Loads a nested non-byte list element into a fresh GC array.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_nested_list_load(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        inner: &CanonicalType,
        repr: crate::cc::ReprId,
        element_shape: ValueShape,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let inner_guest = self.resolve_guest(element_shape, span)?;
        let inner_array_type = self.repr_index(repr, span)?;
        let layout = abi::canonical::size_align(inner);
        let inner_stride = layout.size as i32;
        let array_local = self.node_locals(context.depth).array_local;
        let inner_locals = self.node_locals(context.depth + 1);
        // pointer
        self.element_address(body, context, offset, span)?;
        body.push(Op::Leaf(Instruction::I32Load(super::super::ops::memory(0))));
        body.push(Op::Leaf(Instruction::LocalSet(inner_locals.scratch_local)));
        // array.new_default(len)
        self.element_address(body, context, offset + 4, span)?;
        body.push(Op::Leaf(Instruction::I32Load(super::super::ops::memory(0))));
        body.push(Op::Leaf(Instruction::ArrayNewDefault(inner_array_type)));
        body.push(Op::Leaf(Instruction::LocalSet(array_local)));
        let inner_context = ListLoop {
            array: LocalRef {
                index: array_local,
                non_null: false,
            },
            array_type: inner_array_type,
            pointer: LocalRef {
                index: inner_locals.scratch_local,
                non_null: false,
            },
            index_local: inner_locals.index_local,
            stride: inner_stride,
            depth: context.depth + 1,
        };
        let mut loop_body = Body::new();
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::LocalGet(array_local)));
        loop_body.push(Op::Leaf(Instruction::ArrayLen));
        loop_body.push(Op::Leaf(Instruction::I32GeU));
        loop_body.push(Op::Leaf(Instruction::BrIf(1)));
        self.array_local_concrete(&mut loop_body, array_local, inner_array_type);
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        self.emit_load_node(&mut loop_body, &inner_context, 0, inner, &inner_guest, span)?;
        loop_body.push(Op::Leaf(Instruction::ArraySet(inner_array_type)));
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::I32Const(1)));
        loop_body.push(Op::Leaf(Instruction::I32Add));
        loop_body.push(Op::Leaf(Instruction::LocalSet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::Br(0)));
        body.push(Op::Block {
            body: vec![Op::Loop {
                body: loop_body,
                result: None,
                span,
            }],
            result: None,
            span,
        });
        body.push(Op::Leaf(Instruction::LocalGet(array_local)));
        Ok(())
    }

    /// Frees a nested non-byte list element's inner elements and buffer.
    pub(super) fn emit_nested_list_free(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        inner: &CanonicalType,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let layout = abi::canonical::size_align(inner);
        let inner_stride = layout.size as i32;
        let inner_locals = self.node_locals(context.depth + 1);
        self.element_address(body, context, offset, span)?;
        body.push(Op::Leaf(Instruction::I32Load(super::super::ops::memory(0))));
        body.push(Op::Leaf(Instruction::LocalSet(inner_locals.scratch_local)));
        let inner_context = ListLoop {
            array: LocalRef {
                index: inner_locals.index_local,
                non_null: false,
            },
            array_type: 0,
            pointer: LocalRef {
                index: inner_locals.scratch_local,
                non_null: false,
            },
            index_local: inner_locals.index_local,
            stride: inner_stride,
            depth: context.depth + 1,
        };
        let mut loop_body = Body::new();
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        self.element_address(&mut loop_body, context, offset + 4, span)?;
        loop_body.push(Op::Leaf(Instruction::I32Load(super::super::ops::memory(0))));
        loop_body.push(Op::Leaf(Instruction::I32GeU));
        loop_body.push(Op::Leaf(Instruction::BrIf(1)));
        self.emit_free_node(&mut loop_body, &inner_context, 0, inner, span)?;
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::I32Const(1)));
        loop_body.push(Op::Leaf(Instruction::I32Add));
        loop_body.push(Op::Leaf(Instruction::LocalSet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::Br(0)));
        body.push(Op::Block {
            body: vec![Op::Loop {
                body: loop_body,
                result: None,
                span,
            }],
            result: None,
            span,
        });
        self.free_inner_buffer(
            body,
            context,
            offset,
            inner_stride,
            layout.align as i32,
            span,
        )
    }

    /// Stores a fixed-length list element, whose source array has a static
    /// element count.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_fixed_list_store(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        element: &CanonicalType,
        length: u32,
        repr: crate::cc::ReprId,
        element_shape: ValueShape,
        path: &[super::Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let element_guest = self.resolve_guest(element_shape, span)?;
        let array_type = self.repr_index(repr, span)?;
        let layout = abi::canonical::size_align(element);
        let array_local = self.node_locals(context.depth).array_local;
        let inner_locals = self.node_locals(context.depth + 1);
        self.emit_project(body, context, path, span)?;
        body.push(Op::Leaf(Instruction::LocalSet(array_local)));
        self.element_address(body, context, offset, span)?;
        body.push(Op::Leaf(Instruction::LocalSet(inner_locals.scratch_local)));
        let inner_context = ListLoop {
            array: LocalRef {
                index: array_local,
                non_null: false,
            },
            array_type,
            pointer: LocalRef {
                index: inner_locals.scratch_local,
                non_null: false,
            },
            index_local: inner_locals.index_local,
            stride: layout.size as i32,
            depth: context.depth + 1,
        };
        let mut loop_body = Body::new();
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::I32Const(length as i32)));
        loop_body.push(Op::Leaf(Instruction::I32GeU));
        loop_body.push(Op::Leaf(Instruction::BrIf(1)));
        self.emit_store_node(
            &mut loop_body,
            &inner_context,
            0,
            element,
            &element_guest,
            &[],
            span,
        )?;
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::I32Const(1)));
        loop_body.push(Op::Leaf(Instruction::I32Add));
        loop_body.push(Op::Leaf(Instruction::LocalSet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::Br(0)));
        body.push(Op::Block {
            body: vec![Op::Loop {
                body: loop_body,
                result: None,
                span,
            }],
            result: None,
            span,
        });
        Ok(())
    }

    /// Loads a fixed-length list element into a fresh GC array of length `N`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_fixed_list_load(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        element: &CanonicalType,
        length: u32,
        repr: crate::cc::ReprId,
        element_shape: ValueShape,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let element_guest = self.resolve_guest(element_shape, span)?;
        let array_type = self.repr_index(repr, span)?;
        let layout = abi::canonical::size_align(element);
        let array_local = self.node_locals(context.depth).array_local;
        let inner_locals = self.node_locals(context.depth + 1);
        body.push(Op::Leaf(Instruction::I32Const(length as i32)));
        body.push(Op::Leaf(Instruction::ArrayNewDefault(array_type)));
        body.push(Op::Leaf(Instruction::LocalSet(array_local)));
        let inner_context = ListLoop {
            array: LocalRef {
                index: array_local,
                non_null: false,
            },
            array_type,
            pointer: LocalRef {
                index: inner_locals.scratch_local,
                non_null: false,
            },
            index_local: inner_locals.index_local,
            stride: layout.size as i32,
            depth: context.depth + 1,
        };
        // The fixed list elements are inline at `offset`, not at a separate
        // buffer, so the inner loop addresses them through the element base.
        let mut setup = Body::new();
        self.element_address(&mut setup, context, offset, span)?;
        setup.push(Op::Leaf(Instruction::LocalSet(inner_locals.scratch_local)));
        body.extend(setup);
        let mut loop_body = Body::new();
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::I32Const(length as i32)));
        loop_body.push(Op::Leaf(Instruction::I32GeU));
        loop_body.push(Op::Leaf(Instruction::BrIf(1)));
        self.array_local_concrete(&mut loop_body, array_local, array_type);
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        self.emit_load_node(
            &mut loop_body,
            &inner_context,
            0,
            element,
            &element_guest,
            span,
        )?;
        loop_body.push(Op::Leaf(Instruction::ArraySet(array_type)));
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::I32Const(1)));
        loop_body.push(Op::Leaf(Instruction::I32Add));
        loop_body.push(Op::Leaf(Instruction::LocalSet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::Br(0)));
        body.push(Op::Block {
            body: vec![Op::Loop {
                body: loop_body,
                result: None,
                span,
            }],
            result: None,
            span,
        });
        body.push(Op::Leaf(Instruction::LocalGet(array_local)));
        Ok(())
    }

    /// Frees a fixed-length list element's nested buffers.
    pub(super) fn emit_fixed_list_free(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        element: &CanonicalType,
        length: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let layout = abi::canonical::size_align(element);
        let inner_locals = self.node_locals(context.depth + 1);
        self.element_address(body, context, offset, span)?;
        body.push(Op::Leaf(Instruction::LocalSet(inner_locals.scratch_local)));
        let inner_context = ListLoop {
            array: LocalRef {
                index: inner_locals.index_local,
                non_null: false,
            },
            array_type: 0,
            pointer: LocalRef {
                index: inner_locals.scratch_local,
                non_null: false,
            },
            index_local: inner_locals.index_local,
            stride: layout.size as i32,
            depth: context.depth + 1,
        };
        let mut loop_body = Body::new();
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::I32Const(length as i32)));
        loop_body.push(Op::Leaf(Instruction::I32GeU));
        loop_body.push(Op::Leaf(Instruction::BrIf(1)));
        self.emit_free_node(&mut loop_body, &inner_context, 0, element, span)?;
        loop_body.push(Op::Leaf(Instruction::LocalGet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::I32Const(1)));
        loop_body.push(Op::Leaf(Instruction::I32Add));
        loop_body.push(Op::Leaf(Instruction::LocalSet(inner_locals.index_local)));
        loop_body.push(Op::Leaf(Instruction::Br(0)));
        body.push(Op::Block {
            body: vec![Op::Loop {
                body: loop_body,
                result: None,
                span,
            }],
            result: None,
            span,
        });
        Ok(())
    }

    fn free_inner_buffer(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        stride: i32,
        align: i32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let realloc = self.realloc_index(span)?;
        self.element_address(body, context, offset, span)?;
        body.push(Op::Leaf(Instruction::I32Load(super::super::ops::memory(0))));
        self.element_address(body, context, offset + 4, span)?;
        body.push(Op::Leaf(Instruction::I32Load(super::super::ops::memory(0))));
        if stride != 1 {
            body.push(Op::Leaf(Instruction::I32Const(stride)));
            body.push(Op::Leaf(Instruction::I32Mul));
        }
        body.push(Op::Leaf(Instruction::I32Const(align)));
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::Call(realloc)));
        body.push(Op::Leaf(Instruction::Drop));
        Ok(())
    }

    fn array_local_concrete(&self, body: &mut Body, local: u32, array_type: u32) {
        body.push(Op::Leaf(Instruction::LocalGet(local)));
        body.push(Op::Leaf(Instruction::RefCastNonNull(
            crate::wasm::convert::heap_type(HeapType::Index(crate::types::DefinedTypeId(
                array_type,
            ))),
        )));
    }

    fn realloc_index(&self, span: TextRange) -> Result<u32, Vec<BackendError>> {
        self.function_indices
            .get(&crate::abi::REALLOC_SYMBOL)
            .map(|index| index.0)
            .ok_or_else(|| wasm_error(span, "canonical list copy is missing cabi_realloc"))
    }
}
