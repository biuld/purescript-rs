use super::super::super::super::lower::FunctionLowerer;
use super::super::Test;
use super::{SwitchContext, case_error};
use crate::BackendError;
use crate::cc::{Assignment, AssignmentKind, BinaryOp, TagCase, ValueId, ValueShape};

impl FunctionLowerer<'_> {
    pub(super) fn lower_nullary_switch(
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

    pub(super) fn lower_variant_switch(
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
                kind: AssignmentKind::Constant(
                    i32::try_from(tag)
                        .map_err(|_| case_error(edge.span, "constructor tag exceeds i32"))?,
                ),
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
}
