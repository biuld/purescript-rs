use super::super::layout::{depends_on_type_variable, scalar_type};
use super::super::lower::FunctionLowerer;
use super::super::{Assignment, AssignmentKind, ValueId, ValueType};
use super::case_error;
use crate::BackendError;
use crate::types::{HeapType, RefType};
use psrs_core::{CaseBranch, PatternKind};
use psrs_span::TextRange;
use std::collections::HashSet;

impl FunctionLowerer<'_> {
    pub(super) fn lower_aggregate_case(
        &mut self,
        type_id: psrs_hir::TypeId,
        scrutinee: ValueId,
        branches: &[CaseBranch],
        result_type: ValueType,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(constructors) = self.constructors_by_type.get(&type_id).cloned() else {
            return Err(case_error(span, "case scrutinee type has no constructors"));
        };
        let mut constructor_branches = Vec::new();
        let mut default = None;
        let mut covered = HashSet::new();
        for branch in branches {
            match &branch.pattern.kind {
                PatternKind::Constructor { symbol, .. } => {
                    let Some((_, tag)) = constructors.iter().find(|(known, _)| known == symbol)
                    else {
                        return Err(case_error(
                            span,
                            "case pattern constructor does not belong to the scrutinee type",
                        ));
                    };
                    let Some(type_index) = self.constructor_types.get(symbol).copied() else {
                        return Err(case_error(span, "case constructor has no GC type layout"));
                    };
                    covered.insert(*symbol);
                    constructor_branches.push((branch, *tag, type_index));
                }
                PatternKind::Wildcard | PatternKind::Var { .. } => default = Some(branch),
            }
        }
        let fallback = if let Some(branch) = default {
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
            let (branch, _, type_index) = constructor_branches
                .pop()
                .expect("a fully covered aggregate case has a constructor branch");
            let mut fallback_assignments = Vec::new();
            let value = self.lower_constructor_branch(
                branch,
                scrutinee,
                type_index,
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
        branches: &[(&CaseBranch, u32, u32)],
        fallback: (Vec<Assignment>, ValueId),
        result_type: ValueType,
        span: TextRange,
    ) -> Result<(Vec<Assignment>, ValueId), Vec<BackendError>> {
        let Some((branch, _, type_index)) = branches.last() else {
            return Ok(fallback);
        };
        let rest = &branches[..branches.len() - 1];
        let (else_assignments, else_value) =
            self.build_aggregate_case(scrutinee, rest, fallback, result_type, span)?;
        let condition = self.fresh(ValueType::Boolean);
        let mut prefix = vec![Assignment {
            destination: condition,
            kind: AssignmentKind::RefTest {
                destination: condition,
                value: scrutinee,
                reference: RefType {
                    nullable: false,
                    heap: HeapType::Index(*type_index),
                },
            },
            span,
        }];
        let mut then_assignments = Vec::new();
        let then_value =
            self.lower_constructor_branch(branch, scrutinee, *type_index, &mut then_assignments)?;
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
        type_index: u32,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let PatternKind::Constructor { arguments, .. } = &branch.pattern.kind else {
            return self.lower_branch(branch, scrutinee, assignments);
        };
        let cast = self.fresh(crate::types::ValueType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(type_index),
        }));
        assignments.push(Assignment {
            destination: cast,
            kind: AssignmentKind::RefCast {
                destination: cast,
                value: scrutinee,
                reference: RefType {
                    nullable: false,
                    heap: HeapType::Index(type_index),
                },
            },
            span: branch.span,
        });
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
        for (field, pattern) in arguments.iter().enumerate() {
            match &pattern.kind {
                PatternKind::Var { id, ty } => {
                    let value =
                        if depends_on_type_variable(self.module, constructor.field_types[field]) {
                            self.lower_erased_field(
                                *ty,
                                cast,
                                type_index,
                                field as u32 + 1,
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
                            )?;
                            let value = self.fresh(field_type);
                            assignments.push(Assignment {
                                destination: value,
                                kind: AssignmentKind::StructGet {
                                    destination: value,
                                    type_index,
                                    field: field as u32 + 1,
                                    value: cast,
                                },
                                span: pattern.span,
                            });
                            value
                        };
                    self.locals.insert(*id, value);
                    bound.push(*id);
                }
                PatternKind::Wildcard => {}
                PatternKind::Constructor { .. } => {
                    return Err(case_error(
                        pattern.span,
                        "nested field constructor patterns are not supported yet",
                    ));
                }
            }
        }
        let value = self.lower_value(&branch.value, assignments);
        for id in bound {
            self.locals.remove(&id);
        }
        value
    }
}
