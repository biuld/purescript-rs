use super::*;
use crate::mir::{runtime::RawInvocation, state::RuntimeInvocation};

impl FunctionLowerer<'_> {
    pub(super) fn lower_storage_call(
        &mut self,
        binding: &crate::RuntimeBinding,
        assignment: &cc::Assignment,
        block: BlockId,
    ) -> Result<bool, Vec<BackendError>> {
        let context = self
            .runtime
            .expect("runtime binding was selected from this context");
        let plan = RawInvocation::from_source(
            binding,
            &context.logical,
            self.owner,
            assignment.destination,
            self.layout,
            self.next_value,
        )?;
        let never_returns = plan.never_returns();
        let (values, instructions) = plan.emit()?;
        let next = self
            .next_value
            .checked_add(values.len() as u32)
            .ok_or_else(|| {
                vec![BackendError::invalid_ir(
                    "P9 runtime projection",
                    assignment.span,
                    "runtime invocation has exhausted temporary identities",
                )]
            })?;
        let start = self
            .blocks
            .iter()
            .find(|actual| actual.id == block)
            .ok_or_else(|| {
                vec![BackendError::invalid_ir(
                    "P9 runtime projection",
                    assignment.span,
                    "runtime invocation has no current control block",
                )]
            })?
            .instructions
            .len();
        self.runtime_calls.push(RuntimeInvocation::new(
            plan,
            block,
            start,
            instructions.len(),
            values.clone(),
        ));
        self.values.extend(values);
        self.next_value = next;
        for instruction in instructions {
            self.append_instruction(block, instruction, assignment.span)?;
        }
        Ok(never_returns)
    }
}
