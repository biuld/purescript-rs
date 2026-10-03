//! Array element projection and concatenation lowering.

use super::aggregate::{nullable_reference_shape, reference_type, representation_shape};
use super::{BlockId, FunctionLowerer, layout_error};
use crate::BackendError;
use crate::cc::ReprId;
use crate::mir::instruction::Instruction;
use crate::mir::{NumericOp, Terminator};
use crate::types::{DefinedTypeId, RefType, ValueId, ValueType};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    /// Lowers `array.get`. A non-null reference element needs a nullable
    /// temporary plus a `ref.cast`, because the array element storage is
    /// nullable while the destination is not.
    pub(super) fn lower_array_get(
        &mut self,
        current: super::BlockId,
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
        index: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let type_index = self
            .layout
            .repr_index(representation)
            .map_err(|error| layout_error(span, error))?;
        let destination_type = self
            .values
            .iter()
            .find(|candidate| candidate.id == destination)
            .map(|candidate| candidate.ty)
            .ok_or_else(|| {
                vec![BackendError::invalid_ir(
                    "P9 MIR lowering",
                    span,
                    "array.get destination has no value declaration",
                )]
            })?;
        if let ValueType::Ref(reference) = destination_type
            && !reference.nullable
        {
            let temporary = self.fresh(ValueType::Ref(RefType {
                nullable: true,
                heap: reference.heap,
            }));
            self.append_instruction(
                current,
                Instruction::ArrayGet {
                    destination: temporary,
                    type_index,
                    value,
                    index,
                    span,
                },
                span,
            )?;
            self.append_instruction(
                current,
                Instruction::RefCast {
                    destination,
                    value: temporary,
                    reference,
                    span,
                },
                span,
            )?;
        } else {
            self.append_instruction(
                current,
                Instruction::ArrayGet {
                    destination,
                    type_index,
                    value,
                    index,
                    span,
                },
                span,
            )?;
        }
        Ok(())
    }

    /// Lowers `arrayAppend`: a fresh array of `len(left) + len(right)` whose
    /// first `len(left)` elements come from `left` and the rest from `right`.
    ///
    /// The MIR instruction set has no range copy, so this is two element-copy
    /// loops over the existing array instructions, the same shape the
    /// `stringToBytes` byte copy uses. `array.new_default` allocates; the first
    /// loop copies `left` into `0..len(left)`, and the second copies `right`
    /// into `len(left)..len(left) + len(right)`. Neither operand is mutated.
    pub(super) fn lower_array_append(
        &mut self,
        current: BlockId,
        destination: ValueId,
        representation: ReprId,
        left: ValueId,
        right: ValueId,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let type_index = self
            .layout
            .repr_index(representation)
            .map_err(|error| layout_error(span, error))?;
        // One element is moved through a raw storage-shaped temporary: a
        // reference element is read from and written back to nullable storage,
        // and a scalar keeps its shape.
        let element_shape = self
            .layout
            .array_element(representation)
            .map_err(|error| layout_error(span, error))?;
        let element_type = self
            .layout
            .value_type(&element_shape)
            .map_err(|error| layout_error(span, error))?;
        // A reference element is stored in nullable GC slots, so one element
        // moves through a nullable temporary. A `String` element is a reference
        // too, even though its logical shape is not `Reference`.
        let storage_type = match element_type {
            ValueType::Ref(reference) if !reference.nullable => ValueType::Ref(RefType {
                nullable: true,
                heap: reference.heap,
            }),
            other => other,
        };

        let length_left = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::ArrayLen {
                destination: length_left,
                value: left,
                span,
            },
            span,
        )?;
        let length_right = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::ArrayLen {
                destination: length_right,
                value: right,
                span,
            },
            span,
        )?;
        let total = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::Primitive {
                destination: total,
                op: NumericOp::I32Add,
                left: length_left,
                right: length_right,
                span,
            },
            span,
        )?;

        let destination_shape = representation_shape(representation);
        let storage_destination_shape = nullable_reference_shape(destination_shape);
        let array = self.fresh(
            self.layout
                .value_type(&storage_destination_shape)
                .map_err(|error| layout_error(span, error))?,
        );

        // Allocate, then copy `left` over `0..length_left`.
        let index = self.fresh(ValueType::I32);
        let header = self.new_block(vec![index]);
        let body = self.new_block(Vec::new());
        let exit = self.new_block(Vec::new());
        self.append_instruction(
            current,
            Instruction::ArrayNewSized {
                destination: array,
                type_index,
                length: total,
                span,
            },
            span,
        )?;
        let zero = self.append_int_constant(current, 0, span)?;
        self.set_terminator(
            current,
            Terminator::Jump {
                target: header,
                arguments: vec![zero],
                span,
            },
            span,
        )?;
        self.copy_loop(
            header,
            body,
            exit,
            index,
            length_left,
            left,
            array,
            type_index,
            storage_type,
            None,
            span,
        )?;

        // Copy `right` over `length_left..length_left + length_right`.
        let index_right = self.fresh(ValueType::I32);
        let header_right = self.new_block(vec![index_right]);
        let body_right = self.new_block(Vec::new());
        let exit_right = self.new_block(Vec::new());
        let zero = self.append_int_constant(exit, 0, span)?;
        self.set_terminator(
            exit,
            Terminator::Jump {
                target: header_right,
                arguments: vec![zero],
                span,
            },
            span,
        )?;
        self.copy_loop(
            header_right,
            body_right,
            exit_right,
            index_right,
            length_right,
            right,
            array,
            type_index,
            storage_type,
            Some(length_left),
            span,
        )?;

        // The loop result lands in the CC value this assignment defines; the
        // cast restores its non-null type from the nullable storage.
        self.append_instruction(
            exit_right,
            Instruction::RefCast {
                destination,
                value: array,
                reference: reference_type(&destination_shape, self.layout, span)?,
                span,
            },
            span,
        )?;
        Ok(exit_right)
    }

    /// Emits one copy loop over the blocks `header`/`body`/`exit`: for each
    /// `i` in `0..count`, write `source[i]` to `destination[base + i]`. `base`
    /// is absent when the destination starts at zero.
    #[allow(clippy::too_many_arguments)]
    fn copy_loop(
        &mut self,
        header: BlockId,
        body: BlockId,
        exit: BlockId,
        index: ValueId,
        count: ValueId,
        source: ValueId,
        destination: ValueId,
        type_index: DefinedTypeId,
        storage_type: ValueType,
        base: Option<ValueId>,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let condition = self.fresh(ValueType::Boolean);
        self.append_instruction(
            header,
            Instruction::Primitive {
                destination: condition,
                op: NumericOp::I32LtS,
                left: index,
                right: count,
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
        )?;

        let element = self.fresh(storage_type);
        self.append_instruction(
            body,
            Instruction::ArrayGet {
                destination: element,
                type_index,
                value: source,
                index,
                span,
            },
            span,
        )?;
        let target_index = match base {
            Some(base) => {
                let target = self.fresh(ValueType::I32);
                self.append_instruction(
                    body,
                    Instruction::Primitive {
                        destination: target,
                        op: NumericOp::I32Add,
                        left: index,
                        right: base,
                        span,
                    },
                    span,
                )?;
                target
            }
            None => index,
        };
        self.append_instruction(
            body,
            Instruction::ArraySet {
                type_index,
                value: destination,
                index: target_index,
                new_value: element,
                span,
            },
            span,
        )?;
        let one = self.append_int_constant(body, 1, span)?;
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

    fn append_int_constant(
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
}
