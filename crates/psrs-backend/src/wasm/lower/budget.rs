//! The stack-exhaustion budget's emission (EH-13, EH-14): the prologue check
//! and epilogue/tail decrements every lowered function carries when the
//! target enables `exceptions`, plus the try-region watermark save/restore
//! that keeps the depth counter from leaking across an EH throw. The weight
//! and budget constants live in [`crate::mir::budget`]; this module owns only
//! their instruction shape.
//!
//! Shape (design "Budget emission"):
//!
//! ```text
//! prologue:  depth += W_f; if depth > STACK_BUDGET then throw $stack-exhaustion
//! epilogue:  depth -= W_f                      // before every `return`
//! tail:      depth -= W_f                      // before every `return_call*`
//! trap:      nothing                           // the leak is unobservable
//! ```

use super::structure::Structurer;
use super::wasm_error;
use crate::BackendError;
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

/// The budget context one lowered module carries: the depth global's final
/// index, the `psrs:stack-exhaustion` tag's final index, and the profile-fixed
/// budget constant. Constructed by `lower_module_with_plan` when
/// `target.exceptions` is enabled; `None` lowers every function unchanged.
#[derive(Clone, Copy, Debug)]
pub(super) struct Budget {
    /// The depth counter global: a mutable `i32` initialized to 0, allocated
    /// after the literal-interning globals and never exported.
    pub(super) depth: u32,
    /// The module-local index of the `psrs:stack-exhaustion` tag (empty
    /// payload). The prologue's overflow path throws it; it unwinds out of
    /// the function because every try region arms only after the prologue.
    pub(super) tag: u32,
    /// The budget the prologue check compares against.
    pub(super) stack_budget: i32,
}

impl Budget {
    /// Plans the module's budget context when `target.exceptions` is enabled,
    /// appending the stack-exhaustion tag's function type and its `TagDef`.
    /// The tag is appended after the MIR tags, so a MIR `TagId`'s position —
    /// the final `TagIndex` — is unchanged. `literal_global_count` is the
    /// number of already-planned interning globals: the depth global takes
    /// the next global index.
    pub(super) fn for_module(
        module: &crate::mir::Module,
        target: crate::capability::TargetCapabilities,
        defined: u32,
        types: &mut Vec<super::FuncType>,
        tag_defs: &mut Vec<super::TagDef>,
        literal_global_count: usize,
    ) -> Result<Option<Budget>, Vec<BackendError>> {
        if !target.exceptions {
            return Ok(None);
        }
        if !target.mutable_globals {
            return Err(wasm_error(
                module.span,
                "the stack-exhaustion budget requires mutable WebAssembly globals",
            ));
        }
        // The depth counter: one mutable `i32` global initialized to 0,
        // allocated after the literal-interning globals, invisible to source
        // and never exported.
        let depth = super::GlobalIndex(literal_global_count as u32);
        let tag = super::TagIndex(module.tags.len() as u32);
        let tag_type = super::TypeIndex(defined + types.len() as u32);
        types.push(super::FuncType {
            parameters: Vec::new(),
            results: Vec::new(),
        });
        tag_defs.push(super::TagDef {
            type_index: tag_type,
            span: module.span,
        });
        Ok(Some(Budget {
            depth: depth.0,
            tag: tag.0,
            stack_budget: crate::mir::budget::STACK_BUDGET,
        }))
    }

    /// `depth += W_f` — the prologue's accounting half.
    pub(super) fn increment(&self, weight: u32, body: &mut Body) {
        body.push(Op::Leaf(Instruction::GlobalGet(self.depth)));
        body.push(Op::Leaf(Instruction::I32Const(weight as i32)));
        body.push(Op::Leaf(Instruction::I32Add));
        body.push(Op::Leaf(Instruction::GlobalSet(self.depth)));
    }

    /// `depth -= W_f` — the epilogue and tail-path decrement.
    pub(super) fn decrement(&self, weight: u32, body: &mut Body) {
        body.push(Op::Leaf(Instruction::GlobalGet(self.depth)));
        body.push(Op::Leaf(Instruction::I32Const(weight as i32)));
        body.push(Op::Leaf(Instruction::I32Sub));
        body.push(Op::Leaf(Instruction::GlobalSet(self.depth)));
    }

    /// The prologue's overflow check: when `depth > STACK_BUDGET`, throw the
    /// stack-exhaustion tag. Emitted after the increment; the throw unwinds
    /// out of the function (no try region of this function is armed yet).
    pub(super) fn overflow_check(&self, body: &mut Body, span: TextRange) {
        body.push(Op::Leaf(Instruction::GlobalGet(self.depth)));
        body.push(Op::Leaf(Instruction::I32Const(self.stack_budget)));
        body.push(Op::Leaf(Instruction::I32GtS));
        let overflow = vec![Op::Leaf(Instruction::Throw(self.tag))];
        body.push(Op::If {
            then_body: overflow,
            else_body: Vec::new(),
            result: None,
            span,
        });
    }

    /// `saved = depth` — the watermark save at a `try_table`'s entry, into the
    /// try region's fresh `i32` local.
    pub(super) fn watermark_save(&self, saved: u32, body: &mut Body) {
        body.push(Op::Leaf(Instruction::GlobalGet(self.depth)));
        body.push(Op::Leaf(Instruction::LocalSet(saved)));
    }

    /// `depth = saved` — the catch path's first action, settling every frame
    /// the EH throw unwound (whose epilogues never ran) at once.
    pub(super) fn watermark_restore(&self, saved: u32, body: &mut Body) {
        body.push(Op::Leaf(Instruction::LocalGet(saved)));
        body.push(Op::Leaf(Instruction::GlobalSet(self.depth)));
    }
}

impl Structurer<'_> {
    /// The budget context, or the error a try region would otherwise hit
    /// silently: P9's capability gate rejects a module with try regions under
    /// a disabled profile, so a missing context cannot reach an armed region
    /// in a checked pipeline.
    pub(super) fn budget(&self, span: TextRange) -> Result<&Budget, Vec<BackendError>> {
        match &self.budget {
            Some(budget) => Ok(budget),
            None => Err(wasm_error(
                span,
                "a try region needs the stack-exhaustion budget context",
            )),
        }
    }

    /// The fresh `i32` watermark local of the try region owned by `try_block`:
    /// one local per try region, allocated after the lowered values, the
    /// list-copy locals, and (mutually exclusive with them) the dispatcher
    /// state local. Try regions are numbered in block order, which is
    /// deterministic.
    pub(super) fn watermark_local(&self, try_block: crate::mir::BlockId) -> u32 {
        let ordinal = self
            .function
            .blocks
            .iter()
            .filter(|block| {
                block.id.0 < try_block.0
                    && matches!(&block.terminator, Some(crate::mir::Terminator::Try { .. }))
            })
            .count() as u32;
        self.watermark_base + ordinal
    }

    /// The epilogue decrement on a `return` path (EH-13). Nothing when the
    /// profile disables the budget.
    pub(super) fn emit_budget_epilogue(&self, body: &mut Body) -> Result<(), Vec<BackendError>> {
        if let Some(budget) = &self.budget {
            budget.decrement(self.function.frame_weight, body);
        }
        Ok(())
    }

    /// The tail-path decrement before a `return_call`/`return_call_ref`
    /// (EH-13): the callee replaces this frame, so this frame's weight must
    /// come off the counter before the transfer. Nothing when the profile
    /// disables the budget.
    pub(super) fn emit_budget_tail_decrement(
        &self,
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        if let Some(budget) = &self.budget {
            budget.decrement(self.function.frame_weight, body);
        }
        Ok(())
    }
}
