use super::*;
use psrs_core::state::dependency::Edge;

impl<'a> Builder<'a, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn branches(
        &mut self,
        assignment: &Assignment,
        then_assignments: &'a [Assignment],
        then_value: ValueId,
        else_assignments: &'a [Assignment],
        else_value: ValueId,
        path: &mut Path,
    ) -> Result<Value, Vec<BackendError>> {
        self.alternatives(
            assignment,
            &[
                (then_assignments, then_value),
                (else_assignments, else_value),
            ],
            false,
            path,
        )
    }

    pub(super) fn alternatives(
        &mut self,
        assignment: &Assignment,
        arms: &[(&'a [Assignment], ValueId)],
        multiway: bool,
        path: &mut Path,
    ) -> Result<Value, Vec<BackendError>> {
        let before = path.clone();
        let before_current = before.current;
        let mut paths = Vec::new();
        let mut values = Vec::new();
        let mut edges = Vec::new();
        for &(assignments, result) in arms {
            let parameter = self.fresh();
            let block = self.blocks.len();
            self.blocks.push(Block {
                id: block as u32,
                parameters: if self.state_root {
                    vec![Dependency {
                        id: parameter,
                        region: RegionId(0),
                    }]
                } else {
                    Vec::new()
                },
                transitions: Vec::new(),
                terminator: Terminator::Trap,
                span: assignment.span,
            });
            edges.push(Edge {
                target: block as u32,
                arguments: if self.state_root {
                    vec![before.current]
                } else {
                    Vec::new()
                },
            });
            let mut branch = before.clone();
            branch.block = block;
            branch.current = parameter;
            for value in branch.values.values_mut() {
                value.remap(before.current, parameter);
            }
            self.assignments(assignments, &mut branch)?;
            if !branch.terminated {
                values.push(self.value(&branch, result));
                paths.push(branch);
            }
        }
        self.blocks[before.block].terminator = if multiway {
            Terminator::Switch {
                default_edge: edges.pop().expect("a switch always has a default arm"),
                case_edges: edges,
            }
        } else {
            Terminator::Branch {
                then_edge: edges.remove(0),
                else_edge: edges.remove(0),
            }
        };
        if paths.is_empty() {
            path.terminated = true;
            return Ok(Value::Opaque);
        }
        let parameter = self.fresh();
        let join = self.blocks.len();
        self.blocks.push(Block {
            id: join as u32,
            parameters: if self.state_root {
                vec![Dependency {
                    id: parameter,
                    region: RegionId(0),
                }]
            } else {
                Vec::new()
            },
            transitions: Vec::new(),
            terminator: Terminator::Trap,
            span: assignment.span,
        });
        let pure = paths
            .iter()
            .all(|branch| branch.revision == before.revision);
        for (branch, value) in paths.iter().zip(&mut values) {
            self.blocks[branch.block].terminator = Terminator::Jump(Edge {
                target: join as u32,
                arguments: if self.state_root {
                    vec![branch.current]
                } else {
                    Vec::new()
                },
            });
            value.remap(branch.current, parameter);
        }
        *path = before;
        path.block = join;
        path.current = parameter;
        path.revision = paths.iter().map(|branch| branch.revision).max().unwrap();
        if pure {
            // Only the dependency current before this choice remains an alias
            // after a pure join. Earlier observable predecessors stay stale.
            for value in path.values.values_mut() {
                value.remap(before_current, parameter);
            }
        }
        let mut value = values.remove(0);
        for other in values {
            value = merge(value, other);
        }
        Ok(value)
    }
}

fn merge(left: Value, right: Value) -> Value {
    match (left, right) {
        (Value::Product(left), Value::Product(right)) if left.len() == right.len() => {
            Value::Product(
                left.into_iter()
                    .zip(right)
                    .map(|(left, right)| merge(left, right))
                    .collect(),
            )
        }
        (left, right) if left == right => left,
        _ => Value::Opaque,
    }
}
