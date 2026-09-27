//! Whole-function ABI decisions derived from the flattened parameter and result
//! counts, not from the WIT shape.
//!
//! A parameter tuple that flattens past `MAX_FLAT_PARAMS` is passed indirectly
//! through a pointer; a result that flattens past `MAX_FLAT_RESULTS` is written
//! through a return pointer. Both are length thresholds, so no shape needs a
//! per-case decision.

use super::{CanonicalType, CoreVal, flatten};
use wit_parser::{Function, Resolve};

/// The canonical ABI decisions for one function, with its flattened parameter
/// and result core values and the size of its indirect return area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FnAbi {
    pub flat_params: Vec<CoreVal>,
    pub flat_results: Vec<CoreVal>,
    pub indirect_params: bool,
    pub retptr: bool,
    /// The `(size, align)` of the indirect return area, when the result is an
    /// aggregate the lowering reads. `None` for a result read directly.
    pub result_area: Option<(u32, u32)>,
}

/// Derives the ABI decisions for `function`. Returns `None` when a parameter or
/// the result has no canonical representation.
pub(crate) fn function_abi(resolve: &Resolve, function: &Function) -> Option<FnAbi> {
    let mut params = Vec::new();
    for parameter in &function.params {
        params.push(super::resolve(resolve, &parameter.ty)?);
    }
    let result = match &function.result {
        Some(result) => Some(super::resolve(resolve, result)?),
        None => None,
    };
    Some(function_abi_from_types(&params, result.as_ref()))
}

/// Derives the ABI decisions from already-resolved canonical types.
pub(crate) fn function_abi_from_types(
    params: &[CanonicalType],
    result: Option<&CanonicalType>,
) -> FnAbi {
    let mut flat_params = Vec::new();
    for ty in params {
        flat_params.extend(flatten(ty));
    }
    let flat_results = result.map(flatten).unwrap_or_default();
    let indirect_params = flat_params.len() > Resolve::MAX_FLAT_PARAMS;
    let retptr = flat_results.len() > Resolve::MAX_FLAT_RESULTS;
    let result_area = result.and_then(crate::abi::layout::result_area);
    FnAbi {
        flat_params,
        flat_results,
        indirect_params,
        retptr,
        result_area,
    }
}
