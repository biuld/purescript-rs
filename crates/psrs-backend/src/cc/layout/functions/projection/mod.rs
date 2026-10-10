//! Checked logical-to-physical calling signatures. This plans storage only;
//! expression lowering must retain the state dependency of every invocation.
use super::super::layout_error;
use crate::BackendError;
use crate::cc::{Signature, ValueShape};
use psrs_core::{Module, TypeId, state::StateSignature};

pub(super) struct CallProjection<'a> {
    source: &'a Module,
    parameters: Vec<TypeId>,
    result: TypeId,
    logical_result: TypeId,
    state: Option<StateSignature>,
}

impl<'a> CallProjection<'a> {
    pub(super) fn checked(source: &'a Module, ty: TypeId) -> Result<Self, Vec<BackendError>> {
        let (parameters, result) = psrs_core::call_parts(&source.types, ty)
            .map_err(|message| layout_error(source.span, message))?;
        let has_state = parameters
            .iter()
            .any(|parameter| psrs_core::state::region(source, *parameter).is_some());
        if has_state {
            let state = psrs_core::state::signature(source, ty)
                .map_err(|message| layout_error(source.span, message))?;
            return Ok(Self {
                source,
                parameters: state.parameters.clone(),
                result: state.payload,
                logical_result: result,
                state: Some(state),
            });
        }
        Ok(Self {
            source,
            parameters,
            result,
            logical_result: result,
            state: None,
        })
    }

    pub(super) fn state_contract(&self) -> Option<&StateSignature> {
        self.state.as_ref()
    }

    pub(super) fn logical_result(&self) -> TypeId {
        self.logical_result
    }

    /// Only payload types reach the physical layout callback. The source arena
    /// is retained explicitly so the projected TypeIds cannot be interpreted in
    /// a rewritten physical arena. An empty parameter list is a real nullary
    /// call, not permission to evaluate the action at initialization time.
    pub(super) fn physical_signature(
        &self,
        mut shape: impl FnMut(&Module, TypeId) -> Result<ValueShape, Vec<BackendError>>,
    ) -> Result<Signature, Vec<BackendError>> {
        let parameters = self
            .parameters
            .iter()
            .map(|ty| shape(self.source, *ty))
            .collect::<Result<Vec<_>, _>>()?;
        let result = shape(self.source, self.result)?;
        if parameters.contains(&ValueShape::State) || result == ValueShape::State {
            return Err(layout_error(
                self.source.span,
                "nested state payloads require checked zero-width result projection",
            ));
        }
        Ok(Signature { parameters, result })
    }
}

#[cfg(test)]
mod tests;
