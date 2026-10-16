//! Immutable canonical adapter plans, produced before physical emission.
use crate::BackendError;
use crate::cc;
use crate::mir::{BasicBlock, BlockId, Function};
use crate::types::ValueDecl;
use psrs_hir::SymbolId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::mir) struct CallPlan {
    owner: SymbolId,
    source: cc::Assignment,
    start: usize,
    exit: BlockId,
    blocks: Vec<BasicBlock>,
    temporaries: Vec<ValueDecl>,
}

impl CallPlan {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::mir) fn new(
        owner: SymbolId,
        source: cc::Assignment,
        start: usize,
        exit: BlockId,
        blocks: Vec<BasicBlock>,
        temporaries: Vec<ValueDecl>,
    ) -> Self {
        Self {
            owner,
            source,
            start,
            exit,
            blocks,
            temporaries,
        }
    }

    pub(in crate::mir) fn verify(&self, function: &Function) -> Result<(), Vec<BackendError>> {
        let invalid = || {
            vec![BackendError::invalid_ir(
                "P9 canonical call verification",
                self.source.span,
                "canonical adapter differs from its immutable checked plan",
            )]
        };
        if function.symbol != self.owner {
            return Err(invalid());
        }
        for temporary in &self.temporaries {
            if !function.values.iter().any(|actual| actual == temporary) {
                return Err(invalid());
            }
        }
        for (index, expected) in self.blocks.iter().enumerate() {
            let actual = function
                .blocks
                .iter()
                .find(|block| block.id == expected.id)
                .ok_or_else(invalid)?;
            let start = if index == 0 { self.start } else { 0 };
            let end = start
                .checked_add(expected.instructions.len())
                .ok_or_else(invalid)?;
            if actual.instructions.get(start..end) != Some(expected.instructions.as_slice()) {
                return Err(invalid());
            }
            // The exit port belongs to the enclosing source continuation;
            // every other adapter edge and block is fixed by the plan.
            if expected.id != self.exit
                && (actual.instructions.len() != end || actual.terminator != expected.terminator)
            {
                return Err(invalid());
            }
            if index != 0 && actual.parameters != expected.parameters {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
