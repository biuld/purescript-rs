//! Emission of the structurer's try unit (EH-10): the try block's
//! instructions, one synthesized unwind block enclosing the `try_table`, and
//! the inline handler that copies the caught payload into its parameter
//! locals. The watermark save/restore of the stack-exhaustion budget hooks in
//! at the two marked points (EH-14).

use super::super::cfg::RegionPlan;
use super::super::{Structurer, block_traps, local, wasm_error};
use super::{Label, RegionOps};
use crate::BackendError;
use crate::mir::BlockId;
use crate::mir::Terminator;
use crate::wasm::convert::val_type;
use crate::wasm::{Body, Op, TagIndex};
use wasm_encoder::Instruction;

impl Structurer<'_> {
    /// Emits one try unit (EH-10). The shape follows the design's "Emitting a
    /// try unit": the try block's instructions run before arming, then one
    /// synthesized unwind block whose result type is the caught payload
    /// encloses the `try_table`, every clause of the try targets that block,
    /// and the handler's code follows the block's `end` and copies the
    /// delivered payload into the handler's parameter locals exactly the way
    /// jump arguments are copied.
    ///
    /// The watermark save/restore of the stack-exhaustion budget belongs at
    /// the two marked points (EH-14, the budget slice): a save of the depth
    /// global into a fresh local immediately before the unwind block, and the
    /// restore as the catch path's first action after it.
    pub(super) fn emit_try_unit(
        &self,
        try_block: BlockId,
        protected: &RegionPlan,
        handler: Option<BlockId>,
        payload: Option<crate::types::ValueType>,
        labels: &[Label],
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        let block = self
            .blocks
            .get(&try_block)
            .ok_or_else(|| wasm_error(self.function.span, "MIR block does not exist"))?;
        let span = block
            .terminator
            .as_ref()
            .map_or(self.function.span, |terminator| match terminator {
                Terminator::Try { span, .. } => *span,
                _ => self.function.span,
            });
        self.emit_block_instructions(&block.instructions, body)?;
        // A trapping try block never reaches its terminator, so it never arms
        // the region (the same rule as an ordinary trapping block).
        if block_traps(block) {
            return Ok(());
        }
        // EH-14, the watermark save: the region's fresh `i32` local takes the
        // depth counter's value immediately before the `try_table` arms. An
        // EH throw unwinds frames without their epilogues; the catch path
        // restores this value and settles every unwound frame at once.
        let budget = self.budget(span)?;
        let saved = self.watermark_local(try_block);
        budget.watermark_save(saved, body);
        // The clause labels resolve at the try_table's emission point against
        // the active labels not counting the region's own internal label;
        // `0` is the synthesized unwind block immediately enclosing it.
        let mut clauses = Vec::new();
        if let Some(Terminator::Try {
            clauses: mir_clauses,
            ..
        }) = &block.terminator
        {
            for clause in mir_clauses {
                clauses.push(match clause {
                    crate::mir::CatchClause::Catch { tag, .. } => crate::wasm::CatchClause::Catch {
                        tag: TagIndex(tag.0),
                        label: 0,
                    },
                    crate::mir::CatchClause::CatchAll { .. } => {
                        crate::wasm::CatchClause::CatchAll { label: 0 }
                    }
                    crate::mir::CatchClause::CatchAllRef { .. } => {
                        crate::wasm::CatchClause::CatchAllRef { label: 0 }
                    }
                });
            }
        }

        let mut try_body = Body::new();
        // The protected sub-region is emitted with the two anonymous entries
        // active: the unwind block, then the try_table's internal label.
        let mut protected_labels = labels.to_vec();
        protected_labels.push(Label::Unwind);
        protected_labels.push(Label::TryTable);
        self.emit_region(protected, &protected_labels, &mut try_body)?;

        body.push(Op::Block {
            result: payload.map(val_type),
            span,
            body: vec![
                Op::Try {
                    body: try_body,
                    clauses,
                    span,
                },
                // The try_table cannot complete normally: every protected
                // block's terminator transfers control out of the region, so
                // this path is statically dead and delivers no payload. (The
                // design sketch writes a `br` to the normal join here; the
                // join's arguments are per-path values, which the dead path
                // does not carry.)
                Op::Leaf(Instruction::Unreachable),
            ],
        });
        // The catch path lands here, payload on the stack (if any). The
        // watermark restore is the catch path's first action (EH-14): every
        // frame the throw unwound skipped its epilogue, and this one store
        // settles all of them.
        budget.watermark_restore(saved, body);
        let Some(handler) = handler else {
            return Ok(());
        };
        let handler_block = self
            .blocks
            .get(&handler)
            .ok_or_else(|| wasm_error(self.function.span, "MIR block does not exist"))?;
        for parameter in handler_block.parameters.iter().rev() {
            body.push(Op::Leaf(Instruction::LocalSet(local(
                &self.locals,
                *parameter,
                span,
            )?)));
        }
        self.emit_block_instructions(&handler_block.instructions, body)?;
        self.emit_terminator(handler_block, labels, body)
    }
}
