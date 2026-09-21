use super::super::layout::scalar_type;
use super::super::lower::FunctionLowerer;
use super::super::{Assignment, AssignmentKind, ValueId, ValueShape};
use super::PatternState;
use super::case_error;
use super::clone::AssignmentCloning;
use super::decision;
use crate::BackendError;
use psrs_core::{CaseBranch, Pattern, PatternKind, Type};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    pub(super) fn lower_record_case(
        &mut self,
        scrutinee_type: psrs_core::TypeId,
        scrutinee: ValueId,
        branches: &[CaseBranch],
        result_type: ValueShape,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let mut fallback = None;
        let compiled =
            decision::compile(branches, span).map_err(|message| case_error(span, message))?;
        for index in compiled.ordered.iter().rev() {
            let branch = &branches[*index];
            match &branch.pattern.kind {
                PatternKind::Wildcard | PatternKind::Var { .. } => {
                    let mut branch_assignments = Vec::new();
                    let value = self.lower_branch(branch, scrutinee, &mut branch_assignments)?;
                    fallback = Some((branch_assignments, value));
                }
                PatternKind::Record { .. } => {
                    let (branch_assignments, value, conditions) =
                        self.lower_record_branch(branch, scrutinee, scrutinee_type)?;
                    if conditions.is_empty() {
                        fallback = Some((branch_assignments, value));
                    } else {
                        let Some(else_case) = fallback.take() else {
                            return Err(case_error(
                                branch.pattern.span,
                                "non-exhaustive record case requires a wildcard alternative",
                            ));
                        };
                        fallback = Some(self.build_record_branch(
                            branch_assignments,
                            value,
                            conditions,
                            else_case,
                            result_type,
                            span,
                        )?);
                    }
                }
                _ => {
                    return Err(case_error(
                        branch.pattern.span,
                        "record case requires a record, wildcard, or variable pattern",
                    ));
                }
            }
        }
        let Some((built, value)) = fallback else {
            return Err(case_error(span, "record case has no alternatives"));
        };
        assignments.extend(built);
        Ok(value)
    }

    fn lower_record_branch(
        &mut self,
        branch: &CaseBranch,
        scrutinee: ValueId,
        scrutinee_type: psrs_core::TypeId,
    ) -> Result<(Vec<Assignment>, ValueId, Vec<ValueId>), Vec<BackendError>> {
        let PatternKind::Record { fields } = &branch.pattern.kind else {
            unreachable!("record branch lowering received another pattern");
        };
        let mut assignments = Vec::new();
        let mut bound = Vec::new();
        let mut conditions = Vec::new();
        let mut state = PatternState {
            check_nested: true,
            conditions: &mut conditions,
            bound: &mut bound,
            assignments: &mut assignments,
        };
        self.lower_record_pattern(fields, scrutinee, scrutinee_type, &mut state)?;
        let value = self.lower_value(&branch.value, &mut assignments)?;
        for id in bound {
            self.locals.remove(&id);
        }
        Ok((assignments, value, conditions))
    }

    fn build_record_branch(
        &mut self,
        branch_assignments: Vec<Assignment>,
        branch_value: ValueId,
        conditions: Vec<ValueId>,
        fallback: (Vec<Assignment>, ValueId),
        result_type: ValueShape,
        span: TextRange,
    ) -> Result<(Vec<Assignment>, ValueId), Vec<BackendError>> {
        let positions = conditions
            .iter()
            .map(|condition| {
                branch_assignments
                    .iter()
                    .position(|assignment| assignment.destination == *condition)
                    .expect("record pattern condition has an assignment")
            })
            .collect::<Vec<_>>();
        let mut nested_assignment = None;
        let mut nested_value = branch_value;
        for (index, condition) in conditions.iter().enumerate().rev() {
            let start = positions[index] + 1;
            let end = positions
                .get(index + 1)
                .map_or(branch_assignments.len(), |position| position + 1);
            let mut then_assignments = branch_assignments[start..end].to_vec();
            if let Some(assignment) = nested_assignment.take() {
                then_assignments.push(assignment);
            }
            let (else_assignments, else_value) = self.clone_assignments(&fallback.0, fallback.1);
            let destination = self.fresh(result_type);
            nested_assignment = Some(Assignment {
                destination,
                kind: AssignmentKind::If {
                    condition: *condition,
                    then_assignments,
                    then_value: nested_value,
                    else_assignments,
                    else_value,
                },
                span,
            });
            nested_value = destination;
        }
        let first_condition = positions[0] + 1;
        let mut prefix = branch_assignments[..first_condition].to_vec();
        prefix.push(nested_assignment.expect("record pattern has a conditional assignment"));
        Ok((prefix, nested_value))
    }

    pub(super) fn lower_record_pattern(
        &mut self,
        fields: &[(String, Pattern)],
        value: ValueId,
        source_type: psrs_core::TypeId,
        state: &mut PatternState<'_>,
    ) -> Result<(), Vec<BackendError>> {
        let Some(representation) = self.record_types.get(&source_type).copied() else {
            return Err(case_error(
                fields
                    .first()
                    .map_or(TextRange::default(), |(_, field)| field.span),
                "record pattern has no representation requirement",
            ));
        };
        let Some(Type::Record(record_fields)) = self.module.types.get(source_type.0 as usize)
        else {
            return Err(case_error(
                fields
                    .first()
                    .map_or(TextRange::default(), |(_, field)| field.span),
                "record pattern does not match a record type",
            ));
        };
        for (label, pattern) in fields {
            let Some((field_index, (_, field_type))) = record_fields
                .iter()
                .enumerate()
                .find(|(_, (field_label, _))| field_label == label)
            else {
                return Err(case_error(
                    pattern.span,
                    "record pattern field is not present in its type",
                ));
            };
            if matches!(&pattern.kind, PatternKind::Wildcard) {
                continue;
            }
            let value_type = scalar_type(
                self.module,
                *field_type,
                pattern.span,
                self.enum_types,
                self.aggregate_types,
                self.newtype_ids,
                self.array_types,
                self.record_types,
                self.function_types,
            )?;
            let field_value = self.fresh(value_type);
            state.assignments.push(Assignment {
                destination: field_value,
                kind: AssignmentKind::ProductGet {
                    destination: field_value,
                    representation,
                    field: field_index as u32,
                    value,
                },
                span: pattern.span,
            });
            self.lower_pattern(pattern, field_value, *field_type, state)?;
        }
        Ok(())
    }
}
