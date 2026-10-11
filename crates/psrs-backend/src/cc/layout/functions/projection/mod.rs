//! Checked logical-to-physical calling signatures. This plans storage only;
//! expression lowering retains the calling convention of every invocation.
use super::super::layout_error;
use crate::BackendError;
use crate::cc::{Signature, ValueShape};
use psrs_core::{Module, TypeId};

pub(super) struct CallProjection<'a> {
    source: &'a Module,
    parameters: Vec<TypeId>,
    result: TypeId,
}

impl<'a> CallProjection<'a> {
    pub(super) fn checked(source: &'a Module, ty: TypeId) -> Result<Self, Vec<BackendError>> {
        let (parameters, result) = psrs_core::call_parts(&source.types, ty)
            .map_err(|message| layout_error(source.span, message))?;
        Ok(Self {
            source,
            parameters,
            result,
        })
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
        Ok(Signature { parameters, result })
    }
}
