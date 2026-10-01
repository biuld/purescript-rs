//! The recursive list-element dispatch and variant copies.
//!
//! One `(CanonicalType, GuestLayout)` pair drives the store, load, and free
//! walks. Variants branch on the tag; a nested list or fixed list recurses
//! through its own loop. The guest projection walks from the enclosing array
//! element to the nested value.
#![allow(clippy::vec_init_then_push)]

use super::{ListLoop, Projection, Structurer};
use crate::BackendError;
use crate::abi::canonical::CanonicalType;
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
        // A WIT `list<u8>` is `Array Int`: each element is range-checked before
        // it is narrowed to one canonical byte (DEC-16).
        if matches!(
            canonical,
            CanonicalType::Int {
                width: 8,
                signed: false
            }
        ) {
            return self.emit_byte_store(body, context, offset, path, span);
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
                .emit_nested_list_store(
                    body,
                    context,
                    offset,
                    inner,
                    *repr,
                    element.stored,
                    path,
                    span,
                ),
            (
                CanonicalType::FixedList { element, length },
                GuestLayout::Array {
                    repr,
                    element: field,
                },
            ) => self.emit_fixed_list_store(
                body,
                context,
                offset,
                element,
                *length,
                *repr,
                field.stored,
                path,
                span,
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
            (CanonicalType::List(inner), GuestLayout::Array { repr, element }) => self
                .emit_nested_list_load(body, context, offset, inner, *repr, element.stored, span),
            (
                CanonicalType::FixedList { element, length },
                GuestLayout::Array {
                    repr,
                    element: field,
                },
            ) => self.emit_fixed_list_load(
                body,
                context,
                offset,
                element,
                *length,
                *repr,
                field.stored,
                span,
            ),
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
}
