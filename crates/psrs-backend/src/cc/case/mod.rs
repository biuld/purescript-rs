use super::layout::user_type_id;
use super::lower::FunctionLowerer;
use super::{Assignment, AssignmentKind, BinaryOp, TagCase, ValueId, ValueShape};
use crate::{BackendError, BackendWarning};
use psrs_core::{CaseBranch, PatternKind};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::HashSet;

mod aggregate;
mod clone;
mod coverage;
mod decision;
mod erased;
mod record;

struct PatternState<'a> {
    check_nested: bool,
    conditions: &'a mut Vec<ValueId>,
    bound: &'a mut Vec<psrs_hir::LocalId>,
    assignments: &'a mut Vec<Assignment>,
}

impl FunctionLowerer<'_> {
    /// Lowers a `case` over a type whose constructors are all nullary into a
    /// tag switch when the constructor patterns are unique, preserving a
    /// comparison chain for duplicate patterns that need source-order priority.
    pub(super) fn lower_case(
        &mut self,
        scrutinee_type: psrs_core::TypeId,
        scrutinee: ValueId,
        branches: &[CaseBranch],
        result_type: ValueShape,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let coverage = coverage::analyze(self.module, scrutinee_type, branches);
        if matches!(
            self.module.types.get(scrutinee_type.0 as usize),
            Some(psrs_core::Type::Record(_))
        ) {
            require_exhaustive(span, branches, &coverage)?;
            self.report_redundant_branches(branches, &coverage);
            return self.lower_record_case(
                scrutinee_type,
                scrutinee,
                branches,
                result_type,
                span,
                assignments,
            );
        }
        let Some(type_id) = user_type_id(self.module, scrutinee_type) else {
            return Err(case_error(span, "case scrutinee is not a data type"));
        };
        require_exhaustive(span, branches, &coverage)?;
        self.report_redundant_branches(branches, &coverage);
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
        let compiled =
            decision::compile(branches, span).map_err(|message| case_error(span, message))?;
        let mut constructor_branches: Vec<(&CaseBranch, u32)> = Vec::new();
        let mut covered: HashSet<SymbolId> = HashSet::new();
        for alternative in &compiled.alternatives {
            let decision::Matcher::Constructor(symbol) = alternative.matcher else {
                return Err(case_error(
                    branches[alternative.branch].pattern.span,
                    "record pattern does not match a data type",
                ));
            };
            let branch = &branches[alternative.branch];
            match &branch.pattern.kind {
                PatternKind::Constructor { arguments, .. } => {
                    if !arguments.is_empty() {
                        return Err(case_error(
                            span,
                            "field constructor patterns require aggregate lowering",
                        ));
                    }
                    let Some((_, tag)) = constructors.iter().find(|(known, _)| known == &symbol)
                    else {
                        return Err(case_error(
                            span,
                            "case pattern constructor does not belong to the scrutinee type",
                        ));
                    };
                    covered.insert(symbol);
                    constructor_branches.push((branch, *tag));
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
                    coverage.non_exhaustive_message(
                        "non-exhaustive case requires a wildcard alternative",
                    ),
                ));
            }
            let (branch, _) = constructor_branches
                .pop()
                .expect("a fully covered case has at least one constructor branch");
            let mut fallback_assignments = Vec::new();
            let value = self.lower_branch(branch, scrutinee, &mut fallback_assignments)?;
            (fallback_assignments, value)
        };

        let has_unique_tags = constructor_branches
            .iter()
            .map(|(_, tag)| *tag)
            .collect::<HashSet<_>>()
            .len()
            == constructor_branches.len();
        let (built, value) = if has_unique_tags && !constructor_branches.is_empty() {
            self.build_tag_switch(
                scrutinee,
                &constructor_branches,
                fallback,
                result_type,
                span,
            )?
        } else {
            self.build_case(
                scrutinee,
                &constructor_branches,
                fallback,
                result_type,
                span,
            )?
        };
        assignments.extend(built);
        Ok(value)
    }

    fn report_redundant_branches(
        &mut self,
        branches: &[CaseBranch],
        coverage: &coverage::CoverageReport,
    ) {
        for index in &coverage.redundant_branches {
            self.warnings.push(BackendWarning::new(
                "P8 closure conversion",
                branches[*index].span,
                "redundant case alternative is unreachable",
            ));
        }
    }

    fn build_tag_switch(
        &mut self,
        scrutinee: ValueId,
        constructor_branches: &[(&CaseBranch, u32)],
        fallback: (Vec<Assignment>, ValueId),
        result_type: ValueShape,
        span: TextRange,
    ) -> Result<(Vec<Assignment>, ValueId), Vec<BackendError>> {
        let mut cases = Vec::with_capacity(constructor_branches.len());
        for (branch, tag) in constructor_branches {
            let mut assignments = Vec::new();
            let value = self.lower_branch(branch, scrutinee, &mut assignments)?;
            cases.push(TagCase {
                tag: i32::try_from(*tag)
                    .map_err(|_| case_error(span, "data constructor tag exceeds i32"))?,
                assignments,
                value,
            });
        }
        let destination = self.fresh(result_type);
        Ok((
            vec![Assignment {
                destination,
                kind: AssignmentKind::TagSwitch {
                    value: scrutinee,
                    cases,
                    default_assignments: fallback.0,
                    default_value: fallback.1,
                },
                span,
            }],
            destination,
        ))
    }

    fn lower_newtype_case(
        &mut self,
        type_id: psrs_hir::TypeId,
        scrutinee: ValueId,
        branches: &[CaseBranch],
        result_type: ValueShape,
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
                PatternKind::Record { .. } => {
                    return Err(case_error(
                        branch.pattern.span,
                        "record pattern does not match a newtype",
                    ));
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
                return Err(case_error(span, "newtype case has no alternatives"));
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
        result_type: ValueShape,
        span: TextRange,
    ) -> Result<(Vec<Assignment>, ValueId), Vec<BackendError>> {
        let Some((branch, tag)) = constructor_branches.first() else {
            return Ok(fallback);
        };
        let rest = &constructor_branches[1..];
        let (else_assignments, else_value) =
            self.build_case(scrutinee, rest, fallback, result_type, span)?;

        let mut prefix = Vec::new();
        let tag_value = self.fresh(ValueShape::Integer);
        prefix.push(Assignment {
            destination: tag_value,
            kind: AssignmentKind::Constant(*tag as i32),
            span,
        });
        let condition = self.fresh(ValueShape::Boolean);
        prefix.push(Assignment {
            destination: condition,
            kind: AssignmentKind::Primitive {
                op: BinaryOp::IntEq,
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

fn case_error(span: TextRange, message: impl Into<String>) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
}

fn require_exhaustive(
    span: TextRange,
    branches: &[CaseBranch],
    coverage: &coverage::CoverageReport,
) -> Result<(), Vec<BackendError>> {
    if branches.is_empty() || coverage.exhaustive {
        return Ok(());
    }
    Err(case_error(
        span,
        coverage.non_exhaustive_message("non-exhaustive case requires a wildcard alternative"),
    ))
}
