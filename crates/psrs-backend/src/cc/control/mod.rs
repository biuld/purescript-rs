//! Checked source contracts and semantic identities for the control machine.

mod contracts;
mod helpers;

pub(in crate::cc) use contracts::ControlImplementation;
pub use contracts::{
    ControlContract, ControlContractId, ControlDomain, SourceTypeIdentityId,
    TypeConstructorIdentity, TypeIdentity,
};
pub use helpers::{
    ControlHelperContract, ControlHelperId, ControlHelperRole, ControlResumptionContract,
};
