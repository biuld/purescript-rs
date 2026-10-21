//! Discharge of `psrs:runtime-exception` provider calls into inline try
//! regions and throw terminators. The precedent is the provider-dispatched
//! storage call lowering (`lower/storage_call.rs`); here the call site is not
//! an ABI call at all, so the region is built inline.
//!
//! The checked contract (`bindings/runtime/exception/mod.rs`) fixes each shape:
//!
//! - `checkThrows`: one guarded callable taking the erased `Unit`, a `Boolean`
//!   result.
//! - `throw`: one `String` message, raised on the planned source-error tag.
//! - `catch`: a body callable taking `Unit` and a handler callable taking the
//!   `String` payload. Exactly one named `Catch` clause, so the region's
//!   clauses stay uniform; any other tag propagates.
//!
//! When the target profile disables `exceptions`, the call is rejected at the
//! binding's span — there is no fallback ABI.

use super::*;
use crate::bindings::{ExceptionOperation, ExceptionProvider};

impl FunctionLowerer<'_> {
    /// Discharges one `DirectCall` to a runtime-exception provider export and
    /// returns the block the remaining assignments continue in. A `throw` has
    /// no continuation: it terminates the block and returns it.
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
        match provider.operation {
            ExceptionOperation::CheckThrows => {
                self.lower_check_throws(block, destination, provider, arguments, span)
            }
            ExceptionOperation::Throw => self.lower_throw(block, provider, arguments, span),
            ExceptionOperation::Catch => {
                self.lower_catch(block, destination, provider, arguments, span)
            }
        }
    }

    /// `checkThrows`: apply the guarded callable inside a catch-all region and
    /// join `false` (returned) with `true` (threw).
    fn lower_check_throws(
        &mut self,
        block: BlockId,
        destination: ValueId,
        provider: &ExceptionProvider,
        arguments: &[ValueId],
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let [callable] = arguments else {
            return Err(wrong_argument_count(span));
        };
        let callable_signature = provider
            .callable
            .ok_or_else(|| missing_contract(span, "callable"))?;
        let operand_type = provider
            .operand
            .ok_or_else(|| missing_contract(span, "operand"))?;
        let guarded_result = provider
            .guarded_result
            .ok_or_else(|| missing_contract(span, "result"))?;
        let result_type = self
            .value_type(destination)
            .ok_or_else(|| result_has_no_type(span))?;

        let protected_block = self.new_block(Vec::new());
        let handler_block = self.new_block(Vec::new());
        // The join receives the Boolean through one block parameter: the
        // assignment's destination itself, exactly like a branch merge.
        let join = self.new_block(vec![destination]);

        // The guarded application is the protected body's only content. The
        // callable's erased `Unit` operand is fabricated by the discharge.
        let unit = self.fresh(operand_type);
        self.append_instruction(
            protected_block,
            Instruction::Constant {
                destination: unit,
                value: 0,
                span,
            },
            span,
        )?;
        let guarded = self.fresh(guarded_result);
        let type_index = self
            .layout
            .signature_index(callable_signature)
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

    /// `throw`: raise the source-error tag. The block is terminated, so it has
    /// no normal continuation; the destination is never written.
    fn lower_throw(
        &mut self,
        block: BlockId,
        provider: &ExceptionProvider,
        arguments: &[ValueId],
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let [message] = arguments else {
            return Err(wrong_argument_count(span));
        };
        let tag = provider.tag.ok_or_else(|| missing_contract(span, "tag"))?;
        let payload = provider
            .payload
            .ok_or_else(|| missing_contract(span, "payload"))?;
        if self.value_type(*message) != Some(payload) {
            return Err(vec![BackendError::invalid_ir(
                "P9 runtime binding",
                span,
                "the runtime-exception message does not carry the source-error payload",
            )]);
        }
        self.set_terminator(
            block,
            Terminator::Throw {
                tag,
                arguments: vec![*message],
                handler: None,
                span,
            },
            span,
        )?;
        Ok(block)
    }

    /// `catch`: arm one named clause around the body callable and apply the
    /// handler callable to the caught message. Both paths join on the
    /// destination.
    fn lower_catch(
        &mut self,
        block: BlockId,
        destination: ValueId,
        provider: &ExceptionProvider,
        arguments: &[ValueId],
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let [body, handler] = arguments else {
            return Err(wrong_argument_count(span));
        };
        let body_signature = provider
            .callable
            .ok_or_else(|| missing_contract(span, "body callable"))?;
        let handler_signature = provider
            .handler
            .ok_or_else(|| missing_contract(span, "handler callable"))?;
        let operand_type = provider
            .operand
            .ok_or_else(|| missing_contract(span, "operand"))?;
        let payload = provider
            .payload
            .ok_or_else(|| missing_contract(span, "payload"))?;
        let tag = provider.tag.ok_or_else(|| missing_contract(span, "tag"))?;
        let result_type = self
            .value_type(destination)
            .ok_or_else(|| result_has_no_type(span))?;
        let (closure_type, capture_array_type) = self
            .layout
            .closure_layout()
            .map_err(|error| layout_error(span, error))?;

        // The handler block receives the caught message as its parameter.
        let caught = self.fresh(payload);
        let protected_block = self.new_block(Vec::new());
        let handler_block = self.new_block(vec![caught]);
        let join = self.new_block(vec![destination]);

        let unit = self.fresh(operand_type);
        self.append_instruction(
            protected_block,
            Instruction::Constant {
                destination: unit,
                value: 0,
                span,
            },
            span,
        )?;
        let guarded = self.fresh(result_type);
        let body_type_index = self
            .layout
            .signature_index(body_signature)
            .map_err(|error| layout_error(span, error))?;
        self.append_instruction(
            protected_block,
            Instruction::ClosureCall {
                destination: guarded,
                function: *body,
                type_index: body_type_index,
                closure_type,
                capture_array_type,
                arguments: vec![unit],
                span,
            },
            span,
        )?;
        self.set_terminator(
            protected_block,
            Terminator::Jump {
                target: join,
                arguments: vec![guarded],
                span,
            },
            span,
        )?;

        let recovered = self.fresh(result_type);
        let handler_type_index = self
            .layout
            .signature_index(handler_signature)
            .map_err(|error| layout_error(span, error))?;
        self.append_instruction(
            handler_block,
            Instruction::ClosureCall {
                destination: recovered,
                function: *handler,
                type_index: handler_type_index,
                closure_type,
                capture_array_type,
                arguments: vec![caught],
                span,
            },
            span,
        )?;
        self.set_terminator(
            handler_block,
            Terminator::Jump {
                target: join,
                arguments: vec![recovered],
                span,
            },
            span,
        )?;

        self.set_terminator(
            block,
            Terminator::Try {
                protected: protected_block,
                continuation: Some(join),
                clauses: vec![crate::mir::CatchClause::Catch {
                    tag,
                    target: handler_block,
                }],
                span,
            },
            span,
        )?;
        Ok(join)
    }
}

fn wrong_argument_count(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::invalid_ir(
        "P9 runtime binding",
        span,
        "the runtime-exception call has the wrong argument count",
    )]
}

fn missing_contract(span: TextRange, what: &str) -> Vec<BackendError> {
    vec![BackendError::invalid_ir(
        "P9 runtime binding",
        span,
        format!("the runtime-exception binding has no checked {what}"),
    )]
}

fn result_has_no_type(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::invalid_ir(
        "P9 runtime binding",
        span,
        "the runtime-exception result has no value type",
    )]
}
