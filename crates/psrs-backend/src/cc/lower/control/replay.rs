//! Instantiate lifecycle frames when replaying a persistent source checkpoint.
//! Ordinary callbacks and one-shot owner objects are shared, never cloned.
//!
//! The bodies are built from the checked plans in [`super::plan`]; only the
//! replay-result projection stays here because the executor also uses it.

use super::builder::{Machine, repr};
use super::fields::ControlEmitter;
use crate::cc::{Assignment, AssignmentKind, Function, ValueId};

impl Machine<'_> {
    pub(super) fn clone_sequence(mut self) -> Function {
        let queue = self.fresh_value(repr(self.layout.sequence));
        let todo = self.fresh_value(repr(self.layout.sequence));
        let reversed = self.fresh_value(repr(self.layout.sequence));
        let active = self.fresh_value(repr(self.layout.boundary));
        let parent = self.fresh_value(repr(self.layout.boundary));
        let params = [queue, todo, reversed, active, parent];
        let program = super::plan::build(super::plan::Program::CloneSequence);
        let mut body = Vec::new();
        let result = self.expand(&program, &params, &mut body);
        let representations = self.representations;
        let symbol = self.functions.clone_sequence;
        let function = self.finish(
            symbol,
            "control_clone_sequence",
            params.to_vec(),
            body,
            result,
        );
        super::plan::check_boundary(&function, representations, &program)
            .expect("clone_sequence boundary check must pass");
        function
    }

    pub(super) fn rebuild_sequence(mut self) -> Function {
        let reversed = self.fresh_value(repr(self.layout.sequence));
        let result_queue = self.fresh_value(repr(self.layout.sequence));
        let active = self.fresh_value(repr(self.layout.boundary));
        let parent = self.fresh_value(repr(self.layout.boundary));
        let params = [reversed, result_queue, active, parent];
        let program = super::plan::build(super::plan::Program::RebuildSequence);
        let mut body = Vec::new();
        let result = self.expand(&program, &params, &mut body);
        let representations = self.representations;
        let symbol = self.functions.rebuild_sequence;
        let function = self.finish(
            symbol,
            "control_rebuild_sequence",
            params.to_vec(),
            body,
            result,
        );
        super::plan::check_boundary(&function, representations, &program)
            .expect("rebuild_sequence boundary check must pass");
        function
    }

    pub(super) fn replay_field(
        &mut self,
        value: ValueId,
        field: u32,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let representation = self.layout.replay_result;
        let Some(crate::cc::Representation::Product { fields }) =
            self.representations.representation(representation)
        else {
            unreachable!("predeclared replay result product");
        };
        let shape = *fields
            .get(field as usize)
            .expect("predeclared replay result field");
        self.emit(shape, body, |destination| AssignmentKind::ProductGet {
            destination,
            representation,
            field,
            value,
        })
    }
}
