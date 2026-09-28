//! Variant element copies for the generic list copy.
//!
//! A variant node stores its discriminant and the selected case payload; the
//! load rebuilds the active source case. Both walks share the canonical payload
//! offset, so the discriminant read matches the case written.
#![allow(clippy::vec_init_then_push)]

use super::super::wasm_error;
use super::{ListLoop, Projection, Structurer, concrete_ref};
use crate::BackendError;
use crate::abi;
use crate::abi::canonical::{
    CanonicalType, canonical_case_for_tag, payload_cases, source_tag_for_case, swaps_case_tags,
};
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

impl Structurer<'_> {
    /// Stores a variant node: write the discriminant and branch on the tag.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_variant_store(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        repr: crate::cc::ReprId,
        guest_cases: &[crate::cc::GuestCase],
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
        // The memory discriminant is canonical; a `result` swaps its tag
        // relative to the source `Either` ([DEC-13]).
        if swaps_case_tags(canonical) {
            body.push(Op::Leaf(Instruction::I32Eqz));
        }
        store_discriminant(body, cases.len());
        let payload_offset = abi::layout::variant_payload_offset(&cases)
            .ok_or_else(|| wasm_error(span, "variant"))?;
        let mut chain = Body::new();
        self.variant_store_chain(
            &mut chain,
            context,
            offset + payload_offset,
            canonical,
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
    pub(super) fn variant_store_chain(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        repr: crate::cc::ReprId,
        cases: &[Option<&CanonicalType>],
        guest_cases: &[crate::cc::GuestCase],
        path: &[Projection],
        scratch: u32,
        index: usize,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if index >= cases.len() {
            return Ok(());
        }
        // `index` is the guest constructor tag; the payload it carries is the
        // canonical case that tag maps to.
        let canonical_index = canonical_case_for_tag(canonical, index);
        let mut then_body = Body::new();
        if let Some(payload) = cases[canonical_index] {
            let case_type = self.case_index(repr, index as u32, span)?;
            let shape = guest_cases
                .get(index)
                .and_then(|case| case.fields.first())
                .map(|field| field.stored)
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
            canonical,
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
    pub(super) fn emit_variant_load(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        repr: crate::cc::ReprId,
        guest_cases: &[crate::cc::GuestCase],
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
            canonical,
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
    pub(super) fn variant_load_chain(
        &self,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        repr: crate::cc::ReprId,
        case_count: usize,
        cases: &[Option<&CanonicalType>],
        guest_cases: &[crate::cc::GuestCase],
        scratch: u32,
        index: usize,
        result: wasm_encoder::ValType,
        span: TextRange,
    ) -> Result<Body, Vec<BackendError>> {
        // `index` is the canonical discriminant (the value in memory); the guest
        // case it rebuilds uses the source tag that discriminant maps to.
        let source_tag = source_tag_for_case(canonical, index);
        let case_type = self.case_index(repr, source_tag as u32, span)?;
        let mut build = Body::new();
        build.push(Op::Leaf(Instruction::I32Const(source_tag as i32)));
        if let Some(payload) = cases[index] {
            let shape = guest_cases
                .get(source_tag)
                .and_then(|case| case.fields.first())
                .map(|field| field.stored)
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
            canonical,
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
    pub(super) fn emit_variant_free(
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
    pub(super) fn variant_free_chain(
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
