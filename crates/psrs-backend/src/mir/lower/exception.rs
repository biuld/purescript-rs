//! Discharge of `psrs:runtime-exception` provider calls into inline try
//! regions (EH-09). The precedent is the provider-dispatched storage call
//! lowering (`lower/storage_call.rs`); here the call site is not an ABI call
//! at all: the design's worked example shape
//! `Try { protected: B1, catch_all: B2 }` is built inline, where the protected
//! block applies the guarded callable and the handler supplies the caught
//! result, and a join returns the Boolean.
//!
//! The checked contract (`bindings/runtime/exception.rs`) fixes the shape:
//! exactly one guarded callable operand taking the erased `Unit`, and a
//! `Boolean` result. When the target profile disables `exceptions`, the call
//! is rejected at the binding's span — there is no fallback ABI.

use super::*;
use crate::bindings::ExceptionProvider;

impl FunctionLowerer<'_> {
    /// Discharges one `DirectCall` to a runtime-exception provider export and
    /// returns the block the remaining assignments continue in (the join).
    pub(super) fn lower_exception_call(
        &mut self,
        block: BlockId,
        destination: ValueId,
        provider: &ExceptionProvider,
        arguments: &[ValueId],
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        if !self.target.exceptions {
            return Err(vec![BackendError::new(
                "P9 MIR lowering",
                provider.span,
                format!(
                    "the call to `{}#{}` requires the WebAssembly exception-handling proposal, which the selected target disables",
                    provider.module, provider.function
                ),
            )
            .with_module(provider.source_module)]);
        }
        let [callable] = arguments else {
            return Err(vec![BackendError::invalid_ir(
                "P9 runtime binding",
                span,
                "the runtime-exception call has the wrong argument count",
            )]);
        };
        let result_type = self.value_type(destination).ok_or_else(|| {
            vec![BackendError::invalid_ir(
                "P9 runtime binding",
                span,
                "the runtime-exception result has no value type",
            )]
        })?;

        let protected_block = self.new_block(Vec::new());
        let handler_block = self.new_block(Vec::new());
        // The join receives the Boolean through one block parameter: the
        // assignment's destination itself, exactly like a branch merge.
        let join = self.new_block(vec![destination]);

        // The guarded application is the protected body's only content. The
        // callable's erased `Unit` operand is fabricated by the discharge.
        let unit = self.fresh(provider.operand);
        self.append_instruction(
            protected_block,
            Instruction::Constant {
                destination: unit,
                value: 0,
                span,
            },
            span,
        )?;
        let guarded = self.fresh(provider.guarded_result);
        let type_index = self
            .layout
            .signature_index(provider.callable)
            .map_err(|error| layout_error(span, error))?;
        let (closure_type, capture_array_type) = self
            .layout
            .closure_layout()
            .map_err(|error| layout_error(span, error))?;
        self.append_instruction(
            protected_block,
            Instruction::ClosureCall {
                destination: guarded,
                function: *callable,
                type_index,
                closure_type,
                capture_array_type,
                arguments: vec![unit],
                span,
            },
            span,
        )?;
        // The call returned normally: it did not throw.
        let false_value = self.fresh(result_type);
        self.append_instruction(
            protected_block,
            Instruction::Constant {
                destination: false_value,
                value: 0,
                span,
            },
            span,
        )?;
        self.set_terminator(
            protected_block,
            Terminator::Jump {
                target: join,
                arguments: vec![false_value],
                span,
            },
            span,
        )?;

        // The handler supplies the caught result.
        let true_value = self.fresh(result_type);
        self.append_instruction(
            handler_block,
            Instruction::Constant {
                destination: true_value,
                value: 1,
                span,
            },
            span,
        )?;
        self.set_terminator(
            handler_block,
            Terminator::Jump {
                target: join,
                arguments: vec![true_value],
                span,
            },
            span,
        )?;

        self.set_terminator(
            block,
            Terminator::Try {
                protected: protected_block,
                continuation: Some(join),
                clauses: vec![crate::mir::CatchClause::CatchAll {
                    target: handler_block,
                }],
                span,
            },
            span,
        )?;
        Ok(join)
    }
}
