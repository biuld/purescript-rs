//! Linear array application with the callback ABI supplied by checked CC.
use super::aggregate::{nullable_reference_shape, reference_type, representation_shape};
use super::{BlockId, FunctionLowerer, layout_error};
use crate::BackendError;
use crate::cc::ReprId;
use crate::mir::instruction::Instruction;
use crate::mir::{NumericOp, Terminator};
use crate::types::{ValueId, ValueType};
use psrs_hir::SymbolId;
use psrs_span::TextRange;

struct ArrayLoop {
    index: ValueId,
    header: BlockId,
    body: BlockId,
    exit: BlockId,
}

impl FunctionLowerer<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_array_apply(
        &mut self,
        current: BlockId,
        destination: ValueId,
        functions: ValueId,
        values: ValueId,
        functions_repr: ReprId,
        values_repr: ReprId,
        result_repr: ReprId,
        invoker: SymbolId,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        // Non-null references crossing structured control labels require
        // defaultable storage locals. Logical views are restored at use sites.
        let functions = self.array_apply_storage(current, functions, functions_repr, span)?;
        let values = self.array_apply_storage(current, values, values_repr, span)?;
        let functions_length = self.fresh(ValueType::I32);
        let values_length = self.fresh(ValueType::I32);
        for (destination, value) in [(functions_length, functions), (values_length, values)] {
            self.append_instruction(
                current,
                Instruction::ArrayLen {
                    destination,
                    value,
                    span,
                },
                span,
            )?;
        }
        let zero = self.array_apply_constant(current, 0, span)?;
        // Source array lengths are signed Int. Reject unrepresentable lengths
        // and products, rather than wrapping and silently dropping callbacks.
        for length in [functions_length, values_length] {
            let negative = self.array_apply_numeric(
                current,
                NumericOp::I32LtS,
                length,
                zero,
                ValueType::Boolean,
                span,
            )?;
            self.append_instruction(
                current,
                Instruction::TrapIf {
                    condition: negative,
                    span,
                },
                span,
            )?;
        }
        let total = self.array_apply_numeric(
            current,
            NumericOp::I32Mul,
            functions_length,
            values_length,
            ValueType::I32,
            span,
        )?;
        let empty = self.array_apply_numeric(
            current,
            NumericOp::I32Eq,
            values_length,
            zero,
            ValueType::Boolean,
            span,
        )?;
        let guard = self.new_block(Vec::new());
        let allocate = self.new_block(Vec::new());
        self.set_terminator(
            current,
            Terminator::Branch {
                condition: empty,
                then_block: allocate,
                else_block: guard,
                span,
            },
            span,
        )?;
        let quotient = self.array_apply_numeric(
            guard,
            NumericOp::I32DivS,
            total,
            values_length,
            ValueType::I32,
            span,
        )?;
        let overflow = self.array_apply_numeric(
            guard,
            NumericOp::I32Ne,
            quotient,
            functions_length,
            ValueType::Boolean,
            span,
        )?;
        self.append_instruction(
            guard,
            Instruction::TrapIf {
                condition: overflow,
                span,
            },
            span,
        )?;
        self.set_terminator(
            guard,
            Terminator::Jump {
                target: allocate,
                arguments: Vec::new(),
                span,
            },
            span,
        )?;

        let destination_shape = representation_shape(result_repr);
        let array = self.fresh(
            self.layout
                .value_type(&nullable_reference_shape(destination_shape))
                .map_err(|error| layout_error(span, error))?,
        );
        let type_index = self
            .layout
            .repr_index(result_repr)
            .map_err(|error| layout_error(span, error))?;
        self.append_instruction(
            allocate,
            Instruction::ArrayNewSized {
                destination: array,
                type_index,
                length: total,
                span,
            },
            span,
        )?;
        let outer = self.array_apply_loop(allocate, functions_length, span)?;
        let callback_shape = self
            .layout
            .array_element(functions_repr)
            .map_err(|error| layout_error(span, error))?;
        let argument_shape = self
            .layout
            .array_element(values_repr)
            .map_err(|error| layout_error(span, error))?;
        let result_shape = self
            .layout
            .array_element(result_repr)
            .map_err(|error| layout_error(span, error))?;
        let callback = self.fresh(
            self.layout
                .value_type(&callback_shape)
                .map_err(|error| layout_error(span, error))?,
        );
        self.lower_array_get(
            outer.body,
            callback,
            functions_repr,
            functions,
            outer.index,
            span,
        )?;
        // Match the official loop: cache the function once for this complete
        // value traversal, including when the values array is empty.
        let cached_shape = nullable_reference_shape(callback_shape);
        let cached = self.fresh(
            self.layout
                .value_type(&cached_shape)
                .map_err(|error| layout_error(span, error))?,
        );
        self.append_instruction(
            outer.body,
            Instruction::RefCast {
                destination: cached,
                value: callback,
                reference: reference_type(&cached_shape, self.layout, span)?,
                span,
            },
            span,
        )?;
        let base = self.array_apply_numeric(
            outer.body,
            NumericOp::I32Mul,
            outer.index,
            values_length,
            ValueType::I32,
            span,
        )?;
        let inner = self.array_apply_loop(outer.body, values_length, span)?;
        let callback = self.fresh(
            self.layout
                .value_type(&callback_shape)
                .map_err(|error| layout_error(span, error))?,
        );
        self.append_instruction(
            inner.body,
            Instruction::RefCast {
                destination: callback,
                value: cached,
                reference: reference_type(&callback_shape, self.layout, span)?,
                span,
            },
            span,
        )?;
        let argument = self.fresh(
            self.layout
                .value_type(&argument_shape)
                .map_err(|error| layout_error(span, error))?,
        );
        let result = self.fresh(
            self.layout
                .value_type(&result_shape)
                .map_err(|error| layout_error(span, error))?,
        );
        self.lower_array_get(inner.body, argument, values_repr, values, inner.index, span)?;
        self.append_instruction(
            inner.body,
            Instruction::Call {
                destination: result,
                function: invoker,
                arguments: vec![callback, argument],
                span,
            },
            span,
        )?;
        let index = self.array_apply_numeric(
            inner.body,
            NumericOp::I32Add,
            base,
            inner.index,
            ValueType::I32,
            span,
        )?;
        self.append_instruction(
            inner.body,
            Instruction::ArraySet {
                type_index,
                value: array,
                index,
                new_value: result,
                span,
            },
            span,
        )?;
        self.array_apply_advance(inner.body, &inner, span)?;
        self.array_apply_advance(inner.exit, &outer, span)?;
        let exit = outer.exit;
        self.append_instruction(
            exit,
            Instruction::RefCast {
                destination,
                value: array,
                reference: reference_type(&destination_shape, self.layout, span)?,
                span,
            },
            span,
        )?;
        Ok(exit)
    }

    fn array_apply_loop(
        &mut self,
        start: BlockId,
        length: ValueId,
        span: TextRange,
    ) -> Result<ArrayLoop, Vec<BackendError>> {
        let index = self.fresh(ValueType::I32);
        let header = self.new_block(vec![index]);
        let body = self.new_block(Vec::new());
        let exit = self.new_block(Vec::new());
        let zero = self.array_apply_constant(start, 0, span)?;
        self.set_terminator(
            start,
            Terminator::Jump {
                target: header,
                arguments: vec![zero],
                span,
            },
            span,
        )?;
        let condition = self.array_apply_numeric(
            header,
            NumericOp::I32LtS,
            index,
            length,
            ValueType::Boolean,
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
        Ok(ArrayLoop {
            index,
            header,
            body,
            exit,
        })
    }

    fn array_apply_advance(
        &mut self,
        block: BlockId,
        loop_: &ArrayLoop,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let one = self.array_apply_constant(block, 1, span)?;
        let next = self.array_apply_numeric(
            block,
            NumericOp::I32Add,
            loop_.index,
            one,
            ValueType::I32,
            span,
        )?;
        self.set_terminator(
            block,
            Terminator::Jump {
                target: loop_.header,
                arguments: vec![next],
                span,
            },
            span,
        )
    }

    fn array_apply_storage(
        &mut self,
        block: BlockId,
        value: ValueId,
        representation: ReprId,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let shape = nullable_reference_shape(representation_shape(representation));
        let destination = self.fresh(
            self.layout
                .value_type(&shape)
                .map_err(|error| layout_error(span, error))?,
        );
        self.append_instruction(
            block,
            Instruction::RefCast {
                destination,
                value,
                reference: reference_type(&shape, self.layout, span)?,
                span,
            },
            span,
        )?;
        Ok(destination)
    }

    fn array_apply_constant(
        &mut self,
        block: BlockId,
        value: i32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let destination = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::Constant {
                destination,
                value,
                span,
            },
            span,
        )?;
        Ok(destination)
    }

    #[allow(clippy::too_many_arguments)]
    fn array_apply_numeric(
        &mut self,
        block: BlockId,
        op: NumericOp,
        left: ValueId,
        right: ValueId,
        ty: ValueType,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let destination = self.fresh(ty);
        self.append_instruction(
            block,
            Instruction::Primitive {
                destination,
                op,
                left,
                right,
                span,
            },
            span,
        )?;
        Ok(destination)
    }
}
