//! Closed execution roots belong to explicit checked runner instructions.
use super::*;
use crate::cc::{AssignmentKind, RepresentationTable, SignatureId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedExecution<'a> {
    pub assignment: &'a Assignment,
    pub graph: Graph,
}

pub fn execution_projection(
    signature: SignatureId,
    table: &RepresentationTable,
) -> Result<StateCallProjection, &'static str> {
    let signature = table
        .signature(signature)
        .ok_or("state execution has no checked callable signature")?;
    let plan = StateCallProjection::checked(signature, table)?
        .ok_or("state execution requires a State-to-Step callable")?;
    if plan.state_parameter != 0 {
        return Err("state execution requires an action without ordinary parameters");
    }
    Ok(plan)
}

pub(super) fn contains(assignments: &[Assignment]) -> bool {
    assignments.iter().any(|assignment| match &assignment.kind {
        AssignmentKind::StateExecution { .. } => true,
        AssignmentKind::If {
            then_assignments,
            else_assignments,
            ..
        } => contains(then_assignments) || contains(else_assignments),
        AssignmentKind::TagSwitch {
            cases,
            default_assignments,
            ..
        } => contains(default_assignments) || cases.iter().any(|case| contains(&case.assignments)),
        _ => false,
    })
}

impl<'a> Builder<'a, '_> {
    pub(super) fn execution(
        &mut self,
        assignment: &'a Assignment,
        signature: SignatureId,
    ) -> Result<Value, Vec<BackendError>> {
        execution_projection(signature, &self.module.representations)
            .map_err(|message| error(self.function, message))?;
        // Each instruction owns a separate graph namespace. Its explicit
        // boundary supplies the root; no State value or integer literal exists.
        let input = DependencyId(0);
        let output = DependencyId(1);
        let graph = Graph {
            entry: 0,
            regions: vec![RegionId(0)],
            blocks: vec![Block {
                id: 0,
                parameters: vec![Dependency {
                    id: input,
                    region: RegionId(0),
                }],
                transitions: vec![Transition {
                    operation: 0,
                    input,
                    output: Dependency {
                        id: output,
                        region: RegionId(0),
                    },
                    span: assignment.span,
                }],
                terminator: Terminator::Return(vec![output]),
                span: assignment.span,
            }],
        };
        graph
            .verify()
            .map_err(|failure| error(self.function, failure.message))?;
        self.executions.push(ClosedExecution { assignment, graph });
        self.passthrough_safe = false;
        Ok(Value::Opaque)
    }
}
