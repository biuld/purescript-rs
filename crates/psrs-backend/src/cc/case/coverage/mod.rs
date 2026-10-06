//! Usefulness analysis over the decision compiler's shared pattern matrix.
//!
//! Coverage and first-match compilation use the same typed pattern algebra,
//! literal normalization, and constructor, scalar, and array heads.

use super::decision::{SurfacePattern, surface_pattern};
use psrs_core::{CaseBranch, Module, Pattern, TypeId};
use psrs_hir::CaseBranchCoverage;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CoverageReport {
    pub(super) exhaustive: bool,
    pub(super) witness: Option<String>,
    pub(super) redundant_branches: Vec<usize>,
}

impl CoverageReport {
    pub(super) fn non_exhaustive_message(&self, fallback: &'static str) -> String {
        match &self.witness {
            Some(witness) => format!("non-exhaustive case; missing pattern {witness}"),
            None => fallback.to_owned(),
        }
    }
}

mod engine;
use engine::{render, useful};

pub(super) fn analyze(
    module: &Module,
    scrutinee_type: TypeId,
    branches: &[CaseBranch],
) -> CoverageReport {
    if !branches.is_empty()
        && branches
            .iter()
            .all(|branch| branch.coverage == CaseBranchCoverage::Generated)
    {
        return CoverageReport {
            exhaustive: true,
            witness: None,
            redundant_branches: Vec::new(),
        };
    }
    let matrix = branches
        .iter()
        .filter(|branch| {
            matches!(
                branch.coverage,
                CaseBranchCoverage::Source | CaseBranchCoverage::PartialFallback
            )
        })
        .map(|branch| vec![coverage_pattern(&branch.pattern)])
        .collect::<Vec<Vec<SurfacePattern>>>();
    let query = vec![SurfacePattern::Any { ty: scrutinee_type }];
    let witness = useful(module, &matrix, &query)
        .map(|mut patterns| patterns.remove(0))
        .map(|pattern| render(module, &pattern));
    let mut prior = Vec::new();
    let mut redundant_branches = Vec::new();
    for (index, branch) in branches.iter().enumerate() {
        if matches!(
            branch.coverage,
            CaseBranchCoverage::Generated | CaseBranchCoverage::PartialFallback
        ) {
            continue;
        }
        let query = vec![coverage_pattern(&branch.pattern)];
        if useful(module, &prior, &query).is_none() {
            redundant_branches.push(index);
        }
        if branch.coverage == CaseBranchCoverage::Source {
            prior.push(query);
        }
    }
    CoverageReport {
        exhaustive: witness.is_none(),
        witness,
        redundant_branches,
    }
}

fn coverage_pattern(pattern: &Pattern) -> SurfacePattern {
    erase_binders(surface_pattern(pattern))
}

fn erase_binders(pattern: SurfacePattern) -> SurfacePattern {
    match pattern {
        SurfacePattern::Var { ty, .. } => SurfacePattern::Any { ty },
        SurfacePattern::Named { pattern, .. } => erase_binders(*pattern),
        SurfacePattern::Array { elements, ty, span } => SurfacePattern::Array {
            elements: elements.into_iter().map(erase_binders).collect(),
            ty,
            span,
        },
        SurfacePattern::Constructor {
            symbol,
            arguments,
            ty,
            span,
        } => SurfacePattern::Constructor {
            symbol,
            arguments: arguments.into_iter().map(erase_binders).collect(),
            ty,
            span,
        },
        SurfacePattern::Record { fields, ty, span } => SurfacePattern::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, erase_binders(value)))
                .collect(),
            ty,
            span,
        },
        literal @ SurfacePattern::Literal { .. } | literal @ SurfacePattern::Any { .. } => literal,
    }
}

#[cfg(test)]
mod tests;
