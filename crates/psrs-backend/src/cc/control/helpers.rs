#![allow(dead_code)]
//! Stable semantic identities of predeclared control-machine helpers.
use crate::cc::SignatureId;
use psrs_hir::SymbolId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ControlHelperId(pub u32);

pub use crate::control::schema::ControlHelperRole;

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
        Some(self.control_schema()?.sequence)
    }

    pub(crate) fn verify_control_call(
        &self,
        function: SymbolId,
        contract: Option<ControlHelperId>,
    ) -> Result<(), &'static str> {
        match contract {
            Some(id) => {
                let helper = self
                    .control_helpers()
                    .get(id.0 as usize)
                    .ok_or("control call has no predeclared helper contract")?;
                if helper.function != function {
                    return Err(
                        "control call target disagrees with its predeclared semantic helper",
                    );
                }
            }
            None if self
                .control_helpers()
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
        let schema = self.control_schema()?;
        Some(ControlCheckpointSchema {
            computation: schema.computation,
            context: schema.context,
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
        let schema = self.control_schema()?;
        if !matches!(
            self.representation(schema.status),
            Some(crate::cc::Representation::Array {
                element: crate::cc::ValueShape::Integer
            })
        ) {
            return None;
        }
        Some(ControlReplaySchema {
            sequence: schema.sequence,
            boundary: schema.extent,
            status: schema.status,
            result: schema.replay_result,
        })
    }
}
