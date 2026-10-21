//! Explicit CC protection to native exception CFG conversion.
use super::*;
use crate::cc::{Region, RegionExit};

impl FunctionLowerer<'_> {
    pub(super) fn lower_protect(
        &mut self,
        current: BlockId,
        destination: ValueId,
        body: &Region,
        exception: ValueId,
        handler: &Region,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let protected = self.new_block(Vec::new());
        let caught = self.new_block(vec![exception]);
        let join = self.new_block(vec![destination]);
        self.set_terminator(
            current,
            Terminator::Try {
                protected,
                continuation: Some(join),
                clauses: vec![super::super::CatchClause::CatchAllRef { target: caught }],
                span,
            },
            span,
        )?;
        for (entry, region) in [(protected, body), (caught, handler)] {
            let end = self.lower_assignments(&region.assignments, entry)?;
            let terminator = match region.exit {
                RegionExit::Yield(value) => Terminator::Jump {
                    target: join,
                    arguments: vec![value],
                    span,
                },
                RegionExit::Propagate(exception) => Terminator::ThrowRef { exception, span },
            };
            self.terminate_if_open(end, terminator, span)?;
        }
        Ok(join)
    }
}
