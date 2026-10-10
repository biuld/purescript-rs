//! Immutable canonical adapter plans, produced before physical emission.
use crate::BackendError;
use crate::cc;
use crate::mir::{BasicBlock, BlockId, BoundWasiImport, Function};
use crate::types::ValueDecl;
use psrs_hir::SymbolId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::mir) struct CallPlan {
    owner: SymbolId,
    source: cc::Assignment,
    binding: BoundWasiImport,
    start: usize,
    exit: BlockId,
    blocks: Vec<BasicBlock>,
    temporaries: Vec<ValueDecl>,
}

impl CallPlan {
    pub(in crate::mir) fn entry(&self) -> BlockId {
        self.blocks[0].id
    }
    pub(in crate::mir) fn start(&self) -> usize {
        self.start
    }
    pub(in crate::mir) fn exit(&self) -> (BlockId, usize) {
        let block = self
            .blocks
            .iter()
            .find(|block| block.id == self.exit)
            .expect("canonical lowering returns its own exit block");
        (
            self.exit,
            if self.exit == self.entry() {
                self.start
            } else {
                0
            } + block.instructions.len(),
        )
    }
    pub(in crate::mir) fn created_blocks(&self) -> impl Iterator<Item = BlockId> + '_ {
        self.blocks.iter().skip(1).map(|block| block.id)
    }
    pub(in crate::mir) fn matches_source(
        &self,
        assignment: &cc::Assignment,
        arguments: &[cc::ValueId],
    ) -> bool {
        match (&self.source.kind, &assignment.kind) {
            (
                cc::AssignmentKind::DirectCall {
                    function,
                    arguments: planned,
                },
                cc::AssignmentKind::DirectCall {
                    function: actual, ..
                },
            ) => {
                function == actual
                    && planned == arguments
                    && self.source.destination == assignment.destination
                    && self.source.span == assignment.span
            }
            _ => false,
        }
    }

    pub(in crate::mir) fn verify_import(
        &self,
        module: &crate::mir::Module,
    ) -> Result<(), Vec<BackendError>> {
        let import = &self.binding.import;
        let expected = crate::mir::Import {
            runtime: None,
            symbol: import.symbol,
            parameters: import.parameters.clone(),
            result: import.result,
        };
        if module
            .imports
            .iter()
            .find(|actual| actual.symbol == expected.symbol)
            != Some(&expected)
        {
            return Err(vec![BackendError::invalid_ir(
                "P9 canonical call verification",
                self.source.span,
                "canonical import differs from its immutable checked binding",
            )]);
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::mir) fn new(
        owner: SymbolId,
        source: cc::Assignment,
        binding: BoundWasiImport,
        start: usize,
        exit: BlockId,
        blocks: Vec<BasicBlock>,
        temporaries: Vec<ValueDecl>,
    ) -> Self {
        Self {
            owner,
            source,
            binding,
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
