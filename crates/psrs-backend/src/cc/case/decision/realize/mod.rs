use super::super::super::layout::user_type_id;
use super::super::super::lower::{ErasedFieldRecovery, FunctionLowerer};
use super::{Action, ColumnKey, Decision, DecisionDag, DecisionEdge, NodeId, Test};
use crate::BackendError;
use crate::cc::{Assignment, AssignmentKind, ValueId, ValueShape};
use psrs_core::CaseBranch;
use psrs_span::TextRange;
use std::collections::HashMap;

mod switch;

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
                let stored_shape = variant_field_shape(
                    self.representations.representation(representation),
                    *tag,
                    *field as usize,
                    *span,
                )?;
                let target_shape = self.value_shape(*target_type, *span)?;
                let projected = self.fresh(stored_shape);
                assignments.push(Assignment {
                    destination: projected,
                    kind: AssignmentKind::VariantGet {
                        destination: projected,
                        representation,
                        case: *tag,
                        field: *field,
                        value: source_value,
                    },
                    span: *span,
                });
                let conversion = self.erased_field_recovery(ErasedFieldRecovery {
                    variant: representation,
                    tag: *tag,
                    field: *field,
                    template_type: *declared_type,
                    target_type: *target_type,
                    target_shape,
                    stored_shape,
                    span: *span,
                })?;
                self.emit_conversion(
                    projected,
                    stored_shape,
                    target_shape,
                    conversion,
                    *span,
                    &mut assignments,
                )
            } else {
                let representation =
                    self.record_types.get(source_type).copied().ok_or_else(|| {
                        case_error(*span, "record pattern has no representation requirement")
                    })?;
                let canonical_field = *field as usize;
                let stored_shape = product_field_shape(
                    self.representations.representation(representation),
                    canonical_field,
                    *span,
                )?;
                let target_shape = self.value_shape(*target_type, *span)?;
                let projected = self.fresh(stored_shape);
                assignments.push(Assignment {
                    destination: projected,
                    kind: AssignmentKind::ProductGet {
                        destination: projected,
                        representation,
                        field: canonical_field as u32,
                        value: source_value,
                    },
                    span: *span,
                });
                let conversion = self.typed_conversion(
                    *declared_type,
                    *target_type,
                    stored_shape,
                    target_shape,
                    *span,
                )?;
                self.emit_conversion(
                    projected,
                    stored_shape,
                    target_shape,
                    conversion,
                    *span,
                    &mut assignments,
                )
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
    vec![BackendError::invalid_ir(
        "P8 closure conversion",
        span,
        message,
    )]
}

fn variant_field_shape(
    representation: Option<&crate::cc::Representation>,
    tag: u32,
    field: usize,
    span: TextRange,
) -> Result<ValueShape, Vec<BackendError>> {
    let Some(crate::cc::Representation::Variant { cases }) = representation else {
        return Err(case_error(
            span,
            "constructor has no variant representation",
        ));
    };
    cases
        .iter()
        .find(|case| case.tag == tag)
        .and_then(|case| case.fields.get(field))
        .copied()
        .ok_or_else(|| case_error(span, "constructor field has no storage shape"))
}

fn product_field_shape(
    representation: Option<&crate::cc::Representation>,
    field: usize,
    span: TextRange,
) -> Result<ValueShape, Vec<BackendError>> {
    let Some(crate::cc::Representation::Product { fields }) = representation else {
        return Err(case_error(span, "record has no product representation"));
    };
    fields
        .get(field)
        .copied()
        .ok_or_else(|| case_error(span, "record field has no storage shape"))
}
