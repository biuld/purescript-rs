use super::super::super::layout::{depends_on_type_variable, scalar_type, user_type_id};
use super::super::super::lower::FunctionLowerer;
use super::super::erased::VariantField;
use super::{Action, ColumnKey, Decision, DecisionDag, DecisionEdge, NodeId, Test};
use crate::BackendError;
use crate::cc::{Assignment, AssignmentKind, BinaryOp, TagCase, ValueId, ValueShape};
use psrs_core::CaseBranch;
use psrs_span::TextRange;
use std::collections::HashMap;

type ActionResult = (Vec<Assignment>, HashMap<ColumnKey, ValueId>);

struct SwitchContext<'a> {
    dag: &'a DecisionDag,
    edges: &'a [DecisionEdge],
    default_actions: &'a [Action],
    default: Option<NodeId>,
    branches: &'a [CaseBranch],
    values: &'a HashMap<ColumnKey, ValueId>,
    scrutinee: ValueId,
    result_type: ValueShape,
    span: TextRange,
}

#[cfg(test)]
#[path = "realize_tests.rs"]
mod tests;

impl FunctionLowerer<'_> {
    pub(in crate::cc::case) fn lower_decision(
        &mut self,
        dag: &DecisionDag,
        branches: &[CaseBranch],
        scrutinee: ValueId,
        result_type: ValueShape,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let mut values = HashMap::new();
        values.insert(ColumnKey::root(), scrutinee);
        let (built, value) =
            self.lower_decision_node(dag, dag.root, branches, &values, result_type, span)?;
        assignments.extend(built);
        Ok(value)
    }

    fn lower_decision_node(
        &mut self,
        dag: &DecisionDag,
        node: NodeId,
        branches: &[CaseBranch],
        values: &HashMap<ColumnKey, ValueId>,
        result_type: ValueShape,
        _span: TextRange,
    ) -> Result<(Vec<Assignment>, ValueId), Vec<BackendError>> {
        match &dag.nodes[node.0] {
            Decision::Leaf {
                branch,
                actions,
                span,
            } => {
                let mut previous = Vec::new();
                for action in actions {
                    let Action::Bind { id, source } = action else {
                        continue;
                    };
                    let value = lookup(values, source, *span)?;
                    previous.push((*id, self.locals.insert(*id, value)));
                }
                let mut assignments = Vec::new();
                let value = self.lower_value(&branches[*branch].value, &mut assignments);
                for (id, old) in previous.into_iter().rev() {
                    if let Some(value) = old {
                        self.locals.insert(id, value);
                    } else {
                        self.locals.remove(&id);
                    }
                }
                value.map(|value| (assignments, value))
            }
            Decision::Fail { span } => {
                let destination = self.fresh(result_type);
                Ok((
                    vec![Assignment {
                        destination,
                        kind: AssignmentKind::Unreachable,
                        span: *span,
                    }],
                    destination,
                ))
            }
            Decision::Switch {
                column,
                ty,
                edges,
                default_actions,
                default,
                span: switch_span,
            } => {
                if edges.len() == 1 && matches!(edges[0].test, Test::Irrefutable) {
                    let (mut projected, child_values) = self.apply_actions(&edges[0], values)?;
                    let (mut child, value) = self.lower_decision_node(
                        dag,
                        edges[0].target,
                        branches,
                        &child_values,
                        result_type,
                        *switch_span,
                    )?;
                    projected.append(&mut child);
                    return Ok((projected, value));
                }
                let scrutinee = lookup(values, column, *switch_span)?;
                let switch = SwitchContext {
                    dag,
                    edges,
                    default_actions,
                    default: *default,
                    branches,
                    values,
                    scrutinee,
                    result_type,
                    span: *switch_span,
                };
                if self.is_nullary_sum(*ty) {
                    self.lower_nullary_switch(switch)
                } else {
                    self.lower_variant_switch(switch)
                }
            }
        }
    }

    fn lower_nullary_switch(
        &mut self,
        switch: SwitchContext<'_>,
    ) -> Result<(Vec<Assignment>, ValueId), Vec<BackendError>> {
        let SwitchContext {
            dag,
            edges,
            default_actions,
            default,
            branches,
            values,
            scrutinee,
            result_type,
            span,
        } = switch;
        let mut cases = Vec::with_capacity(edges.len());
        for edge in edges {
            let Test::Constructor { tag, .. } = edge.test else {
                return Err(case_error(
                    edge.span,
                    "nullary switch contains a non-constructor edge",
                ));
            };
            let (mut arm_assignments, arm_values) = self.apply_actions(edge, values)?;
            let (mut body_assignments, value) = self.lower_decision_node(
                dag,
                edge.target,
                branches,
                &arm_values,
                result_type,
                edge.span,
            )?;
            arm_assignments.append(&mut body_assignments);
            cases.push(TagCase {
                tag: i32::try_from(tag)
                    .map_err(|_| case_error(edge.span, "constructor tag exceeds i32"))?,
                assignments: arm_assignments,
                value,
            });
        }
        let (default_assignments, default_value) = if let Some(default) = default {
            let (mapping, default_values) = self.apply_action_list(default_actions, values)?;
            let (mut body, value) = self.lower_decision_node(
                dag,
                default,
                branches,
                &default_values,
                result_type,
                span,
            )?;
            let mut mapping = mapping;
            mapping.append(&mut body);
            (mapping, value)
        } else {
            self.unreachable_value(result_type, span)
        };
        let destination = self.fresh(result_type);
        Ok((
            vec![Assignment {
                destination,
                kind: AssignmentKind::TagSwitch {
                    value: scrutinee,
                    cases,
                    default_assignments,
                    default_value,
                },
                span,
            }],
            destination,
        ))
    }

    fn lower_variant_switch(
        &mut self,
        switch: SwitchContext<'_>,
    ) -> Result<(Vec<Assignment>, ValueId), Vec<BackendError>> {
        let SwitchContext {
            dag,
            edges,
            default_actions,
            default,
            branches,
            values,
            scrutinee,
            result_type,
            span,
        } = switch;
        let Some(first_symbol) = edges.iter().find_map(|edge| match edge.test {
            Test::Constructor { symbol, .. } => Some(symbol),
            Test::Irrefutable => None,
        }) else {
            return Err(case_error(
                span,
                "constructor switch has no constructor edges",
            ));
        };
        let representation = self
            .constructor_types
            .get(&first_symbol)
            .copied()
            .ok_or_else(|| case_error(span, "case constructor has no representation"))?;
        let actual_tag = self.fresh(ValueShape::Integer);
        let mut prefix = vec![Assignment {
            destination: actual_tag,
            kind: AssignmentKind::VariantTag {
                destination: actual_tag,
                representation,
                value: scrutinee,
            },
            span,
        }];
        let mut conditions = Vec::with_capacity(edges.len());
        for edge in edges {
            let Test::Constructor { tag, .. } = edge.test else {
                return Err(case_error(
                    edge.span,
                    "variant switch contains a product edge",
                ));
            };
            let expected = self.fresh(ValueShape::Integer);
            prefix.push(Assignment {
                destination: expected,
                kind: AssignmentKind::Constant(tag as i32),
                span: edge.span,
            });
            let condition = self.fresh(ValueShape::Boolean);
            prefix.push(Assignment {
                destination: condition,
                kind: AssignmentKind::Primitive {
                    op: BinaryOp::IntEq,
                    left: actual_tag,
                    right: expected,
                },
                span: edge.span,
            });
            conditions.push(condition);
        }
        let (default_assignments, default_value) = if let Some(default) = default {
            let (mapping, default_values) = self.apply_action_list(default_actions, values)?;
            let (mut body, value) = self.lower_decision_node(
                dag,
                default,
                branches,
                &default_values,
                result_type,
                span,
            )?;
            let mut mapping = mapping;
            mapping.append(&mut body);
            (mapping, value)
        } else {
            self.unreachable_value(result_type, span)
        };
        let mut tail = (default_assignments, default_value);
        for (edge, condition) in edges.iter().zip(conditions).rev() {
            let (mut then_assignments, arm_values) = self.apply_actions(edge, values)?;
            let (mut body_assignments, then_value) = self.lower_decision_node(
                dag,
                edge.target,
                branches,
                &arm_values,
                result_type,
                edge.span,
            )?;
            then_assignments.append(&mut body_assignments);
            let destination = self.fresh(result_type);
            let assignment = Assignment {
                destination,
                kind: AssignmentKind::If {
                    condition,
                    then_assignments,
                    then_value,
                    else_assignments: tail.0,
                    else_value: tail.1,
                },
                span: edge.span,
            };
            tail = (vec![assignment], destination);
        }
        prefix.append(&mut tail.0);
        Ok((prefix, tail.1))
    }

    fn apply_actions(
        &mut self,
        edge: &DecisionEdge,
        values: &HashMap<ColumnKey, ValueId>,
    ) -> Result<ActionResult, Vec<BackendError>> {
        self.apply_action_list(&edge.actions, values)
    }

    fn apply_action_list(
        &mut self,
        actions: &[Action],
        values: &HashMap<ColumnKey, ValueId>,
    ) -> Result<ActionResult, Vec<BackendError>> {
        let mut assignments = Vec::new();
        let mut projected = values.clone();
        for action in actions {
            if let Action::Map {
                source,
                target,
                span,
            } = action
            {
                let value = lookup(values, source, *span)?;
                projected.insert(target.clone(), value);
                continue;
            }
            let Action::Project {
                source,
                target,
                field,
                source_type,
                declared_type,
                target_type,
                constructor,
                newtype,
                span,
            } = action
            else {
                continue;
            };
            let source_value = lookup(values, source, *span)?;
            if *newtype {
                projected.insert(target.clone(), source_value);
                continue;
            }
            let value = if let Some((symbol, tag)) = constructor {
                let representation =
                    self.constructor_types.get(symbol).copied().ok_or_else(|| {
                        case_error(*span, "case constructor has no representation")
                    })?;
                if depends_on_type_variable(self.module, *declared_type) {
                    self.lower_erased_field(
                        *target_type,
                        source_value,
                        VariantField {
                            representation,
                            case: *tag,
                            field: *field,
                        },
                        *span,
                        &mut assignments,
                    )?
                } else {
                    let shape = scalar_type(
                        self.module,
                        *target_type,
                        *span,
                        self.enum_types,
                        self.aggregate_types,
                        self.newtype_ids,
                        self.array_types,
                        self.record_types,
                        self.function_types,
                    )?;
                    let value = self.fresh(shape);
                    assignments.push(Assignment {
                        destination: value,
                        kind: AssignmentKind::VariantGet {
                            destination: value,
                            representation,
                            case: *tag,
                            field: *field,
                            value: source_value,
                        },
                        span: *span,
                    });
                    value
                }
            } else {
                let representation =
                    self.record_types.get(source_type).copied().ok_or_else(|| {
                        case_error(*span, "record pattern has no representation requirement")
                    })?;
                let shape = scalar_type(
                    self.module,
                    *target_type,
                    *span,
                    self.enum_types,
                    self.aggregate_types,
                    self.newtype_ids,
                    self.array_types,
                    self.record_types,
                    self.function_types,
                )?;
                let value = self.fresh(shape);
                assignments.push(Assignment {
                    destination: value,
                    kind: AssignmentKind::ProductGet {
                        destination: value,
                        representation,
                        field: *field,
                        value: source_value,
                    },
                    span: *span,
                });
                value
            };
            projected.insert(target.clone(), value);
        }
        Ok((assignments, projected))
    }

    fn is_nullary_sum(&self, ty: psrs_core::TypeId) -> bool {
        user_type_id(self.module, ty).is_some_and(|type_id| self.enum_types.contains(&type_id))
    }

    fn unreachable_value(&mut self, ty: ValueShape, span: TextRange) -> (Vec<Assignment>, ValueId) {
        let destination = self.fresh(ty);
        (
            vec![Assignment {
                destination,
                kind: AssignmentKind::Unreachable,
                span,
            }],
            destination,
        )
    }
}

fn lookup(
    values: &HashMap<ColumnKey, ValueId>,
    key: &ColumnKey,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    values
        .get(key)
        .copied()
        .ok_or_else(|| case_error(span, "decision DAG references a value before projecting it"))
}

fn case_error(span: TextRange, message: impl Into<String>) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
}
