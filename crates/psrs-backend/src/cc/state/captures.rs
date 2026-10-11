//! A captured dependency is supplied by the closure producer, never invented
//! by the lifted body. Capture slots come from ordinary CC layout verification.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapturedCallProjection {
    pub capture_index: usize,
    pub step: StateResultProjection,
}

impl CapturedCallProjection {
    pub(crate) fn checked(
        module: &Module,
        function: &Function,
        signature: &Signature,
    ) -> Result<Option<Self>, Vec<BackendError>> {
        let captures = super::super::verify::capture_shapes(function)?;
        let states = captures
            .iter()
            .enumerate()
            .filter_map(|(index, shape)| (*shape == ValueShape::State).then_some(index))
            .collect::<Vec<_>>();
        if states.is_empty() {
            return Ok(None);
        }
        if states.len() != 1 || signature.parameters.contains(&ValueShape::State) {
            return Err(error(
                function,
                "captured State requires one dependency source",
            ));
        }
        let step = StateResultProjection::checked(signature.result, &module.representations)
            .map_err(|message| error(function, message))?;
        Ok(Some(Self {
            capture_index: states[0],
            step,
        }))
    }
}
