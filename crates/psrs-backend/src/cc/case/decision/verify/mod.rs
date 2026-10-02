//! Structural checks for path-sensitive decision DAG actions.

use super::{Action, ColumnKey, Decision, DecisionDag, NodeId, Test};
use std::collections::{HashMap, HashSet};

pub(super) fn array_projection_guards(dag: &DecisionDag) -> Result<(), &'static str> {
    visit(dag, dag.root, &HashMap::new(), &mut HashSet::new())
}

fn visit(
    dag: &DecisionDag,
    node: NodeId,
    inherited: &HashMap<ColumnKey, usize>,
    active: &mut HashSet<NodeId>,
) -> Result<(), &'static str> {
    if !active.insert(node) {
        return Err("decision DAG contains a cycle");
    }
    let decision = dag
        .nodes
        .get(node.0)
        .ok_or("decision target is out of bounds")?;
    let result = match decision {
        Decision::Leaf { actions, .. } => {
            let mut facts = inherited.clone();
            verify_actions(actions, &mut facts)
        }
        Decision::Fail { .. } => Ok(()),
        Decision::Switch {
            column,
            edges,
            default_actions,
            default,
            ..
        } => {
            for edge in edges {
                let mut facts = inherited.clone();
                if let Test::ArrayLength { length } = &edge.test {
                    facts.insert(column.clone(), *length);
                }
                verify_actions(&edge.actions, &mut facts)?;
                visit(dag, edge.target, &facts, active)?;
            }
            let mut facts = inherited.clone();
            verify_actions(default_actions, &mut facts)?;
            if let Some(default) = default {
                visit(dag, *default, &facts, active)?;
            }
            Ok(())
        }
    };
    active.remove(&node);
    result
}

fn verify_actions(
    actions: &[Action],
    facts: &mut HashMap<ColumnKey, usize>,
) -> Result<(), &'static str> {
    // Every action reads `values` and writes to a separate projected map.
    // Check all sources against one snapshot, then apply output facts in order.
    let snapshot = facts.clone();
    let mut outputs = HashMap::<ColumnKey, Option<usize>>::new();
    for action in actions {
        match action {
            Action::Map { source, target, .. } => {
                outputs.insert(target.clone(), snapshot.get(source).copied());
            }
            Action::ArrayGet {
                source,
                target,
                index,
                ..
            } => {
                if !snapshot
                    .get(source)
                    .is_some_and(|length| usize::try_from(*index).is_ok_and(|i| i < *length))
                {
                    return Err("array projection is not dominated by a sufficient length test");
                }
                outputs.insert(target.clone(), None);
            }
            Action::Project { target, .. } => {
                outputs.insert(target.clone(), None);
            }
            Action::Bind { .. } | Action::TestTag { .. } => {}
        }
    }
    for (target, length) in outputs {
        facts.remove(&target);
        if let Some(length) = length {
            facts.insert(target, length);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
