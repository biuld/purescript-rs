use super::{Block, Builder, Dependency, Path, RegionId, Terminator, Value};
use crate::state::dependency::Edge;
use crate::{CaseBranch, Expr, Pattern, VerifyError};
use psrs_span::TextRange;
use std::collections::HashMap;

impl<'a> Builder<'a> {
    fn fork(&mut self, path: &Path, span: TextRange) -> Path {
        let input = self.fresh();
        let block = self.blocks.len();
        self.blocks.push(Block {
            id: block as u32,
            parameters: vec![Dependency {
                id: input,
                region: RegionId(0),
            }],
            transitions: Vec::new(),
            terminator: Terminator::Trap,
            span,
        });
        let mut locals = path.locals.clone();
        for value in locals.values_mut() {
            value.remap(path.current, input);
        }
        let mut aliases = path.aliases.clone();
        aliases.insert(path.current, input);
        Path {
            block,
            current: input,
            revision: path.revision,
            locals,
            aliases,
        }
    }

    pub(super) fn alternatives(
        &mut self,
        values: &[&'a Expr],
        path: &mut Path,
        span: TextRange,
    ) -> Result<Value, VerifyError> {
        let arms = values
            .iter()
            .map(|value| (*value, None))
            .collect::<Vec<_>>();
        self.arms(&arms, &Value::Opaque, path, span)
    }

    pub(super) fn case_alternatives(
        &mut self,
        value: Value,
        branches: &'a [CaseBranch],
        path: &mut Path,
        span: TextRange,
    ) -> Result<Value, VerifyError> {
        let arms = branches
            .iter()
            .map(|branch| (&branch.value, Some(&branch.pattern)))
            .collect::<Vec<_>>();
        self.arms(&arms, &value, path, span)
    }

    fn arms(
        &mut self,
        arms: &[(&'a Expr, Option<&'a Pattern>)],
        scrutinee: &Value,
        path: &mut Path,
        span: TextRange,
    ) -> Result<Value, VerifyError> {
        psrs_span::with_sufficient_stack(|| self.arms_inner(arms, scrutinee, path, span))
    }

    fn arms_inner(
        &mut self,
        arms: &[(&'a Expr, Option<&'a Pattern>)],
        scrutinee: &Value,
        path: &mut Path,
        span: TextRange,
    ) -> Result<Value, VerifyError> {
        if arms.is_empty() {
            return Ok(Value::Terminated);
        }
        if arms.len() == 1 {
            if let Some(pattern) = arms[0].1 {
                self.bind(pattern, scrutinee.clone(), path)?;
            }
            return self.eval(arms[0].0, path);
        }
        let incoming = path.current;
        let revision = path.revision;
        let mut left = self.fork(path, span);
        let mut right = self.fork(path, span);
        self.blocks[path.block].terminator = Terminator::Branch {
            then_edge: Edge {
                target: left.block as u32,
                arguments: vec![incoming],
            },
            else_edge: Edge {
                target: right.block as u32,
                arguments: vec![incoming],
            },
        };
        let mut left_scrutinee = scrutinee.clone();
        left_scrutinee.remap(incoming, left.current);
        let left_value = self.arms(&arms[..1], &left_scrutinee, &mut left, span)?;
        let mut right_scrutinee = scrutinee.clone();
        right_scrutinee.remap(incoming, right.current);
        let right_value = self.arms(&arms[1..], &right_scrutinee, &mut right, span)?;
        let live = [(left_value, left), (right_value, right)]
            .into_iter()
            .filter(|(value, _)| !matches!(value, Value::Terminated))
            .collect::<Vec<_>>();
        if live.is_empty() {
            return Ok(Value::Terminated);
        }
        let join = self.fork(path, span);
        let mut merged = None;
        for (value, branch) in &live {
            let mut value = value.clone();
            value.resolve(branch);
            self.blocks[branch.block].terminator = Terminator::Jump(Edge {
                target: join.block as u32,
                arguments: vec![branch.current],
            });
            value.remap(branch.current, join.current);
            merged = Some(match merged {
                None => value,
                Some(previous) => self.merge(previous, value, span)?,
            });
        }
        // With no observable invocation on any surviving path, incoming state
        // aliases also denote the selected join dependency. Otherwise they
        // remain predecessors and a subsequent use must reject.
        if live.iter().all(|(_, branch)| branch.revision == revision) {
            path.aliases.insert(incoming, join.current);
            for value in path.locals.values_mut() {
                value.remap(incoming, join.current);
            }
        }
        path.current = join.current;
        path.block = join.block;
        path.revision = live
            .iter()
            .map(|(_, branch)| branch.revision)
            .max()
            .unwrap();
        Ok(merged.unwrap())
    }

    fn merge(&self, left: Value, right: Value, span: TextRange) -> Result<Value, VerifyError> {
        Ok(match (left, right) {
            (Value::State(left), Value::State(right)) if left == right => Value::State(left),
            (Value::Record(left), Value::Record(mut right)) if left.len() == right.len() => {
                let mut fields = HashMap::new();
                for (label, value) in left {
                    let other = right.remove(&label).ok_or_else(|| {
                        self.error(span, "state branch result changes its checked fields")
                    })?;
                    fields.insert(label, self.merge(value, other, span)?);
                }
                Value::Record(fields)
            }
            (Value::Constructor(left), Value::Constructor(right)) if left.len() == right.len() => {
                Value::Constructor(
                    left.into_iter()
                        .zip(right)
                        .map(|(left, right)| self.merge(left, right, span))
                        .collect::<Result<_, _>>()?,
                )
            }
            (Value::State(_), _) | (_, Value::State(_)) => {
                return Err(self.error(
                    span,
                    "state branch result has incompatible producer provenance",
                ));
            }
            _ => Value::Opaque,
        })
    }
}
