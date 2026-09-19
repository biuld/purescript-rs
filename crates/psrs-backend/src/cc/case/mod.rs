use super::layout::user_type_id;
use super::lower::FunctionLowerer;
use super::{Assignment, AssignmentKind, ValueId, ValueType};
use crate::BackendError;
use psrs_core::{CaseBranch, PatternKind, Primitive};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::HashSet;

mod aggregate;
mod clone;
mod erased;

impl FunctionLowerer<'_> {
    /// Lowers a `case` over a type whose constructors are all nullary into a
    /// chain of tag comparisons.
    pub(super) fn lower_case(
        &mut self,
        scrutinee_type: psrs_core::TypeId,
        scrutinee: ValueId,
        branches: &[CaseBranch],
        result_type: ValueType,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(type_id) = user_type_id(self.module, scrutinee_type) else {
            return Err(case_error(span, "case scrutinee is not a data type"));
        };
        if self.newtype_ids.contains(&type_id) {
            return self.lower_newtype_case(
                type_id,
                scrutinee,
                branches,
                result_type,
                span,
                assignments,
            );
        }
        if self.aggregate_types.contains(&type_id) {
            return self.lower_aggregate_case(
                type_id,
                scrutinee,
                branches,
                result_type,
                span,
                assignments,
            );
        }
        let Some(constructors) = self.constructors_by_type.get(&type_id).cloned() else {
            return Err(case_error(
                span,
                "case scrutinee type has no record of its constructors",
            ));
        };
        let mut constructor_branches: Vec<(&CaseBranch, u32)> = Vec::new();
        let mut default: Option<&CaseBranch> = None;
        let mut covered: HashSet<SymbolId> = HashSet::new();
        for branch in branches {
            match &branch.pattern.kind {
                PatternKind::Constructor { symbol, arguments } => {
                    if !arguments.is_empty() {
                        return Err(case_error(
                            span,
                            "field constructor patterns require GC aggregate lowering",
                        ));
                    }
                    let Some((_, tag)) = constructors.iter().find(|(known, _)| known == symbol)
                    else {
                        return Err(case_error(
                            span,
                            "case pattern constructor does not belong to the scrutinee type",
                        ));
                    };
                    covered.insert(*symbol);
                    constructor_branches.push((branch, *tag));
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
            let (branch, _) = constructor_branches
                .pop()
                .expect("a fully covered case has at least one constructor branch");
            let mut fallback_assignments = Vec::new();
            let value = self.lower_branch(branch, scrutinee, &mut fallback_assignments)?;
            (fallback_assignments, value)
        };

        let (built, value) = self.build_case(
            scrutinee,
            &constructor_branches,
            fallback,
            result_type,
            span,
        )?;
        assignments.extend(built);
        Ok(value)
    }

    fn lower_newtype_case(
        &mut self,
        type_id: psrs_hir::TypeId,
        scrutinee: ValueId,
        branches: &[CaseBranch],
        result_type: ValueType,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(constructor) = self
            .module
            .constructors
            .iter()
            .find(|constructor| constructor.type_id == type_id)
        else {
            return Err(case_error(span, "newtype has no constructor"));
        };
        let Some(field_type) = constructor.field_types.first().copied() else {
            return Err(case_error(span, "newtype has no field"));
        };
        let mut lowered_branches = Vec::with_capacity(branches.len());
        let mut has_constructor_pattern = false;
        for branch in branches {
            let pattern = match &branch.pattern.kind {
                PatternKind::Wildcard | PatternKind::Var { .. } => branch.pattern.clone(),
                PatternKind::Constructor { symbol, arguments } => {
                    if *symbol != constructor.symbol {
                        return Err(case_error(
                            branch.pattern.span,
                            "case pattern constructor does not belong to the newtype",
                        ));
                    }
                    if arguments.len() != 1 {
                        return Err(case_error(
                            branch.pattern.span,
                            "newtype pattern must have exactly one field",
                        ));
                    }
                    has_constructor_pattern |=
                        matches!(arguments[0].kind, PatternKind::Constructor { .. });
                    arguments[0].clone()
                }
            };
            lowered_branches.push(CaseBranch {
                pattern,
                value: branch.value.clone(),
                span: branch.span,
            });
        }
        if !has_constructor_pattern {
            let Some(branch) = lowered_branches.first() else {
                return Err(case_error(
                    span,
                    "non-exhaustive case requires a wildcard alternative",
                ));
            };
            return self.lower_branch(branch, scrutinee, assignments);
        }
        self.lower_case(
            field_type,
            scrutinee,
            &lowered_branches,
            result_type,
            span,
            assignments,
        )
    }

    fn build_case(
        &mut self,
        scrutinee: ValueId,
        constructor_branches: &[(&CaseBranch, u32)],
        fallback: (Vec<Assignment>, ValueId),
        result_type: ValueType,
        span: TextRange,
    ) -> Result<(Vec<Assignment>, ValueId), Vec<BackendError>> {
        let Some((branch, tag)) = constructor_branches.last() else {
            return Ok(fallback);
        };
        let rest = &constructor_branches[..constructor_branches.len() - 1];
        let (else_assignments, else_value) =
            self.build_case(scrutinee, rest, fallback, result_type, span)?;

        let mut prefix = Vec::new();
        let tag_value = self.fresh(ValueType::I32);
        prefix.push(Assignment {
            destination: tag_value,
            kind: AssignmentKind::Constant(*tag as i32),
            span,
        });
        let condition = self.fresh(ValueType::Boolean);
        prefix.push(Assignment {
            destination: condition,
            kind: AssignmentKind::Primitive {
                op: Primitive::Eq,
                left: scrutinee,
                right: tag_value,
            },
            span,
        });

        let mut then_assignments = Vec::new();
        let then_value = self.lower_branch(branch, scrutinee, &mut then_assignments)?;
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

    fn lower_branch(
        &mut self,
        branch: &CaseBranch,
        scrutinee: ValueId,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let bound = if let PatternKind::Var { id, .. } = &branch.pattern.kind {
            self.locals.insert(*id, scrutinee);
            Some(*id)
        } else {
            None
        };
        let value = self.lower_value(&branch.value, assignments)?;
        if let Some(id) = bound {
            self.locals.remove(&id);
        }
        Ok(value)
    }
}

fn case_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
}
