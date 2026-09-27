//! Wasm loops for canonical non-byte lists. The MIR instruction stays
//! straight-line so the extent checker can see allocator provenance without
//! following a dynamic index. One parameterized instruction covers scalars,
//! strings, records, flags, variants, and nested lists, selected recursively by
//! the canonical element and its guest layout. Aggregate payloads recurse into
//! their own store/load/free so nested buffers are transcoded and freed.

mod aggregate;
mod element;
mod nested;
mod record;
mod variant;

use super::super::wasm_error;
use super::Structurer;
use super::helpers::ValueOps;
use crate::BackendError;
use crate::abi;
use crate::abi::canonical::CanonicalType;
use crate::cc::{GuestLayout, RefShape, ValueShape};
use crate::mir::{Instruction as MirInstruction, ListDirection};
use crate::types::{DefinedTypeId, HeapType, RefType, ValueId};
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

/// A Wasm local the list copy reads, plus whether the value restores its
/// non-null reference type with `ref.as_non_null`.
#[derive(Clone, Copy)]
pub(super) struct LocalRef {
    pub(super) index: u32,
    pub(super) non_null: bool,
}

/// One list-copy loop. `array` is the GC array (source for a store, destination
/// for a load); `pointer` is the canonical buffer; `index_local` counts the
/// elements. `depth` selects this loop's index and scratch locals so nested
/// list copies do not share an index.
#[derive(Clone, Copy)]
pub(super) struct ListLoop {
    pub(super) array: LocalRef,
    pub(super) array_type: u32,
    pub(super) pointer: LocalRef,
    pub(super) index_local: u32,
    pub(super) stride: i32,
    pub(super) depth: u32,
}

/// One projection from the enclosing array element to a nested guest value. A
/// record field reads a struct field directly; a tagged payload first casts the
/// element to the concrete case struct.
#[derive(Clone, Copy)]
pub(super) enum Projection {
    Field { ty: u32, field: u32 },
    Case { ty: u32, field: u32 },
}

impl Structurer<'_> {
    fn mir_local(&self, value: ValueId, span: TextRange) -> Result<LocalRef, Vec<BackendError>> {
        super::super::local(&self.locals, value, span).map(|index| LocalRef {
            index,
            non_null: self.needs_non_null_cast(value),
        })
    }

    fn node_locals(&self, depth: u32) -> NodeLocals {
        let locals = self
            .list_locals
            .expect("a nested list copy has loop locals");
        NodeLocals {
            index_local: locals.index(depth),
            scratch_local: locals.scratch(depth),
            array_local: locals.array(depth),
        }
    }

    pub(super) fn emit_list_copy(
        &self,
        instruction: &MirInstruction,
    ) -> Result<Body, Vec<BackendError>> {
        let MirInstruction::ListCopy {
            direction,
            array,
            array_type,
            element,
            element_guest,
            pointer,
            length,
            span,
            ..
        } = instruction
        else {
            unreachable!("list copy received another instruction")
        };
        if self.list_locals.is_none() {
            return Err(wasm_error(*span, "canonical list copy has no loop locals"));
        }
        let stride = abi::canonical::size_align(element).size as i32;
        let index_local = self.list_locals.expect("checked above").index(0);
        let mut body = Body::new();
        if *direction == ListDirection::Load {
            self.load(&mut body, *length, *span)?;
            body.push(Op::Leaf(Instruction::ArrayNewDefault(array_type.0)));
            self.store(&mut body, *array, *span)?;
        }
        let loop_context = ListLoop {
            array: self.mir_local(*array, *span)?,
            array_type: array_type.0,
            pointer: self.mir_local(*pointer, *span)?,
            index_local,
            stride,
            depth: 0,
        };
        let mut loop_body = Body::new();
        loop_body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        self.load(&mut loop_body, *length, *span)?;
        loop_body.push(Op::Leaf(Instruction::I32GeU));
        loop_body.push(Op::Leaf(Instruction::BrIf(1)));
        match direction {
            ListDirection::Store => {
                self.emit_store_node(
                    &mut loop_body,
                    &loop_context,
                    0,
                    element,
                    element_guest,
                    &[],
                    *span,
                )?;
            }
            ListDirection::Load => {
                self.emit_local(&mut loop_body, loop_context.array);
                loop_body.push(Op::Leaf(Instruction::LocalGet(index_local)));
                self.emit_load_node(
                    &mut loop_body,
                    &loop_context,
                    0,
                    element,
                    element_guest,
                    *span,
                )?;
                loop_body.push(Op::Leaf(Instruction::ArraySet(array_type.0)));
            }
            ListDirection::Free => {
                self.emit_free_node(&mut loop_body, &loop_context, 0, element, *span)?;
            }
        }
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

    /// Emits `local.get` for a stored local, restoring its non-null type.
    pub(super) fn emit_local(&self, body: &mut Body, local: LocalRef) {
        body.push(Op::Leaf(Instruction::LocalGet(local.index)));
        if local.non_null {
            body.push(Op::Leaf(Instruction::RefAsNonNull));
        }
    }
}

/// The index, `i32` scratch, and array-reference locals of one nesting level.
pub(super) struct NodeLocals {
    pub(super) index_local: u32,
    pub(super) scratch_local: u32,
    pub(super) array_local: u32,
}

