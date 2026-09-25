use super::layout::user_type_id;
use super::lower::FunctionLowerer;
use super::{Assignment, ValueId, ValueShape};
use crate::{BackendError, BackendWarning};
use psrs_core::CaseBranch;
use psrs_span::TextRange;

mod coverage;
mod decision;

impl FunctionLowerer<'_> {
    /// Compiles a checked case matrix into a shared decision DAG and realizes it in CC.
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
        let is_record = matches!(
            self.module.types.get(scrutinee_type.0 as usize),
            Some(psrs_core::Type::Record(_))
        );
        let type_id = if is_record {
            None
        } else {
            Some(
                user_type_id(self.module, scrutinee_type)
                    .ok_or_else(|| case_error(span, "case scrutinee is not a data type"))?,
            )
        };
        require_exhaustive(span, branches, &coverage)?;
        self.report_redundant_branches(branches, &coverage);
        if type_id.is_some_and(|type_id| {
            !self.newtype_ids.contains(&type_id)
                && !self.aggregate_types.contains(&type_id)
                && !self.enum_types.contains(&type_id)
        }) {
            return Err(case_error(
                span,
                "case scrutinee type has no runtime representation",
            ));
        }
        let dag = decision::compile_dag(
            self.module,
            self.newtype_ids,
            scrutinee_type,
            branches,
            span,
        )
        .map_err(|message| case_error(span, message))?;
        self.lower_decision(&dag, branches, scrutinee, result_type, span, assignments)
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
