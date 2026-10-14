//! The frozen placement of the named machine schema in one representation table.
//!
//! Planning fixes every machine carrier and private cell once, after the shared
//! [`crate::control::schema`] names and actually emitted helper signatures
//! agree. Consumers read named placements from here instead of recovering a
//! role from a helper parameter offset or a variant field position. The value
//! is immutable evidence: a later pass may read it but not replace it.

use super::{ControlHelperContract, ControlHelperRole};
use crate::cc::{ReprId, Representation};
use crate::control::schema::{ComputationCase, ContextCase, ExtentCase, OwnerCase, SequenceCase};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlSchema {
    /// The shared `Control f a` frame.
    pub computation: ReprId,
    /// The persistent sequencing spine.
    pub sequence: ReprId,
    /// The dispatch context.
    pub context: ReprId,
    /// A lifetime extent (the boundary carrier).
    pub extent: ReprId,
    /// The one-slot mutable status cell.
    pub status: ReprId,
    /// The one-shot owner object.
    pub owner: ReprId,
    /// The one-slot callback cell.
    pub callback: ReprId,
    /// The closed-run result pair.
    pub result_pair: ReprId,
    /// The replay result product.
    pub replay_result: ReprId,
}

impl crate::cc::RepresentationTable {
    /// The frozen machine placement, when this module uses control.
    pub(crate) fn control_schema(&self) -> Option<ControlSchema> {
        self.control_schema
    }

    /// The single checked contract for a semantic helper role.
    pub(crate) fn control_helper(&self, role: ControlHelperRole) -> Option<&ControlHelperContract> {
        self.control_helpers
            .iter()
            .find(|helper| helper.role() == role)
    }

    /// Check that every placed carrier has exactly the cases and field arity the
    /// named schema declares. This is the independent layout check: the physical
    /// frame is compared against the names, not one positional list against
    /// another. Missing placement with present helpers is an error, not a fresh
    /// unknown.
    pub(crate) fn verify_control_schema(&self) -> Result<(), &'static str> {
        let Some(schema) = self.control_schema else {
            return if self.control_helpers.is_empty() {
                Ok(())
            } else {
                Err("control helpers have no placed machine schema")
            };
        };
        check_variant(self, schema.computation, &computation_cases())?;
        check_variant(self, schema.sequence, &sequence_cases())?;
        check_variant(self, schema.context, &context_cases())?;
        check_variant(self, schema.extent, &extent_cases())?;
        check_variant(self, schema.owner, &owner_cases())?;
        check_product(self, schema.result_pair)?;
        check_product(self, schema.replay_result)?;
        for repr in [schema.status, schema.callback] {
            if !matches!(
                self.representation(repr),
                Some(Representation::Array { .. })
            ) {
                return Err("control placement names a private cell that is not an array");
            }
        }
        Ok(())
    }
}

fn computation_cases() -> Vec<(u32, usize)> {
    ComputationCase::ALL
        .iter()
        .map(|case| (case.tag(), case.field_count()))
        .collect()
}

fn sequence_cases() -> Vec<(u32, usize)> {
    SequenceCase::ALL
        .iter()
        .map(|case| (case.tag(), case.field_count()))
        .collect()
}

fn context_cases() -> Vec<(u32, usize)> {
    ContextCase::ALL
        .iter()
        .map(|case| (case.tag(), case.field_count()))
        .collect()
}

fn extent_cases() -> Vec<(u32, usize)> {
    ExtentCase::ALL
        .iter()
        .map(|case| (case.tag(), case.field_count()))
        .collect()
}

fn owner_cases() -> Vec<(u32, usize)> {
    OwnerCase::ALL
        .iter()
        .map(|case| (case.tag(), case.field_count()))
        .collect()
}

fn check_variant(
    table: &crate::cc::RepresentationTable,
    repr: ReprId,
    expected: &[(u32, usize)],
) -> Result<(), &'static str> {
    let Some(Representation::Variant { cases }) = table.representation(repr) else {
        return Err("control placement names a carrier that is not a variant");
    };
    if cases.len() != expected.len() {
        return Err("control carrier does not match the named schema case count");
    }
    for (case, (tag, count)) in cases.iter().zip(expected) {
        if case.tag != *tag || case.fields.len() != *count {
            return Err("control carrier does not match the named schema field arity");
        }
    }
    Ok(())
}

fn check_product(table: &crate::cc::RepresentationTable, repr: ReprId) -> Result<(), &'static str> {
    match table.representation(repr) {
        Some(Representation::Product { fields }) if fields.len() == 2 => Ok(()),
        _ => Err("control placement names a product that does not have two fields"),
    }
}
