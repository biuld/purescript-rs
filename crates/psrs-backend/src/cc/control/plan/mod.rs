//! Mutable source planning has one transition to an immutable machine contract.
use super::schema::ControlSchema;
use super::{ControlContract, ControlHelperContract, ControlHelperRole, ControlResumptionContract};
use crate::cc::{ReprId, RepresentationTable};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(in crate::cc) struct ControlPlanning {
    contracts: Vec<ControlContract>,
    identities: Vec<super::super::TypeIdentity>,
    frozen: Option<FrozenControlPlan>,
}

/// P8's published machine metadata. Instructions and layouts remain ordinary
/// mutable IR; consumers cannot modify or replace the semantic bindings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrozenControlPlan {
    contracts: Box<[ControlContract]>,
    identities: Box<[super::super::TypeIdentity]>,
    helpers: Box<[ControlHelperContract]>,
    roles: HashMap<ControlHelperRole, usize>,
    resumption: ControlResumptionContract,
    schema: ControlSchema,
}

impl FrozenControlPlan {
    pub fn source_type_identities(&self) -> &[super::super::TypeIdentity] {
        &self.identities
    }
    pub fn contracts(&self) -> &[ControlContract] {
        &self.contracts
    }
    pub fn helpers(&self) -> &[ControlHelperContract] {
        &self.helpers
    }
    pub fn helper(&self, role: ControlHelperRole) -> Option<&ControlHelperContract> {
        self.roles
            .get(&role)
            .and_then(|index| self.helpers.get(*index))
    }
    pub fn resumption(&self) -> &ControlResumptionContract {
        &self.resumption
    }
    pub fn result_pair(&self) -> ReprId {
        self.schema.result_pair
    }
    pub(in crate::cc) fn schema(&self) -> ControlSchema {
        self.schema
    }
}

impl RepresentationTable {
    pub fn source_type_identities(&self) -> &[super::super::TypeIdentity] {
        self.control_plan().map_or(
            &self.control.identities,
            FrozenControlPlan::source_type_identities,
        )
    }

    pub(in crate::cc) fn plan_control_identity(
        &mut self,
        identity: super::super::TypeIdentity,
    ) -> super::super::SourceTypeIdentityId {
        assert!(
            self.control.frozen.is_none(),
            "source type identities cannot change after control planning freezes"
        );
        let id = super::super::SourceTypeIdentityId(self.control.identities.len() as u32);
        self.control.identities.push(identity);
        id
    }
    pub fn control_plan(&self) -> Option<&FrozenControlPlan> {
        self.control.frozen.as_ref()
    }
    pub fn control_helpers(&self) -> &[ControlHelperContract] {
        self.control_plan().map_or(&[], FrozenControlPlan::helpers)
    }
    pub fn control_contracts(&self) -> &[ControlContract] {
        self.control_plan()
            .map_or(&self.control.contracts, FrozenControlPlan::contracts)
    }
    pub fn control_resumption(&self) -> Option<&ControlResumptionContract> {
        self.control_plan().map(FrozenControlPlan::resumption)
    }
    pub fn control_result_pair(&self) -> Option<ReprId> {
        self.control_plan().map(FrozenControlPlan::result_pair)
    }

    pub(in crate::cc) fn plan_control_contract(&mut self, contract: ControlContract) {
        assert!(
            self.control.frozen.is_none(),
            "source contracts cannot change after control planning freezes"
        );
        if !self.control.contracts.contains(&contract) {
            self.control.contracts.push(contract);
        }
    }

    pub(in crate::cc) fn freeze_control_plan(
        &mut self,
        helpers: Vec<ControlHelperContract>,
        resumption: ControlResumptionContract,
        schema: ControlSchema,
    ) -> Result<(), &'static str> {
        if self.control.frozen.is_some() {
            return Err("control machine plan is already frozen");
        }
        let mut roles = HashMap::new();
        let mut symbols = std::collections::HashSet::from([resumption.function()]);
        for (index, helper) in helpers.iter().enumerate() {
            if !symbols.insert(helper.function()) {
                return Err("control machine planning repeats a callable identity");
            }
            if roles.insert(helper.role(), index).is_some() {
                return Err("control machine planning repeats a helper role");
            }
        }
        if ControlHelperRole::ALL
            .iter()
            .any(|role| !roles.contains_key(role))
        {
            return Err("control machine planning omits a helper role");
        }
        let contracts = std::mem::take(&mut self.control.contracts).into_boxed_slice();
        let identities = std::mem::take(&mut self.control.identities).into_boxed_slice();
        self.control.frozen = Some(FrozenControlPlan {
            contracts,
            identities,
            helpers: helpers.into_boxed_slice(),
            roles,
            resumption,
            schema,
        });
        if let Err(error) = self.verify_control_schema() {
            let candidate = self.control.frozen.take().expect("candidate plan exists");
            self.control.contracts = candidate.contracts.into_vec();
            self.control.identities = candidate.identities.into_vec();
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