/// The number of nested list loops one function needs: one for the outer list
/// copy plus one per list nested in an element.
pub(crate) fn list_copy_depth(function: &crate::mir::Function) -> u32 {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            MirInstruction::ListCopy { element, .. } => Some(1 + list_nesting(element)),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

/// The number of nested list loops a value needs when copied as one element.
fn list_nesting(ty: &CanonicalType) -> u32 {
    match ty {
        CanonicalType::List(inner) | CanonicalType::FixedList { element: inner, .. } => {
            1 + list_nesting(inner)
        }
        CanonicalType::Record(fields) => fields
            .iter()
            .map(|field| list_nesting(&field.ty))
            .max()
            .unwrap_or(0),
        CanonicalType::Option(payload) => list_nesting(payload),
        CanonicalType::Result { ok, err } => ok
            .as_deref()
            .map(list_nesting)
            .unwrap_or(0)
            .max(err.as_deref().map(list_nesting).unwrap_or(0)),
        CanonicalType::Variant(cases) => cases
            .iter()
            .map(|case| case.payload.as_deref().map(list_nesting).unwrap_or(0))
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

impl Structurer<'_> {
    /// Emits the canonical element's memory address:
    /// `pointer + index * stride + offset`.
    pub(super) fn element_address(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.emit_local(body, context.pointer);
        body.push(Op::Leaf(Instruction::LocalGet(context.index_local)));
        body.push(Op::Leaf(Instruction::I32Const(context.stride)));
        body.push(Op::Leaf(Instruction::I32Mul));
        body.push(Op::Leaf(Instruction::I32Add));
        if offset != 0 {
            body.push(Op::Leaf(Instruction::I32Const(offset as i32)));
            body.push(Op::Leaf(Instruction::I32Add));
        }
        let _ = span;
        Ok(())
    }

    pub(super) fn array_get(
        &self,
        body: &mut Body,
        array: LocalRef,
        array_type: u32,
        index_local: u32,
        _span: TextRange,
    ) {
        self.emit_local(body, array);
        body.push(Op::Leaf(Instruction::RefCastNonNull(
            crate::wasm::convert::heap_type(crate::types::HeapType::Index(
                crate::types::DefinedTypeId(array_type),
            )),
        )));
        body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        body.push(Op::Leaf(Instruction::ArrayGet(array_type)));
    }

    /// Resolves a guest value shape to its recursive layout.
    pub(super) fn resolve_guest(
        &self,
        shape: ValueShape,
        span: TextRange,
    ) -> Result<GuestLayout, Vec<BackendError>> {
        let layout = self
            .layout
            .ok_or_else(|| wasm_error(span, "canonical list copy has no layout table"))?;
        crate::cc::guest_layout(shape, layout.representation_table())
            .ok_or_else(|| wasm_error(span, "canonical list element has no guest layout"))
    }

    /// The concrete defined type of a representation handle.
    pub(super) fn repr_index(
        &self,
        repr: crate::cc::ReprId,
        span: TextRange,
    ) -> Result<u32, Vec<BackendError>> {
        self.layout
            .ok_or_else(|| wasm_error(span, "canonical list copy has no layout table"))?
            .repr_index(repr)
            .map(|id| id.0)
            .map_err(|_| wasm_error(span, "canonical list element has no concrete type"))
    }

    /// The concrete defined type of one variant case.
    pub(super) fn case_index(
        &self,
        repr: crate::cc::ReprId,
        case: u32,
        span: TextRange,
    ) -> Result<u32, Vec<BackendError>> {
        self.layout
            .ok_or_else(|| wasm_error(span, "canonical list copy has no layout table"))?
            .variant_index(repr, case)
            .map(|id| id.0)
            .map_err(|_| wasm_error(span, "canonical variant element has no case type"))
    }

    /// The concrete GC string type, when the module needs the codec.
    pub(super) fn string_index(&self, span: TextRange) -> Result<u32, Vec<BackendError>> {
        self.layout
            .ok_or_else(|| wasm_error(span, "canonical list copy has no layout table"))?
            .string_index()
            .map(|id| id.0)
            .ok_or_else(|| wasm_error(span, "canonical list string has no GC string type"))
    }

    pub(super) fn boxed_integer(&self, span: TextRange) -> Result<u32, Vec<BackendError>> {
        self.layout
            .ok_or_else(|| wasm_error(span, "canonical list copy has no layout table"))?
            .boxed_integer_index()
            .map(|id| id.0)
            .ok_or_else(|| wasm_error(span, "canonical aggregate has no boxed integer type"))
    }

    pub(super) fn boxed_number(&self, span: TextRange) -> Result<u32, Vec<BackendError>> {
        self.layout
            .ok_or_else(|| wasm_error(span, "canonical list copy has no layout table"))?
            .boxed_number_index()
            .map(|id| id.0)
            .ok_or_else(|| wasm_error(span, "canonical aggregate has no boxed number type"))
    }
}

/// Whether a guest scalar shape stores an erased (boxed) value.
pub(super) fn is_erased(shape: ValueShape) -> bool {
    matches!(
        shape,
        ValueShape::Reference(crate::cc::Reference {
            heap: RefShape::Erased,
            ..
        })
    )
}

/// The reference type of a concrete defined type.
pub(super) fn concrete_ref(index: u32) -> RefType {
    RefType {
        nullable: false,
        heap: HeapType::Index(DefinedTypeId(index)),
    }
}
