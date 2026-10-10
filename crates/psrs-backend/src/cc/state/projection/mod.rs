//! Shared logical call boundary for dependency verification and P9 projection.
use crate::cc::{RefShape, Reference, Representation, RepresentationTable, Signature, ValueShape};

mod conversion;
mod slots;
pub(crate) use slots::register_payload_slots;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateCallProjection {
    pub state_parameter: usize,
    pub state_field: usize,
    pub payload_field: usize,
    pub payload: ValueShape,
}

impl StateCallProjection {
    /// This describes slots only. Removing them still requires checked body,
    /// invocation and CFG correspondence; a signature does not prove ordering.
    pub fn checked(
        signature: &Signature,
        table: &RepresentationTable,
    ) -> Result<Option<Self>, &'static str> {
        let states = signature
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, shape)| (*shape == ValueShape::State).then_some(index))
            .collect::<Vec<_>>();
        if states.is_empty() {
            return Ok(None);
        }
        if states != [signature.parameters.len() - 1] {
            return Err("CC state invocation requires exactly one final dependency parameter");
        }
        let ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(id),
        }) = signature.result
        else {
            return Err("CC state invocation requires a non-null Step product");
        };
        let Some(Representation::Product { fields }) = table.representation(id) else {
            return Err("CC state invocation result is not a checked product");
        };
        if fields.len() != 2
            || fields
                .iter()
                .filter(|shape| **shape == ValueShape::State)
                .count()
                != 1
        {
            return Err("CC Step requires one successor and one payload field");
        }
        let state_field = fields
            .iter()
            .position(|shape| *shape == ValueShape::State)
            .unwrap();
        let payload_field = 1 - state_field;
        Ok(Some(Self {
            state_parameter: states[0],
            state_field,
            payload_field,
            payload: fields[payload_field],
        }))
    }

    pub fn physical_signature(
        signature: &Signature,
        table: &RepresentationTable,
    ) -> Result<Signature, &'static str> {
        let Some(plan) = Self::checked(signature, table)? else {
            return Ok(signature.clone());
        };
        Ok(Signature {
            parameters: signature.parameters[..plan.state_parameter].to_vec(),
            result: plan.payload,
        })
    }
}

#[cfg(test)]
mod tests;
