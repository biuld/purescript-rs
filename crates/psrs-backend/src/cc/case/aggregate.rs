use super::super::layout::{depends_on_type_variable, scalar_type, user_type_id};
use super::super::lower::FunctionLowerer;
use super::super::{Assignment, AssignmentKind, BinaryOp, ReprId, ValueId, ValueShape};
use super::clone::AssignmentCloning;
use super::erased::VariantField;
use super::{PatternState, case_error, decision};
use crate::BackendError;
use psrs_core::{CaseBranch, PatternKind};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    pub(super) fn lower_aggregate_case(
        &mut self,
        type_id: psrs_hir::TypeId,
        scrutinee: ValueId,
        branches: &[CaseBranch],
        result_type: ValueShape,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(constructors) = self.constructors_by_type.get(&type_id).cloned() else {
            return Err(case_error(span, "case scrutinee type has no constructors"));
        };
        let compiled =
            decision::compile(branches, span).map_err(|message| case_error(span, message))?;
        let mut constructor_branches = Vec::new();
        let mut covered = std::collections::HashSet::new();
        for alternative in &compiled.alternatives {
            let decision::Matcher::Constructor(symbol) = alternative.matcher else {
                return Err(case_error(
                    branches[alternative.branch].pattern.span,
                    "record pattern does not match an algebraic data type",
                ));
            };
            let branch = &branches[alternative.branch];
            match &branch.pattern.kind {
                PatternKind::Constructor { .. } => {
                    let Some((_, tag)) = constructors.iter().find(|(known, _)| known == &symbol)
                    else {
                        return Err(case_error(
                            span,
                            "case pattern constructor does not belong to the scrutinee type",
                        ));
                    };
                    let Some(representation) = self.constructor_types.get(&symbol).copied() else {
                        return Err(case_error(span, "case constructor has no representation"));
                    };
                    covered.insert(symbol);
                    constructor_branches.push((branch, *tag, representation));
                }
                _ => unreachable!("decision matcher and pattern disagree"),
            }
        }
        let fallback = if let Some(index) = compiled.fallback {
            let branch = &branches[index];
            let mut fallback_assignments = Vec::new();
            let value = self.lower_branch(branch, scrutinee, &mut fallback_assignments)?;
            (fallback_assignments, value)
        } else {
            if covered.len() != constructors.len() {
                return Err(case_error(
                    span,
                    "non-exhaustive case requires a wildcard alternative",
                ));
            }
            let (branch, _, representation) = constructor_branches
                .pop()
                .expect("a fully covered aggregate case has a constructor branch");
            let mut fallback_assignments = Vec::new();
            let (value, _) = self.lower_constructor_branch(
                branch,
                scrutinee,
                representation,
                false,
                &mut fallback_assignments,
            )?;
            (fallback_assignments, value)
        };
        let (built, value) = self.build_aggregate_case(
            scrutinee,
            &constructor_branches,
            fallback,
            result_type,
            span,
        )?;
        assignments.extend(built);
        Ok(value)
    }

    pub(super) fn build_aggregate_case(
        &mut self,
        scrutinee: ValueId,
        branches: &[(&CaseBranch, u32, ReprId)],
        fallback: (Vec<Assignment>, ValueId),
        result_type: ValueShape,
        span: TextRange,
    ) -> Result<(Vec<Assignment>, ValueId), Vec<BackendError>> {
        let Some((branch, case, representation)) = branches.first() else {
            return Ok(fallback);
        };
        let rest = &branches[1..];
        let (else_assignments, else_value) =
            self.build_aggregate_case(scrutinee, rest, fallback, result_type, span)?;
        let actual_tag = self.fresh(ValueShape::Integer);
        let expected_tag = self.fresh(ValueShape::Integer);
        let condition = self.fresh(ValueShape::Boolean);
        let mut prefix = vec![
            Assignment {
                destination: actual_tag,
                kind: AssignmentKind::VariantTag {
                    destination: actual_tag,
                    representation: *representation,
                    value: scrutinee,
                },
                span,
            },
            Assignment {
                destination: expected_tag,
                kind: AssignmentKind::Constant(*case as i32),
                span,
            },
            Assignment {
                destination: condition,
                kind: AssignmentKind::Primitive {
                    op: BinaryOp::IntEq,
                    left: actual_tag,
                    right: expected_tag,
                },
                span,
            },
        ];
        let mut then_assignments = Vec::new();
        let (then_value, conditions) = self.lower_constructor_branch(
            branch,
            scrutinee,
            *representation,
            true,
            &mut then_assignments,
        )?;
        let then_value = if conditions.is_empty() {
            then_value
        } else {
            let positions = conditions
                .iter()
                .map(|condition| {
                    then_assignments
                        .iter()
                        .position(|assignment| assignment.destination == *condition)
                        .expect("nested case condition has an assignment")
                })
                .collect::<Vec<_>>();
            let mut nested_assignment = None;
            let mut nested_value = then_value;
            for (index, condition) in conditions.iter().enumerate().rev() {
                let start = positions[index] + 1;
                let end = positions
                    .get(index + 1)
                    .map_or(then_assignments.len(), |position| position + 1);
                let mut nested_then_assignments = then_assignments[start..end].to_vec();
                if let Some(assignment) = nested_assignment.take() {
                    nested_then_assignments.push(assignment);
                }
                let (nested_else_assignments, nested_else_value) =
                    self.clone_assignments(&else_assignments, else_value);
                let destination = self.fresh(result_type);
                nested_assignment = Some(Assignment {
                    destination,
                    kind: AssignmentKind::If {
                        condition: *condition,
                        then_assignments: nested_then_assignments,
                        then_value: nested_value,
                        else_assignments: nested_else_assignments,
                        else_value: nested_else_value,
                    },
                    span,
                });
                nested_value = destination;
            }
            let first_condition = positions[0] + 1;
            let mut prefix = then_assignments[..first_condition].to_vec();
            prefix.push(nested_assignment.expect("nested case has a conditional assignment"));
            then_assignments = prefix;
            nested_value
        };
        let destination = self.fresh(result_type);
        prefix.push(Assignment {
            destination,
            kind: AssignmentKind::If {
                condition,
                then_assignments,
                then_value,
                else_assignments,
                else_value,
            },
            span,
        });
        Ok((prefix, destination))
    }

    pub(super) fn lower_constructor_branch(
        &mut self,
        branch: &CaseBranch,
        scrutinee: ValueId,
        representation: ReprId,
        check_nested: bool,
        assignments: &mut Vec<Assignment>,
    ) -> Result<(ValueId, Vec<ValueId>), Vec<BackendError>> {
        let PatternKind::Constructor { arguments, .. } = &branch.pattern.kind else {
            return self
                .lower_branch(branch, scrutinee, assignments)
                .map(|value| (value, Vec::new()));
        };
        let constructor = self
            .module
            .constructors
            .iter()
            .find(|constructor| {
                constructor.symbol
                    == match &branch.pattern.kind {
                        PatternKind::Constructor { symbol, .. } => *symbol,
                        _ => unreachable!(),
                    }
            })
            .ok_or_else(|| case_error(branch.span, "case constructor is not declared"))?;
        if arguments.len() != constructor.field_count {
            return Err(case_error(
                branch.span,
                "constructor pattern has the wrong field count",
            ));
        }
        let mut bound = Vec::new();
        let mut conditions = Vec::new();
        for (field, pattern) in arguments.iter().enumerate() {
            if matches!(pattern.kind, PatternKind::Wildcard) {
                continue;
            }
            let value = if depends_on_type_variable(self.module, constructor.field_types[field]) {
                self.lower_erased_field(
                    pattern.ty,
                    scrutinee,
                    VariantField {
                        representation,
                        case: constructor.tag,
                        field: field as u32,
                    },
                    pattern.span,
                    assignments,
                )?
            } else {
                let field_type = scalar_type(
                    self.module,
                    constructor.field_types[field],
                    pattern.span,
                    self.enum_types,
                    self.aggregate_types,
                    self.newtype_ids,
                    self.array_types,
                    self.record_types,
                    self.function_types,
                )?;
                let value = self.fresh(field_type);
                assignments.push(Assignment {
                    destination: value,
                    kind: AssignmentKind::VariantGet {
                        destination: value,
                        representation,
                        case: constructor.tag,
                        field: field as u32,
                        value: scrutinee,
                    },
                    span: pattern.span,
                });
                value
            };
            let mut state = PatternState {
                check_nested,
                conditions: &mut conditions,
                bound: &mut bound,
                assignments,
            };
            self.lower_pattern(pattern, value, constructor.field_types[field], &mut state)?;
        }
        let value = self.lower_value(&branch.value, assignments);
        for id in bound {
            self.locals.remove(&id);
        }
        value.map(|value| (value, conditions))
    }

    pub(super) fn lower_pattern(
        &mut self,
        pattern: &psrs_core::Pattern,
        value: ValueId,
        source_type: psrs_core::TypeId,
        state: &mut PatternState<'_>,
    ) -> Result<(), Vec<BackendError>> {
        let source_type = if depends_on_type_variable(self.module, source_type) {
            pattern.ty
        } else {
            source_type
        };
        match &pattern.kind {
            PatternKind::Wildcard => Ok(()),
            PatternKind::Var { id, .. } => {
                self.locals.insert(*id, value);
                state.bound.push(*id);
                Ok(())
            }
            PatternKind::Constructor { symbol, arguments } => {
                let Some(type_id) = user_type_id(self.module, source_type) else {
                    return Err(case_error(
                        pattern.span,
                        "nested constructor pattern has no data type",
                    ));
                };
                let Some(constructor) = self
                    .module
                    .constructors
                    .iter()
                    .find(|constructor| constructor.symbol == *symbol)
                else {
                    return Err(case_error(
                        pattern.span,
                        "nested constructor pattern is not declared",
                    ));
                };
                if constructor.type_id != type_id {
                    return Err(case_error(
                        pattern.span,
                        "nested constructor pattern does not match its field type",
                    ));
                }
                let Some(constructors) = self.constructors_by_type.get(&type_id) else {
                    return Err(case_error(
                        pattern.span,
                        "nested field type has no constructor table",
                    ));
                };
                let single_constructor = constructors.len() == 1;
                if arguments.len() != constructor.field_count {
                    return Err(case_error(
                        pattern.span,
                        "nested constructor pattern has the wrong field count",
                    ));
                }
                if self.newtype_ids.contains(&type_id) {
                    let Some(field_type) = constructor.field_types.first().copied() else {
                        return Err(case_error(
                            pattern.span,
                            "nested newtype constructor has no field",
                        ));
                    };
                    let Some(field_pattern) = arguments.first() else {
                        return Err(case_error(
                            pattern.span,
                            "nested newtype constructor has no pattern",
                        ));
                    };
                    return self.lower_pattern(field_pattern, value, field_type, state);
                }
                if !self.aggregate_types.contains(&type_id) {
                    if !single_constructor && state.check_nested {
                        let Some(tag) = self.constructor_tags.get(symbol).copied() else {
                            return Err(case_error(
                                pattern.span,
                                "nested enum constructor has no tag",
                            ));
                        };
                        let expected = self.fresh(ValueShape::Integer);
                        state.assignments.push(Assignment {
                            destination: expected,
                            kind: AssignmentKind::Constant(tag as i32),
                            span: pattern.span,
                        });
                        let condition = self.fresh(ValueShape::Boolean);
                        state.assignments.push(Assignment {
                            destination: condition,
                            kind: AssignmentKind::Primitive {
                                op: BinaryOp::IntEq,
                                left: value,
                                right: expected,
                            },
                            span: pattern.span,
                        });
                        state.conditions.push(condition);
                    }
                    return Ok(());
                }
                let Some(representation) = self.constructor_types.get(symbol).copied() else {
                    return Err(case_error(
                        pattern.span,
                        "nested constructor has no representation",
                    ));
                };
                if !single_constructor && state.check_nested {
                    let actual_tag = self.fresh(ValueShape::Integer);
                    let expected_tag = self.fresh(ValueShape::Integer);
                    let condition = self.fresh(ValueShape::Boolean);
                    state.assignments.push(Assignment {
                        destination: actual_tag,
                        kind: AssignmentKind::VariantTag {
                            destination: actual_tag,
                            representation,
                            value,
                        },
                        span: pattern.span,
                    });
                    state.assignments.push(Assignment {
                        destination: expected_tag,
                        kind: AssignmentKind::Constant(constructor.tag as i32),
                        span: pattern.span,
                    });
                    state.assignments.push(Assignment {
                        destination: condition,
                        kind: AssignmentKind::Primitive {
                            op: BinaryOp::IntEq,
                            left: actual_tag,
                            right: expected_tag,
                        },
                        span: pattern.span,
                    });
                    state.conditions.push(condition);
                }
                for (field, child) in arguments.iter().enumerate() {
                    if matches!(child.kind, PatternKind::Wildcard) {
                        continue;
                    }
                    if depends_on_type_variable(self.module, constructor.field_types[field]) {
                        let child_value = self.lower_erased_field(
                            child.ty,
                            value,
                            VariantField {
                                representation,
                                case: constructor.tag,
                                field: field as u32,
                            },
                            child.span,
                            state.assignments,
                        )?;
                        self.lower_pattern(
                            child,
                            child_value,
                            constructor.field_types[field],
                            state,
                        )?;
                        continue;
                    }
                    let child_type = scalar_type(
                        self.module,
                        constructor.field_types[field],
                        child.span,
                        self.enum_types,
                        self.aggregate_types,
                        self.newtype_ids,
                        self.array_types,
                        self.record_types,
                        self.function_types,
                    )?;
                    let child_value = self.fresh(child_type);
                    state.assignments.push(Assignment {
                        destination: child_value,
                        kind: AssignmentKind::VariantGet {
                            destination: child_value,
                            representation,
                            case: constructor.tag,
                            field: field as u32,
                            value,
                        },
                        span: child.span,
                    });
                    self.lower_pattern(child, child_value, constructor.field_types[field], state)?;
                }
                Ok(())
            }
            PatternKind::Record { fields } => {
                self.lower_record_pattern(fields, value, source_type, state)
            }
        }
    }
}
