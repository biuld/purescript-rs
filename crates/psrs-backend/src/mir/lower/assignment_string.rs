//! Lowering of the `String` <-> `Array Int` byte conversions.
//!
//! `stringToBytes` widens each UTF-8 byte of a GC string into one `Int` of an
//! `Array Int`; it is lossless because a source string is already a sequence of
//! Unicode scalar values. `bytesToString` is the checked direction: it
//! range-checks every element against `0..255` and validates the whole byte
//! sequence as UTF-8, matching Rust's `String::from_utf8` rather than a lossy
//! decode ([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).

use super::aggregate::{nullable_reference_shape, reference_type, representation_shape};
use super::{BlockId, FunctionLowerer, layout_error};
use crate::BackendError;
use crate::abi::canonical::CanonicalType;
use crate::cc::GuestLayout;
use crate::cc::{ReprId, ValueShape};
use crate::mir::instruction::{Instruction, ListDirection};
use crate::mir::{NumericOp, Terminator, UnaryOp};
use crate::types::{ValueId, ValueType};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    /// Lowers `stringToBytes`: every UTF-8 byte of the GC string becomes one
    /// `Int` in `0..255`. The copy is lossless because a source string is
    /// already a sequence of Unicode scalar values.
    pub(super) fn lower_string_to_bytes(
        &mut self,
        current: BlockId,
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let string_type = self.layout.string_index().ok_or_else(|| {
            layout_error(span, crate::mir::layout::LayoutError::UnknownRepresentation)
        })?;
        let type_index = self
            .layout
            .repr_index(representation)
            .map_err(|error| layout_error(span, error))?;
        let destination_shape = representation_shape(representation);
        let storage_shape = nullable_reference_shape(destination_shape);
        let destination_value = self.fresh(
            self.layout
                .value_type(&storage_shape)
                .map_err(|error| layout_error(span, error))?,
        );
        let length = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::ArrayLen {
                destination: length,
                value,
                span,
            },
            span,
        )?;
        let index = self.fresh(ValueType::I32);
        let header = self.new_block(vec![index]);
        let body = self.new_block(Vec::new());
        let exit = self.new_block(Vec::new());
        self.append_instruction(
            current,
            Instruction::StringToBytes {
                destination: destination_value,
                type_index,
                string_type,
                value,
                header,
                body,
                exit,
                index,
                span,
            },
            span,
        )?;
        self.enter_loop(current, header, span)?;
        self.loop_header(header, body, exit, index, length, span)?;

        let byte = self.fresh(ValueType::I32);
        self.append_instruction(
            body,
            Instruction::ArrayGetU {
                destination: byte,
                type_index: string_type,
                value,
                index,
                span,
            },
            span,
        )?;
        self.append_instruction(
            body,
            Instruction::ArraySet {
                type_index,
                value: destination_value,
                index,
                new_value: byte,
                span,
            },
            span,
        )?;
        self.loop_step(body, header, index, span)?;

        // The loop result lands in the CC value this assignment defines; the
        // cast restores its non-null type from the nullable storage.
        self.append_instruction(
            exit,
            Instruction::RefCast {
                destination,
                value: destination_value,
                reference: reference_type(&destination_shape, self.layout, span)?,
                span,
            },
            span,
        )?;
        Ok(exit)
    }

    /// Lowers `bytesToString`: every element must be a canonical byte, and the
    /// byte sequence must be well-formed UTF-8. Either violation traps.
    ///
    /// The elements are staged in a transient linear buffer because that is
    /// where the canonical ABI boundary already validates text, and it holds
    /// one implementation of the rule. Staging uses the ordinary list-copy
    /// instruction, whose byte store range-checks each element against `0..255`
    /// so the `i32.store8` cannot silently truncate it; the boundary helper then
    /// validates the sequence and builds the GC string
    /// ([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
    pub(super) fn lower_bytes_to_string(
        &mut self,
        current: BlockId,
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let string_shape = ValueShape::String;
        let array_type = self
            .layout
            .repr_index(representation)
            .map_err(|error| layout_error(span, error))?;
        let length = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::ArrayLen {
                destination: length,
                value,
                span,
            },
            span,
        )?;
        let capacity = self.allocated_size(current, length, span)?;
        let zero = self.constant(current, 0, span)?;
        let align = self.constant(current, 1, span)?;
        let buffer = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::Call {
                destination: buffer,
                function: crate::abi::REALLOC_SYMBOL,
                arguments: vec![zero, zero, align, capacity],
                span,
            },
            span,
        )?;
        self.append_instruction(
            current,
            Instruction::ListCopy {
                direction: ListDirection::Store,
                array: value,
                array_type,
                // A scalar element has no record projection; the struct type is
                // a placeholder in that case.
                struct_type: crate::types::DefinedTypeId(0),
                pointer: buffer,
                length,
                element: CanonicalType::Int {
                    width: 8,
                    signed: false,
                },
                element_guest: GuestLayout::Scalar {
                    shape: ValueShape::Integer,
                },
                span,
            },
            span,
        )?;

        let string = self.fresh(
            self.layout
                .value_type(&string_shape)
                .map_err(|error| layout_error(span, error))?,
        );
        self.append_instruction(
            current,
            Instruction::Call {
                destination: string,
                function: crate::abi::BYTES_TO_STRING_SYMBOL,
                arguments: vec![buffer, length],
                span,
            },
            span,
        )?;
        // The staging buffer is call-local, so free it before source continues.
        let discarded = self.fresh(ValueType::I32);
        let free_size = self.constant(current, 0, span)?;
        self.append_instruction(
            current,
            Instruction::Call {
                destination: discarded,
                function: crate::abi::REALLOC_SYMBOL,
                arguments: vec![buffer, capacity, align, free_size],
                span,
            },
            span,
        )?;
        self.append_instruction(
            current,
            Instruction::RefCast {
                destination,
                value: string,
                reference: reference_type(&string_shape, self.layout, span)?,
                span,
            },
            span,
        )?;
        Ok(current)
    }

    /// The size a staging buffer is actually allocated with. An empty array
    /// still needs a valid allocation, so never request zero bytes:
    /// `cabi_realloc` returns null for that, and a null staging pointer would
    /// look like a valid empty buffer.
    fn allocated_size(
        &mut self,
        block: BlockId,
        length: ValueId,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let zero = self.constant(block, 0, span)?;
        let is_empty = self.fresh(ValueType::Boolean);
        self.append_instruction(
            block,
            Instruction::Primitive {
                destination: is_empty,
                op: NumericOp::I32Eq,
                left: length,
                right: zero,
                span,
            },
            span,
        )?;
        let empty_size = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::UnaryPrimitive {
                destination: empty_size,
                op: UnaryOp::BoolToI32,
                value: is_empty,
                span,
            },
            span,
        )?;
        let capacity = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::Primitive {
                destination: capacity,
                op: NumericOp::I32Or,
                left: length,
                right: empty_size,
                span,
            },
            span,
        )?;
        Ok(capacity)
    }

    fn constant(
        &mut self,
        block: BlockId,
        value: i32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let constant = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::Constant {
                destination: constant,
                value,
                span,
            },
            span,
        )?;
        Ok(constant)
    }

    /// Enters the loop at index zero.
    fn enter_loop(
        &mut self,
        current: BlockId,
        header: BlockId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let zero = self.constant(current, 0, span)?;
        self.set_terminator(
            current,
            Terminator::Jump {
                target: header,
                arguments: vec![zero],
                span,
            },
            span,
        )
    }

    /// Branches to `body` while `index < length`, and to `exit` otherwise.
    fn loop_header(
        &mut self,
        header: BlockId,
        body: BlockId,
        exit: BlockId,
        index: ValueId,
        length: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let condition = self.fresh(ValueType::Boolean);
        self.append_instruction(
            header,
            Instruction::Primitive {
                destination: condition,
                op: NumericOp::I32LtS,
                left: index,
                right: length,
                span,
            },
            span,
        )?;
        self.set_terminator(
            header,
            Terminator::Branch {
                condition,
                then_block: body,
                else_block: exit,
                span,
            },
            span,
        )
    }

    /// Advances `index` and jumps back to the header.
    fn loop_step(
        &mut self,
        body: BlockId,
        header: BlockId,
        index: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let one = self.constant(body, 1, span)?;
        let next = self.fresh(ValueType::I32);
        self.append_instruction(
            body,
            Instruction::Primitive {
                destination: next,
                op: NumericOp::I32Add,
                left: index,
                right: one,
                span,
            },
            span,
        )?;
        self.set_terminator(
            body,
            Terminator::Jump {
                target: header,
                arguments: vec![next],
                span,
            },
            span,
        )
    }
}
