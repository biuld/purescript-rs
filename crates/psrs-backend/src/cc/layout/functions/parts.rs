//! Checked source call boundaries shared by layout and newtype protocols.
use super::super::layout_error;
use crate::BackendError;
use psrs_core::{Module as CoreModule, TypeId};

/// Derives the calling convention of a value.
///
/// A closure contributes exactly the parameter list it was born with. Its
/// result stays a value, even when that value is a function or another
/// closure. An ordinary function flattens every arrow: the parameters are the
/// arrow domains and the result is the codomain. A quantifier in a codomain
/// stops the walk so that polymorphic result stays a separate closure.
pub(crate) fn function_arrow_parameters(module: &CoreModule, id: TypeId) -> (Vec<TypeId>, TypeId) {
    checked_function_arrow_parameters(module, id)
        .expect("P8 layout validated live callable signatures before expression lowering")
}

/// Checks the complete call boundary before registering a physical signature.
/// A non-callable value has zero parameters; dangling/cyclic spines are errors.
pub(crate) fn checked_function_arrow_parameters(
    module: &CoreModule,
    id: TypeId,
) -> Result<(Vec<TypeId>, TypeId), Vec<BackendError>> {
    psrs_core::call_parts(&module.types, id).map_err(|message| layout_error(module.span, message))
}
