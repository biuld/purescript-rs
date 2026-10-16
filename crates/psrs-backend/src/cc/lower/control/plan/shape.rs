//! The dispatch/outcome shape of a plan, shared by the boundary checker.
//!
//! A [`Shape`] is the tree of machine-carrier switches a body performs and the
//! terminal outcome each path reaches. It is derived from the plan by
//! [`shape`] and from emitted CC by the P8 boundary checker; comparing the two
//! is how the boundary validates that expansion emitted the checked program.

use super::ir::{Exec, Family};

/// The terminal outcome class of one plan path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Terminal {
    /// Continue the flat driver with a complete state.
    Advance,
    /// Tail-call another machine helper.
    Tail,
    /// Publish or return a value from the body.
    Value,
    /// Trap on an unsupported alternative.
    Reject,
}

/// A dispatch/outcome tree. Switch arms are in schema order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    Switch(Family, Vec<(u32, Shape)>),
    Leaf(Terminal),
}

/// Derive the shape of a plan.
pub(crate) fn shape(exec: &Exec) -> Shape {
    match exec {
        Exec::Steps { next, .. } => shape(next),
        Exec::Switch { case, arms, .. } => Shape::Switch(
            case.family(),
            arms.iter().map(|(tag, arm)| (*tag, shape(arm))).collect(),
        ),
        Exec::Advance { .. } => Shape::Leaf(Terminal::Advance),
        Exec::Tail { .. } => Shape::Leaf(Terminal::Tail),
        Exec::Publish { .. } | Exec::Return { .. } => Shape::Leaf(Terminal::Value),
        Exec::Reject { .. } => Shape::Leaf(Terminal::Reject),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_is_sensitive_to_a_dropped_dispatch_arm() {
        let mut plan = super::super::executor();
        let before = shape(&plan);
        if let Exec::Switch { arms, .. } = &mut plan {
            arms.retain(|(tag, _)| *tag != 0);
        }
        assert_ne!(shape(&plan), before);
    }
}
