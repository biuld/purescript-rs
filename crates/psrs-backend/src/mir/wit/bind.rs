//! The import binding used by the canonical ABI lowering: each declared
//! parameter and the optional result is paired with the canonical type, the
//! abstract guest shape, and the instance-aware projection.

use super::WitCallLowerer;
use crate::abi::WasiImport;
use crate::abi::canonical::CanonicalType;
use crate::cc::GuestLayout;

/// One canonical ABI value paired with the guest projection of the declaration
/// position it lowers. Canonical drives flattening, layout, and the return area;
/// the projection resolves each field's concrete value and storage slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoundType {
    pub canonical: CanonicalType,
    /// The abstract guest shape, used as a fallback when no instance-aware
    /// projection is supplied (a hand-built MIR test module).
    pub guest: crate::cc::ValueShape,
    /// The instance-aware projection, when CC supplied one.
    pub projection: Option<GuestLayout>,
}

impl BoundType {
    /// The projection when present, otherwise the storage layout resolved from
    /// the abstract shape through the representation table.
    pub(super) fn guest_layout<L: WitCallLowerer>(&self, lowerer: &L) -> Option<GuestLayout> {
        self.projection
            .clone()
            .or_else(|| lowerer.wit_guest_layout(self.guest))
    }
}

/// Every parameter and the optional result of one resolved WIT import, each
/// paired with its guest shape and projection. Built once per call so the
/// lowering never searches the declaration again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoundFn {
    pub parameters: Vec<BoundType>,
    /// The declaration's full abstract parameter list. A primitive import may
    /// have a different source arity than the WIT parameter count, so the
    /// primitive lowering reads these rather than the zipped `parameters`.
    pub guest_parameters: Vec<crate::cc::ValueShape>,
    pub result: Option<BoundType>,
}

impl BoundFn {
    /// Pairs the import's canonical parameters and result with the declaration's
    /// abstract guest shapes and its instance-aware projection.
    pub(super) fn bind(
        import: &WasiImport,
        signature: &crate::cc::Signature,
        projection: Option<&crate::cc::ExternalProjection>,
    ) -> Self {
        let projected = projection.map(|projection| projection.parameters.as_slice());
        let parameters = import
            .params
            .iter()
            .cloned()
            .zip(signature.parameters.iter().copied())
            .enumerate()
            .map(|(index, (canonical, guest))| BoundType {
                canonical,
                guest,
                projection: projected
                    .and_then(|projected| projected.get(index))
                    .cloned(),
            })
            .collect();
        let result = import.canonical_result.clone().map(|canonical| BoundType {
            canonical,
            guest: signature.result,
            projection: projection.and_then(|projection| projection.result.clone()),
        });
        BoundFn {
            parameters,
            guest_parameters: signature.parameters.clone(),
            result,
        }
    }
}
