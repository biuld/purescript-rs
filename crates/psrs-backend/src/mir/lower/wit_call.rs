//! Canonical adapters are planned independently before instructions are emitted.
use super::*;

impl FunctionLowerer<'_> {
    pub(super) fn lower_wit_call(
        &mut self,
        import: &BoundWasiImport,
        assignment: &cc::Assignment,
        entry: BlockId,
    ) -> Result<BlockId, Vec<BackendError>> {
        let AssignmentKind::DirectCall { arguments, .. } = &assignment.kind else {
            return Err(vec![BackendError::invalid_ir(
                "P9 canonical call planning",
                assignment.span,
                "canonical adapter requires a direct source call",
            )]);
        };
        let start = self
            .find_block_mut(entry, assignment.span)?
            .instructions
            .len();
        let value_start = self.values.len();
        // The isolated builder shares checked layouts and helper ownership,
        // but cannot inspect or mutate the actual instruction stream.
        let mut builder = FunctionLowerer {
            owner: self.owner,
            runtime: None,
            runtime_calls: Vec::new(),
            wit_calls: Vec::new(),
            next_block: self.next_block,
            blocks: vec![BasicBlock {
                id: entry,
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: None,
            }],
            values: self.values.clone(),
            next_value: self.next_value,
            wit_imports: self.wit_imports,
            layout: self.layout,
            conversion_helpers: self.conversion_helpers.as_deref_mut(),
            literals: self.literals.as_deref_mut(),
        };
        let exit = wit::lower(
            &mut builder,
            &import.import,
            &import.signature,
            import.projection.as_ref(),
            assignment.destination,
            arguments,
            assignment.span,
            entry,
        )?;
        let plan = wit::CallPlan::new(
            self.owner,
            assignment.clone(),
            import.clone(),
            start,
            exit,
            builder.blocks.clone(),
            builder.values[value_start..].to_vec(),
        );
        let next_block = builder.next_block;
        let next_value = builder.next_value;
        let blocks = builder.blocks;
        let values = builder.values;
        let entry_plan = &blocks[0];
        for instruction in &entry_plan.instructions {
            self.append_instruction(entry, instruction.clone(), assignment.span)?;
        }
        if let Some(terminator) = &entry_plan.terminator {
            self.set_terminator(entry, terminator.clone(), assignment.span)?;
        }
        self.blocks.extend(blocks.into_iter().skip(1));
        self.values.extend(values.into_iter().skip(value_start));
        self.next_block = next_block;
        self.next_value = next_value;
        self.wit_calls.push(plan);
        Ok(exit)
    }
}
