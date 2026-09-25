use super::Structurer;
use crate::BackendError;
use crate::types::ValueId;
use crate::wasm::lower::local;
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

pub(super) trait ValueOps {
    fn load(
        &self,
        body: &mut Body,
        value: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>>;

    fn store(
        &self,
        body: &mut Body,
        destination: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>>;
}

impl ValueOps for Structurer<'_> {
    fn load(
        &self,
        body: &mut Body,
        value: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.emit_load(value, span, body)
    }

    fn store(
        &self,
        body: &mut Body,
        destination: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        body.push(Op::Leaf(Instruction::LocalSet(local(
            &self.locals,
            destination,
            span,
        )?)));
        Ok(())
    }
}
