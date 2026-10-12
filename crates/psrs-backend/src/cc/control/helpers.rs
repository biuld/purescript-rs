//! Stable semantic identities of predeclared control-machine helpers.
use crate::cc::SignatureId;
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
    pub(in crate::cc) role: ControlHelperRole,
    pub(in crate::cc) function: SymbolId,
    pub(in crate::cc) signature: SignatureId,
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

impl crate::cc::RepresentationTable {
    pub(crate) fn control_sequence(&self) -> Option<crate::cc::ReprId> {
        let helper = self
            .control_helpers
            .iter()
            .find(|helper| helper.role == ControlHelperRole::CloneSequence)?;
        let crate::cc::ValueShape::Reference(reference) =
            *self.signature(helper.signature)?.parameters.first()?
        else {
            return None;
        };
        let crate::cc::RefShape::Repr(sequence) = reference.heap else {
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

/// The raw resumption code target is planned separately from direct-call helpers.
/// Its callable protocol excludes the hidden capture receiver parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlResumptionContract {
    pub(in crate::cc) function: SymbolId,
    pub(in crate::cc) signature: SignatureId,
}

impl ControlResumptionContract {
    pub fn function(&self) -> SymbolId {
        self.function
    }
    pub fn signature(&self) -> SignatureId {
        self.signature
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ControlCheckpointSchema {
    pub computation: crate::cc::ReprId,
    pub context: crate::cc::ReprId,
}

impl crate::cc::RepresentationTable {
    pub(crate) fn control_checkpoint_schema(&self) -> Option<ControlCheckpointSchema> {
        let parameter = |role, index| {
            let helper = self
                .control_helpers
                .iter()
                .find(|helper| helper.role() == role)?;
            let crate::cc::ValueShape::Reference(reference) =
                *self.signature(helper.signature())?.parameters.get(index)?
            else {
                return None;
            };
            let crate::cc::RefShape::Repr(repr) = reference.heap else {
                return None;
            };
            Some(repr)
        };
        Some(ControlCheckpointSchema {
            computation: parameter(ControlHelperRole::Executor, 2)?,
            context: parameter(ControlHelperRole::Executor, 5)?,
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ControlReplaySchema {
    pub sequence: crate::cc::ReprId,
    pub boundary: crate::cc::ReprId,
    pub status: crate::cc::ReprId,
    pub result: crate::cc::ReprId,
}

impl crate::cc::RepresentationTable {
    pub(crate) fn control_replay_schema(&self) -> Option<ControlReplaySchema> {
        let repr = |shape| match shape {
            crate::cc::ValueShape::Reference(crate::cc::Reference {
                heap: crate::cc::RefShape::Repr(id),
                nullable: false,
            }) => Some(id),
            _ => None,
        };
        let signature = |role| {
            self.signature(
                self.control_helpers
                    .iter()
                    .find(|helper| helper.role() == role)?
                    .signature(),
            )
        };
        let sequence = self.control_sequence()?;
        let boundary = repr(*signature(ControlHelperRole::Check)?.parameters.first()?)?;
        let crate::cc::Representation::Variant { cases } = self.representation(boundary)? else {
            return None;
        };
        let status = repr(*cases.iter().find(|case| case.tag == 1)?.fields.first()?)?;
        if !matches!(
            self.representation(status),
            Some(crate::cc::Representation::Array {
                element: crate::cc::ValueShape::Integer
            })
        ) {
            return None;
        }
        Some(ControlReplaySchema {
            sequence,
            boundary,
            status,
            result: repr(signature(ControlHelperRole::CloneSequence)?.result)?,
        })
    }
}
