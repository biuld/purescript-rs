//! The P8 expansion boundary checker.
//!
//! It rebuilds the dispatch/outcome shape of an *emitted* function and compares
//! it with the checked plan's shape. The plan and the emitter are independent:
//! the plan is hand-built from the semantics, the emitter is a generic expander,
//! so an expander defect (dropped arm, wrong tag, wrong terminal) is a mismatch
//! rather than a silently accepted body. The generic CC verifiers still check
//! operand semantics; this checks that expansion emitted the checked program.

use super::Exec;
use super::ir::Family;
use super::shape::{Shape, Terminal, shape};
use crate::cc::branch_facts::{Definitions, tag_source};
use crate::cc::value_identity::transparent;
use crate::cc::{AssignmentKind as A, Function, ReprId, RepresentationTable, ValueId};
use psrs_hir::SymbolId;

/// Compare the emitted function's machine dispatch against `plan`.
pub(crate) fn check_boundary(
    function: &Function,
    table: &RepresentationTable,
    plan: &Exec,
) -> Result<(), &'static str> {
    let schema = table
        .control_schema()
        .ok_or("boundary check has no checked machine schema")?;
    let expected = shape(plan);
    let view = View {
        definitions: crate::cc::value_identity::definitions(&function.assignments),
        computation: schema.computation,
        sequence: schema.sequence,
        context: schema.context,
        extent: schema.extent,
        owner: schema.owner,
        executor: table
            .control_helper(crate::cc::ControlHelperRole::Executor)
            .map(|h| h.function()),
        helpers: table
            .control_helpers
            .iter()
            .map(|helper| helper.function())
            .collect(),
    };
    if view.shape(function.result) != expected {
        return Err("emitted machine dispatch or outcome disagrees with the checked plan");
    }
    Ok(())
}

struct View<'a> {
    definitions: Definitions<'a>,
    computation: ReprId,
    sequence: ReprId,
    context: ReprId,
    extent: ReprId,
    owner: ReprId,
    executor: Option<SymbolId>,
    helpers: Vec<SymbolId>,
}

impl View<'_> {
    /// Resolve representations without collapsing a dispatch: follow casts and
    /// transparent conversions, but keep a `TagSwitch` as its own node.
    fn strip(&self, mut value: ValueId) -> ValueId {
        let mut active = std::collections::HashSet::new();
        loop {
            if !active.insert(value) {
                break;
            }
            match self.definitions.get(&value).copied() {
                Some(A::RepresentationCast { value: source, .. }) => value = *source,
                Some(A::AggregateConvert {
                    value: source,
                    conversion,
                    ..
                }) if transparent(&conversion.plan) => value = *source,
                _ => break,
            }
        }
        value
    }

    fn family(&self, repr: ReprId) -> Option<Family> {
        if repr == self.computation {
            Some(Family::Computation)
        } else if repr == self.sequence {
            Some(Family::Sequence)
        } else if repr == self.context {
            Some(Family::Context)
        } else if repr == self.extent {
            Some(Family::Extent)
        } else if repr == self.owner {
            Some(Family::Owner)
        } else {
            None
        }
    }

    fn shape(&self, value: ValueId) -> Shape {
        let value = self.strip(value);
        match self.definitions.get(&value).copied() {
            Some(A::TagSwitch {
                value: selector,
                cases,
                ..
            }) => {
                if let Some((repr, _)) = tag_source(*selector, &self.definitions)
                    && let Some(family) = self.family(repr)
                {
                    return Shape::Switch(
                        family,
                        cases
                            .iter()
                            .map(|case| (case.tag as u32, self.shape(case.value)))
                            .collect(),
                    );
                }
                Shape::Leaf(Terminal::Value)
            }
            Some(A::DirectCall { function, .. }) => {
                if Some(*function) == self.executor {
                    Shape::Leaf(Terminal::Advance)
                } else if self.helpers.contains(function) {
                    Shape::Leaf(Terminal::Tail)
                } else {
                    Shape::Leaf(Terminal::Value)
                }
            }
            Some(A::Unreachable) => Shape::Leaf(Terminal::Reject),
            _ => Shape::Leaf(Terminal::Value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::executor;
    use super::*;

    #[test]
    fn shape_is_sensitive_to_a_dropped_dispatch_arm() {
        let mut plan = executor();
        let before = shape(&plan);
        if let Exec::Switch { arms, .. } = &mut plan {
            arms.retain(|(tag, _)| *tag != 0);
        }
        assert_ne!(shape(&plan), before);
    }
}
