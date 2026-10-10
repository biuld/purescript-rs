//! Canonical terminal payload slots; planning registers them before lowering.
use super::*;
use crate::cc::{ReprId, SignatureId};

type StepRequirement = (Representation, Option<Vec<String>>);

impl StateCallProjection {
    fn slot_requirement(
        &self,
        signature: &Signature,
        table: &RepresentationTable,
    ) -> Result<StepRequirement, &'static str> {
        let ValueShape::Reference(Reference {
            heap: RefShape::Repr(source),
            ..
        }) = signature.result
        else {
            return Err("State payload slot has no checked Step representation");
        };
        let mut fields = vec![crate::cc::payload::erased_shape(); 2];
        fields[self.state_field] = ValueShape::State;
        Ok((
            Representation::Product { fields },
            table.product_labels(source).map(<[String]>::to_vec),
        ))
    }

    pub(crate) fn payload_slot_signature(
        &self,
        signature: &Signature,
        table: &RepresentationTable,
    ) -> Result<SignatureId, &'static str> {
        let (requirement, labels) = self.slot_requirement(signature, table)?;
        let target = table
            .representations
            .iter()
            .enumerate()
            .find(|(index, value)| {
                **value == requirement
                    && table.product_labels(ReprId(*index as u32)) == labels.as_deref()
            })
            .map(|(index, _)| ReprId(index as u32))
            .ok_or("State payload slot has no registered Step protocol")?;
        let signature = Signature {
            parameters: vec![ValueShape::State],
            result: ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(target),
            }),
        };
        table
            .signatures
            .iter()
            .position(|value| *value == signature)
            .map(|index| SignatureId(index as u32))
            .ok_or("State payload slot has no registered callable protocol")
    }
}

pub(crate) fn register_payload_slots(table: &mut RepresentationTable) {
    for signature in table.signatures.clone() {
        let Ok(Some(projection)) = StateCallProjection::checked(&signature, table) else {
            continue;
        };
        let (requirement, labels) = projection
            .slot_requirement(&signature, table)
            .expect("checked Step has a representation");
        let target = table
            .representations
            .iter()
            .enumerate()
            .find(|(index, value)| {
                **value == requirement
                    && table.product_labels(ReprId(*index as u32)) == labels.as_deref()
            })
            .map(|(index, _)| ReprId(index as u32));
        let target = target.unwrap_or_else(|| {
            let id = table.reserve();
            table.set(id, requirement);
            if let Some(labels) = labels {
                table.set_product_labels(id, labels);
            }
            id
        });
        let slot = Signature {
            parameters: vec![ValueShape::State],
            result: ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(target),
            }),
        };
        if !table.signatures.contains(&slot) {
            table.add_signature(slot);
        }
    }
}
