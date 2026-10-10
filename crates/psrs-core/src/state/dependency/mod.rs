//! Representation-independent state flow validation.
//!
//! Block parameters represent joins and loop-carried dependencies. Each block
//! is checked using those parameters as its incoming state; edges transfer the
//! current state of each region to the corresponding successor parameter.
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DependencyId(pub u32);

/// An interned checked region identity, never a physical value or type name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RegionId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub id: DependencyId,
    pub region: RegionId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    /// The defining operation in the owning representation, in execution order.
    pub operation: u32,
    pub input: DependencyId,
    pub output: Dependency,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub target: u32,
    pub arguments: Vec<DependencyId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Return(Vec<DependencyId>),
    Jump(Edge),
    Branch {
        then_edge: Edge,
        else_edge: Edge,
    },
    /// Total multiway choice. Concrete predicates and case labels belong to
    /// the owning representation's instruction-correspondence check.
    Switch {
        case_edges: Vec<Edge>,
        default_edge: Edge,
    },
    /// A trapping path has no normal state successor.
    Trap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub id: u32,
    pub parameters: Vec<Dependency>,
    pub transitions: Vec<Transition>,
    pub terminator: Terminator,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    pub entry: u32,
    /// Checked function input regions, in signature order. A runner's verified
    /// invocation boundary supplies roots; the graph never invents a root.
    pub regions: Vec<RegionId>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyError {
    pub span: TextRange,
    pub message: &'static str,
}

impl Graph {
    /// Validation is read-only. Consumers must additionally check that every
    /// transition's operation identity maps to its actual checked instruction.
    pub fn verify(&self) -> Result<(), DependencyError> {
        let fail = |span, message| DependencyError { span, message };
        let mut blocks = HashMap::new();
        let mut definitions = HashSet::new();
        for block in &self.blocks {
            if blocks.insert(block.id, block).is_some() {
                return Err(fail(block.span, "duplicate state dependency block"));
            }
            for definition in block.parameters.iter().chain(
                block
                    .transitions
                    .iter()
                    .map(|transition| &transition.output),
            ) {
                if !definitions.insert(definition.id) {
                    return Err(fail(
                        block.span,
                        "state dependency has multiple definitions",
                    ));
                }
            }
        }
        let Some(entry) = blocks.get(&self.entry) else {
            return Err(fail(TextRange::new(0, 0), "state graph has no entry block"));
        };
        if entry
            .parameters
            .iter()
            .map(|value| value.region)
            .collect::<Vec<_>>()
            != self.regions
        {
            return Err(fail(
                entry.span,
                "state graph entry does not match its checked regions",
            ));
        }
        for block in &self.blocks {
            let mut current = HashMap::new();
            for parameter in &block.parameters {
                if current.insert(parameter.region, parameter.id).is_some() {
                    return Err(fail(block.span, "duplicate incoming state region"));
                }
            }
            if current.len() != self.regions.len()
                || self
                    .regions
                    .iter()
                    .any(|region| !current.contains_key(region))
            {
                return Err(fail(
                    block.span,
                    "state block changes the checked region set",
                ));
            }
            let mut previous_operation = None;
            for transition in &block.transitions {
                if previous_operation.is_some_and(|previous| previous >= transition.operation) {
                    return Err(fail(
                        transition.span,
                        "state transitions are not in operation order",
                    ));
                }
                previous_operation = Some(transition.operation);
                if current.get(&transition.output.region) != Some(&transition.input) {
                    return Err(fail(
                        transition.span,
                        "state operation uses a stale or foreign dependency",
                    ));
                }
                current.insert(transition.output.region, transition.output.id);
            }
            let check_edge = |edge: &Edge| -> Result<(), DependencyError> {
                let target = blocks
                    .get(&edge.target)
                    .ok_or_else(|| fail(block.span, "state edge has no target block"))?;
                if edge.arguments.len() != target.parameters.len() {
                    return Err(fail(
                        block.span,
                        "state edge has an invalid parameter count",
                    ));
                }
                for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                    if current.get(&parameter.region) != Some(argument) {
                        return Err(fail(
                            block.span,
                            "state edge does not transfer the current dependency",
                        ));
                    }
                }
                Ok(())
            };
            match &block.terminator {
                Terminator::Return(values) => {
                    if values.len() != self.regions.len()
                        || self
                            .regions
                            .iter()
                            .zip(values)
                            .any(|(region, value)| current.get(region) != Some(value))
                    {
                        return Err(fail(
                            block.span,
                            "state return discards an executed operation",
                        ));
                    }
                }
                Terminator::Jump(edge) => check_edge(edge)?,
                Terminator::Branch {
                    then_edge,
                    else_edge,
                } => {
                    check_edge(then_edge)?;
                    check_edge(else_edge)?;
                }
                Terminator::Switch {
                    case_edges,
                    default_edge,
                } => {
                    for edge in case_edges.iter().chain(std::iter::once(default_edge)) {
                        check_edge(edge)?;
                    }
                }
                Terminator::Trap => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
