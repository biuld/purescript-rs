//! Immutable source anchors and actual block ranges for raw ABI adaptation.
use super::*;
use crate::mir::{BlockId, Module, runtime::RawInvocation};
use crate::types::ValueDecl;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::mir) struct RuntimeInvocation {
    plan: RawInvocation,
    block: BlockId,
    start: usize,
    length: usize,
    temporaries: Vec<ValueDecl>,
}

impl RuntimeInvocation {
    pub(in crate::mir) fn new(
        plan: RawInvocation,
        block: BlockId,
        start: usize,
        length: usize,
        temporaries: Vec<ValueDecl>,
    ) -> Self {
        Self {
            plan,
            block,
            start,
            length,
            temporaries,
        }
    }

    pub(in crate::mir) fn destination(&self) -> cc::ValueId {
        self.plan.destination()
    }

    pub(super) fn never_returns(&self) -> bool {
        self.plan.never_returns()
    }

    pub(super) fn verify_call(
        &self,
        source: &cc::Function,
        assignment: &cc::Assignment,
        function: &Function,
        block: BlockId,
        call_index: usize,
    ) -> Result<(), Vec<BackendError>> {
        if block != self.block || !self.plan.matches_source(source.symbol, assignment) {
            return Err(error(
                function,
                "runtime invocation loses its source or control-block anchor",
            ));
        }
        self.plan.verify_source_values(function)?;
        let actual = function
            .blocks
            .iter()
            .find(|actual| actual.id == block)
            .ok_or_else(|| error(function, "runtime invocation has no actual block"))?;
        let end = self
            .start
            .checked_add(self.length)
            .ok_or_else(|| error(function, "runtime invocation range overflows"))?;
        let instructions = actual.instructions.get(self.start..end).ok_or_else(|| {
            error(
                function,
                "runtime invocation has no complete instruction range",
            )
        })?;
        if instructions
            .iter()
            .position(|instruction| {
                matches!(
                    instruction,
                    Instruction::Call { .. } | Instruction::CallVoid { .. }
                )
            })
            .map(|offset| self.start + offset)
            != Some(call_index)
        {
            return Err(error(
                function,
                "runtime invocation moves across source calls",
            ));
        }
        if self.never_returns()
            && (end != actual.instructions.len()
                || !matches!(actual.terminator, Some(crate::mir::Terminator::Trap { .. })))
        {
            return Err(error(
                function,
                "non-returning invocation has a physical continuation",
            ));
        }
        for expected in &self.temporaries {
            if !function.values.iter().any(|value| value == expected) {
                return Err(error(
                    function,
                    "runtime invocation changes a checked temporary type",
                ));
            }
        }
        self.plan.verify(&self.temporaries, instructions)
    }

    pub(super) fn verify_import(&self, module: &Module) -> Result<(), Vec<BackendError>> {
        let expected = self.plan.import();
        if module
            .imports
            .iter()
            .find(|import| import.symbol == expected.symbol)
            != Some(expected)
        {
            return Err(vec![BackendError::invalid_ir(
                "P9 MIR dependency verification",
                module.span,
                "runtime invocation import differs from its immutable checked binding",
            )]);
        }
        Ok(())
    }
}
