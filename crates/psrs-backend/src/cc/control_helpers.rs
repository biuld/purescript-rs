//! Stable semantic identities of predeclared control-machine helpers.
use super::SignatureId;
use psrs_hir::SymbolId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ControlHelperId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlHelperRole {
    Executor,
    CloneSequence,
    RebuildSequence,
    Start,
    Check,
    Close,
    Once,
    Cancel,
}

impl ControlHelperRole {
    pub(crate) const ALL: [Self; 8] = [
        Self::Executor,
        Self::CloneSequence,
        Self::RebuildSequence,
        Self::Start,
        Self::Check,
        Self::Close,
        Self::Once,
        Self::Cancel,
    ];
}

/// A checked call identifies a particular helper, even when two helpers have
/// the same physical signature. Only P8 planning publishes these bindings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlHelperContract {
    pub(super) role: ControlHelperRole,
    pub(super) function: SymbolId,
    pub(super) signature: SignatureId,
}

impl ControlHelperContract {
    pub fn role(&self) -> ControlHelperRole {
        self.role
    }
    pub fn function(&self) -> SymbolId {
        self.function
    }
    pub fn signature(&self) -> SignatureId {
        self.signature
    }
}

impl super::RepresentationTable {
    pub(crate) fn control_sequence(&self) -> Option<super::ReprId> {
        let helper = self
            .control_helpers
            .iter()
            .find(|helper| helper.role == ControlHelperRole::CloneSequence)?;
        let super::ValueShape::Reference(reference) =
            *self.signature(helper.signature)?.parameters.first()?
        else {
            return None;
        };
        let super::RefShape::Repr(sequence) = reference.heap else {
            return None;
        };
        Some(sequence)
    }

    pub(crate) fn verify_control_call(
        &self,
        function: SymbolId,
        contract: Option<ControlHelperId>,
    ) -> Result<(), &'static str> {
        match contract {
            Some(id) => {
                let helper = self
                    .control_helpers
                    .get(id.0 as usize)
                    .ok_or("control call has no predeclared helper contract")?;
                if helper.function != function {
                    return Err(
                        "control call target disagrees with its predeclared semantic helper",
                    );
                }
            }
            None if self
                .control_helpers
                .iter()
                .any(|helper| helper.function == function) =>
            {
                return Err("control helper call is missing its semantic contract");
            }
            None => {}
        }
        Ok(())
    }
}
