//! Source dispatch and target lifetime are separate heap contexts.
//!
//! The executor's use of these relations now lives in [`super::plan`]; this
//! module keeps only the named context-extent projection they share.

use super::builder::Machine;
use super::executor::arm;
use crate::cc::{Assignment, ValueId};
use crate::control::schema::{ContextCase, context as x};

impl Machine<'_> {
    /// The current dispatch checkpoint of a context: root field 0 or scoped
    /// field 5. Named so replay and closure capture do not recover the offset.
    pub(super) fn context_extent(
        &mut self,
        context: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let mut cases = Vec::new();
        for case in ContextCase::ALL {
            let mut branch = Vec::new();
            let index = match case {
                ContextCase::Root => x::ROOT_EXTENT,
                ContextCase::Scoped => x::SCOPED_EXTENT,
            };
            let extent = self.field(context, self.layout.context, case.tag(), index, &mut branch);
            cases.push(arm(case.tag(), branch, extent));
        }
        self.switch(context, self.layout.context, cases, body)
    }
}
