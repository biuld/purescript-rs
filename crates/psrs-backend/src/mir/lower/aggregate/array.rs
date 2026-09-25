use super::*;
use crate::mir::{NumericOp, UnaryOp};
use crate::types::RefType;

impl FunctionLowerer<'_> {
    pub(super) fn lower_array_map(
        &mut self,
        block: BlockId,
        value: ValueId,
        source: crate::cc::ReprId,
        target: crate::cc::ReprId,
        element: &ValueConversion,
        span: TextRange,
    ) -> Result<(BlockId, ValueId, ValueShape), Vec<BackendError>> {
        let source_type = self
            .layout
            .repr_index(source)
            .map_err(|error| layout_error(span, error))?;
        let target_type = self
            .layout
            .repr_index(target)
            .map_err(|error| layout_error(span, error))?;
        // Values crossing Wasm control labels must be defaultable locals. The
        // structured encoder tracks initialization of non-null references per
        // label scope, so retain nullable views through the loop and cast the
        // completed result back to its logical non-null shape at the exit.
        let source_shape = representation_shape(source);
        let source_storage_shape = nullable_reference_shape(source_shape);
        let source_storage = self.fresh(
            self.layout
                .value_type(&source_storage_shape)
                .map_err(|error| layout_error(span, error))?,
        );
        self.append_instruction(
            block,
            Instruction::RefCast {
                destination: source_storage,
                value,
                reference: reference_type(&source_storage_shape, self.layout, span)?,
                span,
            },
            span,
        )?;
        let length = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::ArrayLen {
                destination: length,
                value: source_storage,
                span,
            },
            span,
        )?;
        let destination_shape = representation_shape(target);
        let destination_storage_shape = nullable_reference_shape(destination_shape);
        let destination = self.fresh(
            self.layout
                .value_type(&destination_storage_shape)
                .map_err(|error| layout_error(span, error))?,
        );
        let index = self.fresh(ValueType::I32);
        let header = self.new_block(vec![index]);
        let body = self.new_block(Vec::new());
        let exit = self.new_block(Vec::new());
        self.append_instruction(
            block,
            Instruction::ArrayNewDefault {
                destination,
                type_index: target_type,
                length,
                source: source_storage,
                header,
                body,
                exit,
                index,
                span,
            },
            span,
        )?;
        let zero = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::Constant {
                destination: zero,
                value: 0,
                span,
            },
            span,
        )?;
        self.set_terminator(
            block,
            Terminator::Jump {
                target: header,
                arguments: vec![zero],
                span,
            },
            span,
        )?;

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
        )?;

        let source_element = self
            .layout
            .array_element(source)
            .map_err(|error| layout_error(span, error))?;
        let source_element_type = self
            .layout
            .value_type(&source_element)
            .map_err(|error| layout_error(span, error))?;
        let element_value = self.fresh(source_element_type);
        let array_element_value = if source_element == ValueShape::Boolean {
            self.fresh(ValueType::I32)
        } else if let ValueType::Ref(reference) = source_element_type
            && !reference.nullable
        {
            self.fresh(ValueType::Ref(RefType {
                nullable: true,
                heap: reference.heap,
            }))
        } else {
            element_value
        };
        if let ValueType::Ref(reference) = source_element_type
            && !reference.nullable
        {
            self.append_instruction(
                body,
                Instruction::ArrayGet {
                    destination: array_element_value,
                    type_index: source_type,
                    value: source_storage,
                    index,
                    span,
                },
                span,
            )?;
            self.append_instruction(
                body,
                Instruction::RefCast {
                    destination: element_value,
                    value: array_element_value,
                    reference,
                    span,
                },
                span,
            )?;
        } else if source_element == ValueShape::Boolean {
            self.append_instruction(
                body,
                Instruction::ArrayGet {
                    destination: array_element_value,
                    type_index: source_type,
                    value: source_storage,
                    index,
                    span,
                },
                span,
            )?;
            self.append_instruction(
                body,
                Instruction::UnaryPrimitive {
                    destination: element_value,
                    op: UnaryOp::I32ToBool,
                    value: array_element_value,
                    span,
                },
                span,
            )?;
        } else {
            self.append_instruction(
                body,
                Instruction::ArrayGet {
                    destination: element_value,
                    type_index: source_type,
                    value: source_storage,
                    index,
                    span,
                },
                span,
            )?;
        }
        let (body_end, converted, _) =
            self.lower_value_conversion(body, element_value, source_element, element, span)?;
        let target_element = self
            .layout
            .array_element(target)
            .map_err(|error| layout_error(span, error))?;
        let stored_value = if target_element == ValueShape::Boolean {
            let integer = self.fresh(ValueType::I32);
            self.append_instruction(
                body_end,
                Instruction::UnaryPrimitive {
                    destination: integer,
                    op: UnaryOp::BoolToI32,
                    value: converted,
                    span,
                },
                span,
            )?;
            integer
        } else {
            converted
        };
        self.append_instruction(
            body_end,
            Instruction::ArraySet {
                type_index: target_type,
                value: destination,
                index,
                new_value: stored_value,
                span,
            },
            span,
        )?;
        let one = self.fresh(ValueType::I32);
        self.append_instruction(
            body_end,
            Instruction::Constant {
                destination: one,
                value: 1,
                span,
            },
            span,
        )?;
        let increment = self.fresh(ValueType::I32);
        self.append_instruction(
            body_end,
            Instruction::Primitive {
                destination: increment,
                op: NumericOp::I32Add,
                left: index,
                right: one,
                span,
            },
            span,
        )?;
        self.set_terminator(
            body_end,
            Terminator::Jump {
                target: header,
                arguments: vec![increment],
                span,
            },
            span,
        )?;
        let result = self.fresh(
            self.layout
                .value_type(&destination_shape)
                .map_err(|error| layout_error(span, error))?,
        );
        self.append_instruction(
            exit,
            Instruction::RefCast {
                destination: result,
                value: destination,
                reference: reference_type(&destination_shape, self.layout, span)?,
                span,
            },
            span,
        )?;
        Ok((exit, result, destination_shape))
    }
}
