//! Whole-function ABI decisions derived from the flattened parameter and result
//! counts, not from the WIT shape.
//!
//! A parameter tuple that flattens past `MAX_FLAT_PARAMS` is passed indirectly
//! through a pointer; a result that flattens past `MAX_FLAT_RESULTS` is written
//! through a return pointer. Both are length thresholds, so no shape needs a
//! per-case decision.

use super::{CoreVal, flatten};
use wit_parser::{Function, Resolve};

/// The canonical ABI decisions for one function, with its flattened parameter
/// and result core values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FnAbi {
    pub flat_params: Vec<CoreVal>,
    pub flat_results: Vec<CoreVal>,
    pub indirect_params: bool,
    pub retptr: bool,
}

/// Derives the ABI decisions for `function`. Returns `None` when a parameter or
/// the result has no canonical representation.
pub(crate) fn function_abi(resolve: &Resolve, function: &Function) -> Option<FnAbi> {
    let mut flat_params = Vec::new();
    for parameter in &function.params {
        let ty = super::resolve(resolve, &parameter.ty)?;
        flat_params.extend(flatten(&ty));
    }
    let flat_results = match &function.result {
        Some(result) => flatten(&super::resolve(resolve, result)?),
        None => Vec::new(),
    };
    let indirect_params = flat_params.len() > Resolve::MAX_FLAT_PARAMS;
    let retptr = flat_results.len() > Resolve::MAX_FLAT_RESULTS;
    Some(FnAbi {
        flat_params,
        flat_results,
        indirect_params,
        retptr,
    })
}
