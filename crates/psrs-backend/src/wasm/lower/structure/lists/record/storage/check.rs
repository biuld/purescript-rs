use super::*;

impl Structurer<'_> {
    pub(super) fn check_record_storage(
        &self,
        body: &mut Body,
        context: &ListLoop,
        path: &[Projection],
        types: &RecordTypes,
        labels: &[String],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        for column in [0, 1] {
            self.record_column(body, context, path, types, column, span)?;
            body.push(Op::Leaf(Instruction::ArrayLen));
            body.push(Op::Leaf(Instruction::I32Const(labels.len() as i32)));
            body.push(Op::Leaf(Instruction::I32Eq));
            require(body, span);
        }
        for (index, label) in labels.iter().enumerate() {
            self.record_key(body, context, path, types, index, span)?;
            body.push(Op::Leaf(Instruction::ArrayLen));
            body.push(Op::Leaf(Instruction::I32Const(label.len() as i32)));
            body.push(Op::Leaf(Instruction::I32Eq));
            require(body, span);
            for (byte_index, byte) in label.bytes().enumerate() {
                self.record_key(body, context, path, types, index, span)?;
                body.push(Op::Leaf(Instruction::I32Const(byte_index as i32)));
                body.push(Op::Leaf(Instruction::ArrayGetU(types.string)));
                body.push(Op::Leaf(Instruction::I32Const(i32::from(byte))));
                body.push(Op::Leaf(Instruction::I32Eq));
                require(body, span);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn record_key(
        &self,
        body: &mut Body,
        context: &ListLoop,
        path: &[Projection],
        types: &RecordTypes,
        index: usize,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.record_column(body, context, path, types, 0, span)?;
        body.push(Op::Leaf(Instruction::I32Const(index as i32)));
        body.push(Op::Leaf(Instruction::ArrayGet(types.labels)));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn record_column(
        &self,
        body: &mut Body,
        context: &ListLoop,
        path: &[Projection],
        types: &RecordTypes,
        column: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.emit_project(body, context, path, span)?;
        body.push(Op::Leaf(Instruction::RefCastNonNull(
            crate::wasm::convert::heap_type(crate::types::HeapType::Index(
                crate::types::DefinedTypeId(types.header),
            )),
        )));
        body.push(Op::Leaf(Instruction::StructGet {
            struct_type_index: types.header,
            field_index: column,
        }));
        Ok(())
    }
}

fn require(body: &mut Body, span: TextRange) {
    body.push(Op::Leaf(Instruction::I32Eqz));
    body.push(Op::If {
        then_body: vec![Op::Leaf(Instruction::Unreachable)],
        else_body: Vec::new(),
        result: None,
        span,
    });
}
