//! Canonical list records use the same labelled store as language erasure.
use super::*;
use crate::abi::canonical::CanonicalField;
use crate::cc::{Field, GuestLayout, RecordStorage};
use crate::wasm::{Body, Op};
use wasm_encoder::Instruction;

mod check;

struct RecordTypes {
    header: u32,
    labels: u32,
    values: u32,
    string: u32,
}

impl Structurer<'_> {
    fn record_storage_types(
        &self,
        storage: RecordStorage,
        span: TextRange,
    ) -> Result<RecordTypes, Vec<BackendError>> {
        let layout = self
            .layout
            .ok_or_else(|| wasm_error(span, "record storage copy has no target layout"))?;
        if layout.representation_table().record_storage() != Some(storage) {
            return Err(wasm_error(
                span,
                "record storage view disagrees with its representation owner",
            ));
        }
        Ok(RecordTypes {
            header: self.repr_index(storage.representation(), span)?,
            labels: self.repr_index(storage.labels(), span)?,
            values: self.repr_index(storage.values(), span)?,
            string: layout
                .string_index()
                .ok_or_else(|| wasm_error(span, "record labels have no UTF-8 storage"))?
                .0,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::wasm::lower::structure::lists) fn emit_record_storage_store(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &[CanonicalField],
        storage: RecordStorage,
        labels: &[String],
        fields: &[Field],
        path: &[Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        check_view(labels, fields, span)?;
        let types = self.record_storage_types(storage, span)?;
        self.check_record_storage(body, context, path, &types, labels, span)?;
        let layouts = abi::layout::record_fields(
            canonical
                .iter()
                .map(|field| abi::layout::parameter_layout(&field.ty)),
        )
        .ok_or_else(|| wasm_error(span, "canonical record storage has no field layout"))?;
        for (field, (field_offset, _)) in canonical.iter().zip(layouts) {
            let label = abi::source_field_name(&field.name);
            let index = labels
                .iter()
                .position(|existing| *existing == label)
                .ok_or_else(|| wasm_error(span, "canonical record storage is missing a field"))?;
            let guest = self.record_stored_field(&fields[index], span)?;
            let mut nested = path.to_vec();
            nested.push(Projection::RecordField {
                header: types.header,
                values: types.values,
                field: index as u32,
            });
            self.emit_store_node(
                body,
                context,
                offset + field_offset,
                &field.ty,
                &guest,
                &nested,
                span,
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::wasm::lower::structure::lists) fn emit_record_storage_load(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &[CanonicalField],
        storage: RecordStorage,
        labels: &[String],
        fields: &[Field],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        check_view(labels, fields, span)?;
        let types = self.record_storage_types(storage, span)?;
        let layouts = abi::layout::record_fields(
            canonical
                .iter()
                .map(|field| abi::layout::parameter_layout(&field.ty)),
        )
        .ok_or_else(|| wasm_error(span, "canonical record storage has no field layout"))?;
        self.record_label_array(body, &types, labels);
        for (index, label) in labels.iter().enumerate() {
            let field = canonical
                .iter()
                .position(|field| abi::source_field_name(&field.name) == *label)
                .ok_or_else(|| wasm_error(span, "canonical record storage is missing a field"))?;
            let guest = self.record_stored_field(&fields[index], span)?;
            self.emit_load_node(
                body,
                context,
                offset + layouts[field].0,
                &canonical[field].ty,
                &guest,
                span,
            )?;
        }
        body.push(Op::Leaf(Instruction::ArrayNewFixed {
            array_type_index: types.values,
            array_size: labels.len() as u32,
        }));
        body.push(Op::Leaf(Instruction::StructNew(types.header)));
        Ok(())
    }

    fn record_stored_field(
        &self,
        field: &Field,
        span: TextRange,
    ) -> Result<GuestLayout, Vec<BackendError>> {
        self.resolve_field(
            &Field {
                value: field.value.clone(),
                stored: crate::cc::payload::erased_shape(),
            },
            span,
        )
    }

    fn record_label_array(&self, body: &mut Body, types: &RecordTypes, labels: &[String]) {
        // UTF-8 bytes are fixed compile-time metadata. This construction uses
        // the ordinary String storage type and has no source/HIR type tags.
        for label in labels {
            for byte in label.bytes() {
                body.push(Op::Leaf(Instruction::I32Const(i32::from(byte))));
            }
            body.push(Op::Leaf(Instruction::ArrayNewFixed {
                array_type_index: types.string,
                array_size: label.len() as u32,
            }));
        }
        body.push(Op::Leaf(Instruction::ArrayNewFixed {
            array_type_index: types.labels,
            array_size: labels.len() as u32,
        }));
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::wasm::lower::structure::lists) fn emit_record_storage_flags_store(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        names: &[String],
        storage: RecordStorage,
        labels: &[String],
        path: &[Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let types = self.record_storage_types(storage, span)?;
        self.check_record_storage(body, context, path, &types, labels, span)?;
        self.emit_flags_store_layout(
            body,
            context,
            offset,
            names,
            storage.representation(),
            labels,
            Some(storage),
            path,
            span,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::wasm::lower::structure::lists) fn emit_record_storage_flags_load(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        names: &[String],
        storage: RecordStorage,
        labels: &[String],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let types = self.record_storage_types(storage, span)?;
        self.record_label_array(body, &types, labels);
        self.emit_flags_values(body, context, offset, names, labels, true, span)?;
        body.push(Op::Leaf(Instruction::ArrayNewFixed {
            array_type_index: types.values,
            array_size: labels.len() as u32,
        }));
        body.push(Op::Leaf(Instruction::StructNew(types.header)));
        Ok(())
    }
}

fn check_view(
    labels: &[String],
    fields: &[Field],
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    if labels.len() != fields.len() || labels.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(wasm_error(
            span,
            "record storage copy has invalid logical field metadata",
        ));
    }
    Ok(())
}
