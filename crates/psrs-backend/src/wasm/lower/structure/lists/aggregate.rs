//! The recursive list-element dispatch and variant copies.
//!
//! One `(CanonicalType, GuestLayout)` pair drives the store, load, and free
//! walks. Variants branch on the tag; a nested list or fixed list recurses
//! through its own loop. The guest projection walks from the enclosing array
//! element to the nested value.
#![allow(clippy::vec_init_then_push)]

use super::super::wasm_error;
use super::{ListLoop, Projection, Structurer, concrete_ref};
use crate::BackendError;
use crate::abi;
use crate::abi::canonical::{CanonicalType, payload_cases};
use crate::cc::GuestLayout;
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

impl Structurer<'_> {
    /// Stores one canonical node at `offset` within the enclosing element.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_store_node(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        guest: &GuestLayout,
        path: &[Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if canonical.is_byte_list() {
            return self.emit_string_node_store(body, context, offset, guest, path, span);
        }
        match (canonical, guest) {
            (
                CanonicalType::Record(fields),
                GuestLayout::Product {
                    repr,
                    labels,
                    fields: shapes,
                },
            ) => self.emit_record_store(
                body, context, offset, fields, *repr, labels, shapes, path, span,
            ),
            (CanonicalType::Flags(names), GuestLayout::Product { repr, labels, .. }) => {
                self.emit_flags_store(body, context, offset, names, *repr, labels, path, span)
            }
            (
                CanonicalType::Option(_) | CanonicalType::Result { .. } | CanonicalType::Variant(_),
                GuestLayout::Variant { repr, cases },
            ) => {
                self.emit_variant_store(body, context, offset, canonical, *repr, cases, path, span)
            }
            (CanonicalType::List(inner), GuestLayout::Array { repr, element }) => self
                .emit_nested_list_store(body, context, offset, inner, *repr, *element, path, span),
            (
                CanonicalType::FixedList { element, length },
                GuestLayout::Array {
                    repr,
                    element: shape,
                },
            ) => self.emit_fixed_list_store(
                body, context, offset, element, *length, *repr, *shape, path, span,
            ),
            _ => self.emit_scalar_store(body, context, offset, canonical, guest, path, span),
        }
    }

    /// Loads one canonical node at `offset`, pushing its guest value.
    pub(super) fn emit_load_node(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        guest: &GuestLayout,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if canonical.is_byte_list() {
            return self.emit_string_node_load(body, context, offset, span);
        }
        match (canonical, guest) {
            (
                CanonicalType::Record(fields),
                GuestLayout::Product {
                    repr,
                    labels,
                    fields: shapes,
                },
            ) => self.emit_record_load(body, context, offset, fields, *repr, labels, shapes, span),
            (CanonicalType::Flags(names), GuestLayout::Product { repr, labels, .. }) => {
                self.emit_flags_load(body, context, offset, names, *repr, labels, span)
            }
            (
                CanonicalType::Option(_) | CanonicalType::Result { .. } | CanonicalType::Variant(_),
                GuestLayout::Variant { repr, cases },
            ) => self.emit_variant_load(body, context, offset, canonical, *repr, cases, span),
            (CanonicalType::List(inner), GuestLayout::Array { repr, element }) => {
                self.emit_nested_list_load(body, context, offset, inner, *repr, *element, span)
            }
            (
                CanonicalType::FixedList { element, length },
                GuestLayout::Array {
                    repr,
                    element: shape,
                },
            ) => self
                .emit_fixed_list_load(body, context, offset, element, *length, *repr, *shape, span),
            _ => self.emit_scalar_load(body, context, offset, canonical, guest, span),
        }
    }

    /// Frees one canonical node's nested buffers at `offset`.
    pub(super) fn emit_free_node(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if canonical.is_byte_list() {
            return self.free_bytes_at(body, context, offset, span);
        }
        match canonical {
            CanonicalType::Record(fields) => {
                self.emit_record_free(body, context, offset, fields, span)
            }
            CanonicalType::Flags(_) => Ok(()),
            CanonicalType::Option(_) | CanonicalType::Result { .. } | CanonicalType::Variant(_) => {
                self.emit_variant_free(body, context, offset, canonical, span)
            }
            CanonicalType::List(inner) => {
                self.emit_nested_list_free(body, context, offset, inner, span)
            }
            CanonicalType::FixedList { element, length } => {
                self.emit_fixed_list_free(body, context, offset, element, *length, span)
            }
            _ => Ok(()),
        }
    }

    /// Pushes the guest value selected by `path` from the enclosing element.
    pub(super) fn emit_project(
        &self,
        body: &mut Body,
        context: &ListLoop,
        path: &[Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        match path.split_last() {
            None => {
                self.array_get(
                    body,
                    context.array,
                    context.array_type,
                    context.index_local,
                    span,
                );
                Ok(())
            }
            Some((last, rest)) => {
                self.emit_project(body, context, rest, span)?;
                body.push(Op::Leaf(Instruction::RefAsNonNull));
                match last {
                    Projection::Field { ty, field } => {
                        body.push(Op::Leaf(Instruction::StructGet {
                            struct_type_index: *ty,
                            field_index: *field,
                        }));
                    }
                    Projection::Case { ty, field } => {
                        body.push(Op::Leaf(Instruction::RefCastNonNull(
                            crate::wasm::convert::heap_type(crate::types::HeapType::Index(
                                crate::types::DefinedTypeId(*ty),
                            )),
                        )));
                        body.push(Op::Leaf(Instruction::StructGet {
                            struct_type_index: *ty,
                            field_index: *field,
                        }));
                    }
                }
                Ok(())
            }
        }
    }

    /// Stores a variant node: write the discriminant and branch on the tag.
    #[allow(clippy::too_many_arguments)]
    fn emit_variant_store(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        repr: crate::cc::ReprId,
        guest_cases: &[crate::cc::VariantCase],
        path: &[Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let cases = payload_cases(canonical).ok_or_else(|| wasm_error(span, "not a variant"))?;
        let repr_index = self.repr_index(repr, span)?;
        let scratch = self.scratch(context.depth);
        self.emit_project(body, context, path, span)?;
        body.push(Op::Leaf(Instruction::RefAsNonNull));
        body.push(Op::Leaf(Instruction::StructGet {
            struct_type_index: repr_index,
            field_index: 0,
        }));
        body.push(Op::Leaf(Instruction::LocalSet(scratch)));
        self.element_address(body, context, offset, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(scratch)));
        store_discriminant(body, cases.len());
        let payload_offset = abi::layout::variant_payload_offset(&cases)
            .ok_or_else(|| wasm_error(span, "variant"))?;
        let mut chain = Body::new();
        self.variant_store_chain(
            &mut chain,
            context,
            offset + payload_offset,
            repr,
            &cases,
            guest_cases,
            path,
            scratch,
            0,
            span,
        )?;
        body.extend(chain);
        Ok(())
    }

    /// Emits the branch chain for a variant store.
    #[allow(clippy::too_many_arguments)]
    fn variant_store_chain(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        repr: crate::cc::ReprId,
        cases: &[Option<&CanonicalType>],
        guest_cases: &[crate::cc::VariantCase],
        path: &[Projection],
        scratch: u32,
        index: usize,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if index >= cases.len() {
            return Ok(());
        }
        let mut then_body = Body::new();
        if let Some(payload) = cases[index] {
            let case_type = self.case_index(repr, index as u32, span)?;
            let shape = guest_cases
                .get(index)
                .and_then(|case| case.fields.first().copied())
                .ok_or_else(|| wasm_error(span, "variant case has no payload field"))?;
            let guest = self.resolve_guest(shape, span)?;
            let mut nested = path.to_vec();
            nested.push(Projection::Case {
                ty: case_type,
                field: 1,
            });
            self.emit_store_node(
                &mut then_body,
                context,
                offset,
                payload,
                &guest,
                &nested,
                span,
            )?;
        }
        let mut else_body = Body::new();
        self.variant_store_chain(
            &mut else_body,
            context,
            offset,
            repr,
            cases,
            guest_cases,
            path,
            scratch,
            index + 1,
            span,
        )?;
        body.push(Op::Leaf(Instruction::LocalGet(scratch)));
        body.push(Op::Leaf(Instruction::I32Const(index as i32)));
        body.push(Op::Leaf(Instruction::I32Eq));
        body.push(Op::If {
            then_body,
            else_body,
            result: None,
            span,
        });
        Ok(())
    }

    /// Loads a variant node, rebuilding the active source case.
    #[allow(clippy::too_many_arguments)]
    fn emit_variant_load(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        repr: crate::cc::ReprId,
        guest_cases: &[crate::cc::VariantCase],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let cases = payload_cases(canonical).ok_or_else(|| wasm_error(span, "not a variant"))?;
        let repr_index = self.repr_index(repr, span)?;
        let scratch = self.scratch(context.depth);
        self.element_address(body, context, offset, span)?;
        load_discriminant(body, cases.len());
        body.push(Op::Leaf(Instruction::LocalSet(scratch)));
        let payload_offset = abi::layout::variant_payload_offset(&cases)
            .ok_or_else(|| wasm_error(span, "variant"))?;
        let result =
            crate::wasm::convert::val_type(crate::types::ValueType::Ref(concrete_ref(repr_index)));
        let chain = self.variant_load_chain(
            context,
            offset + payload_offset,
            repr,
            cases.len(),
            &cases,
            guest_cases,
            scratch,
            0,
            result,
            span,
        )?;
        body.extend(chain);
        Ok(())
    }

    /// Builds the case value for a variant load, in source tag order.
    #[allow(clippy::too_many_arguments)]
    fn variant_load_chain(
        &self,
        context: &ListLoop,
        offset: u32,
        repr: crate::cc::ReprId,
        case_count: usize,
        cases: &[Option<&CanonicalType>],
        guest_cases: &[crate::cc::VariantCase],
        scratch: u32,
        index: usize,
        result: wasm_encoder::ValType,
        span: TextRange,
    ) -> Result<Body, Vec<BackendError>> {
        let case_type = self.case_index(repr, index as u32, span)?;
        let mut build = Body::new();
        build.push(Op::Leaf(Instruction::I32Const(index as i32)));
        if let Some(payload) = cases[index] {
            let shape = guest_cases
                .get(index)
                .and_then(|case| case.fields.first().copied())
                .ok_or_else(|| wasm_error(span, "variant case has no payload field"))?;
            let guest = self.resolve_guest(shape, span)?;
            self.emit_load_node(&mut build, context, offset, payload, &guest, span)?;
        }
        build.push(Op::Leaf(Instruction::StructNew(case_type)));
        if index + 1 >= case_count {
            return Ok(build);
        }
        let else_body = self.variant_load_chain(
            context,
            offset,
            repr,
            case_count,
            cases,
            guest_cases,
            scratch,
            index + 1,
            result,
            span,
        )?;
        let mut body = Body::new();
        body.push(Op::Leaf(Instruction::LocalGet(scratch)));
        body.push(Op::Leaf(Instruction::I32Const(index as i32)));
        body.push(Op::Leaf(Instruction::I32Eq));
        body.push(Op::If {
            then_body: build,
            else_body,
            result: Some(result),
            span,
        });
        Ok(body)
    }

    /// Frees the active variant case's nested buffers.
    fn emit_variant_free(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let cases = payload_cases(canonical).ok_or_else(|| wasm_error(span, "not a variant"))?;
        let scratch = self.scratch(context.depth);
        self.element_address(body, context, offset, span)?;
        load_discriminant(body, cases.len());
        body.push(Op::Leaf(Instruction::LocalSet(scratch)));
        let payload_offset = abi::layout::variant_payload_offset(&cases)
            .ok_or_else(|| wasm_error(span, "variant"))?;
        let mut chain = Body::new();
        self.variant_free_chain(
            &mut chain,
            context,
            offset + payload_offset,
            &cases,
            scratch,
            0,
            span,
        )?;
        body.extend(chain);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn variant_free_chain(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        cases: &[Option<&CanonicalType>],
        scratch: u32,
        index: usize,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if index >= cases.len() {
            return Ok(());
        }
        let mut then_body = Body::new();
        if let Some(payload) = cases[index] {
            self.emit_free_node(&mut then_body, context, offset, payload, span)?;
        }
        let mut else_body = Body::new();
        self.variant_free_chain(
            &mut else_body,
            context,
            offset,
            cases,
            scratch,
            index + 1,
            span,
        )?;
        body.push(Op::Leaf(Instruction::LocalGet(scratch)));
        body.push(Op::Leaf(Instruction::I32Const(index as i32)));
        body.push(Op::Leaf(Instruction::I32Eq));
        body.push(Op::If {
            then_body,
            else_body,
            result: None,
            span,
        });
        Ok(())
    }
}

fn store_discriminant(body: &mut Body, cases: usize) {
    match abi::layout::discriminant_width(cases) {
        1 => body.push(Op::Leaf(Instruction::I32Store8(
            super::super::ops::memory_with_align(0, 0),
        ))),
        2 => body.push(Op::Leaf(Instruction::I32Store16(
            super::super::ops::memory_with_align(0, 1),
        ))),
        _ => body.push(Op::Leaf(Instruction::I32Store(
            super::super::ops::memory_with_align(0, 2),
        ))),
    }
}

fn load_discriminant(body: &mut Body, cases: usize) {
    match abi::layout::discriminant_width(cases) {
        1 => body.push(Op::Leaf(Instruction::I32Load8U(
            super::super::ops::memory_with_align(0, 0),
        ))),
        2 => body.push(Op::Leaf(Instruction::I32Load16U(
            super::super::ops::memory_with_align(0, 1),
        ))),
        _ => body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(0, 2),
        ))),
    }
}
