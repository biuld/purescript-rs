//! The frozen placement of the named machine schema in one representation table.
//!
//! Planning fixes every machine carrier and private cell once, after the shared
//! [`crate::control::schema`] names and actually emitted helper signatures
//! agree. Consumers read named placements from here instead of recovering a
//! role from a helper parameter offset or a variant field position. The value
//! is immutable evidence: a later pass may read it but not replace it.

use super::{ControlHelperContract, ControlHelperRole};
use crate::cc::{RefShape, Reference, ReprId, Representation, SignatureId, ValueShape};
use crate::control::schema::FieldKind;
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

impl ControlSchema {
    /// Resolve one logical storage role through this module's frozen placement.
    pub(crate) fn shape(self, kind: FieldKind, continuation: SignatureId) -> ValueShape {
        use FieldKind::*;
        let heap = match kind {
            Payload => return crate::cc::payload::erased_shape(),
            Callable => RefShape::Closure(continuation),
            Computation => RefShape::Repr(self.computation),
            Sequence => RefShape::Repr(self.sequence),
            Context => RefShape::Repr(self.context),
            Extent => RefShape::Repr(self.extent),
            Status => RefShape::Repr(self.status),
            Owner => RefShape::Repr(self.owner),
            Callback => RefShape::Repr(self.callback),
            ResultPair => RefShape::Repr(self.result_pair),
            ReplayResult => RefShape::Repr(self.replay_result),
        };
        ValueShape::Reference(Reference {
            nullable: false,
            heap,
        })
    }
}

impl crate::cc::RepresentationTable {
    /// The frozen machine placement, when this module uses control.
    pub(crate) fn control_schema(&self) -> Option<ControlSchema> {
        self.control_plan().map(|plan| plan.schema())
    }

    /// The single checked contract for a semantic helper role.
    pub(crate) fn control_helper(&self, role: ControlHelperRole) -> Option<&ControlHelperContract> {
        self.control_plan()?.helper(role)
    }

    /// Validate the complete logical storage contract against physical CC shapes.
    pub(crate) fn verify_control_schema(&self) -> Result<(), &'static str> {
        let Some(schema) = self.control_schema() else {
            return if self.control_helpers().is_empty() {
                Ok(())
            } else {
                Err("control helpers have no placed machine schema")
            };
        };
        let continuation = self
            .control_resumption()
            .ok_or("control placement has no raw continuation signature")?
            .signature();
        let shape = |kind| schema.shape(kind, continuation);
        let mut identities = std::collections::HashSet::new();
        for repr in [
            schema.computation,
            schema.sequence,
            schema.context,
            schema.extent,
            schema.status,
            schema.owner,
            schema.callback,
            schema.result_pair,
            schema.replay_result,
        ] {
            if !identities.insert(repr) {
                return Err("control placement aliases distinct storage roles");
            }
        }
        for (repr, expected) in [
            (
                schema.computation,
                ComputationCase::ALL
                    .iter()
                    .map(|case| (case.tag(), case.fields()))
                    .collect::<Vec<_>>(),
            ),
            (
                schema.sequence,
                SequenceCase::ALL
                    .iter()
                    .map(|case| (case.tag(), case.fields()))
                    .collect(),
            ),
            (
                schema.context,
                ContextCase::ALL
                    .iter()
                    .map(|case| (case.tag(), case.fields()))
                    .collect(),
            ),
            (
                schema.extent,
                ExtentCase::ALL
                    .iter()
                    .map(|case| (case.tag(), case.fields()))
                    .collect(),
            ),
            (
                schema.owner,
                OwnerCase::ALL
                    .iter()
                    .map(|case| (case.tag(), case.fields()))
                    .collect(),
            ),
        ] {
            let Some(Representation::Variant { cases }) = self.representation(repr) else {
                return Err("control placement names a carrier that is not a variant");
            };
            if cases.len() != expected.len() {
                return Err("control carrier does not match the named schema case count");
            }
            for (actual, (tag, fields)) in cases.iter().zip(expected) {
                let expected = fields.iter().map(|kind| shape(*kind)).collect::<Vec<_>>();
                if actual.tag != tag || actual.fields != expected {
                    return Err("control carrier fields disagree with their logical storage roles");
                }
            }
        }
        for (repr, fields) in [
            (schema.result_pair, vec![shape(FieldKind::Payload); 2]),
            (
                schema.replay_result,
                vec![shape(FieldKind::Sequence), shape(FieldKind::Extent)],
            ),
        ] {
            if self.representation(repr) != Some(&Representation::Product { fields }) {
                return Err("control product fields disagree with their logical storage roles");
            }
        }
        for (repr, element) in [
            (schema.status, ValueShape::Integer),
            (schema.callback, shape(FieldKind::Payload)),
        ] {
            if self.representation(repr) != Some(&Representation::Array { element }) {
                return Err("control private cell has the wrong element role");
            }
        }
        if self.control_result_pair() != Some(schema.result_pair) {
            return Err("control result pair disagrees with its placed role");
        }
        for helper in self.control_helpers() {
            let signature = self
                .signature(helper.signature())
                .ok_or("control helper has no planned signature")?;
            let parameters = helper
                .role()
                .parameters()
                .iter()
                .map(|kind| shape(*kind))
                .collect::<Vec<_>>();
            if signature.parameters != parameters
                || signature.result != shape(helper.role().result())
            {
                return Err("control helper signature disagrees with its logical storage roles");
            }
        }
        let raw = self
            .signature(continuation)
            .ok_or("raw continuation signature is missing")?;
        if raw.parameters != [shape(FieldKind::Payload)] || raw.result != shape(FieldKind::Payload)
        {
            return Err("raw continuation signature disagrees with its unary protocol");
        }
        Ok(())
    }
}
