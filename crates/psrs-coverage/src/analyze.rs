//! Exhaustiveness, witness, and redundancy over one case's alternatives.
//!
//! The analysis owns semantic coverage facts. The type checker turns a missing
//! witness into the official `Partial` authorization flow; the decision
//! compiler turns the same report into first-match lowering and its rejection
//! messages. Generated trap fallbacks participate in coverage exactly as an
//! authorized `Partial` discharge; generated branch shapes are excluded from
//! diagnostics the official linter never sees.

use psrs_hir::CaseBranchCoverage;

use crate::engine::useful;
use crate::pattern::Pattern;
use crate::render::render;
use crate::types::Types;

/// One alternative entering analysis: its coverage class and its
/// binder-erased pattern.
#[derive(Clone, Debug)]
pub struct Branch<Ty> {
    pub coverage: CaseBranchCoverage,
    pub pattern: Pattern<Ty>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Analysis {
    pub exhaustive: bool,
    /// The missing binder atoms of the first uncovered row, in column order,
    /// or `None` when the alternatives cover all inputs.
    pub witness_atoms: Option<Vec<String>>,
    /// Alternative indexes matched by an earlier alternative.
    pub redundant_branches: Vec<usize>,
}

impl Analysis {
    /// The backend decision compiler's existing message: the first column of
    /// the witness.
    pub fn first_witness_atom(&self) -> Option<&str> {
        self.witness_atoms
            .as_deref()
            .and_then(|row| row.first())
            .map(String::as_str)
    }
}

pub fn analyze<Ty, T: Types<Ty>>(types: &T, scrutinee_type: Ty, branches: &[Branch<Ty>]) -> Analysis
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    if !branches.is_empty()
        && branches
            .iter()
            .all(|branch| branch.coverage == CaseBranchCoverage::Generated)
    {
        return Analysis {
            exhaustive: true,
            witness_atoms: None,
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
        .map(|branch| vec![branch.pattern.clone()])
        .collect::<Vec<Vec<Pattern<Ty>>>>();
    let query = vec![Pattern::Any { ty: scrutinee_type }];
    let witness_atoms = useful(types, &matrix, &query)
        .map(|row| row.iter().map(|pattern| render(types, pattern)).collect());
    let mut prior = Vec::new();
    let mut redundant_branches = Vec::new();
    for (index, branch) in branches.iter().enumerate() {
        if matches!(
            branch.coverage,
            CaseBranchCoverage::Generated | CaseBranchCoverage::PartialFallback
        ) {
            continue;
        }
        let query = vec![branch.pattern.clone()];
        if useful(types, &prior, &query).is_none() {
            redundant_branches.push(index);
        }
        if branch.coverage == CaseBranchCoverage::Source {
            prior.push(query);
        }
    }
    Analysis {
        exhaustive: witness_atoms.is_none(),
        witness_atoms,
        redundant_branches,
    }
}
