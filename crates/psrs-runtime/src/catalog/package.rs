//! The produced runtime package and package-level consistency checks.

use super::{ALLOCATOR_UNIT, NUMBER_UNIT, RuntimePackage, VERSION};

/// The compiler-owned runtime package.
pub const PSRS_RUNTIME: RuntimePackage = RuntimePackage {
    id: "psrs:runtime",
    version: VERSION,
    provenance: "compiler-owned catalog; each unit pins its own variant bytes and recipe",
    units: &[&NUMBER_UNIT, &ALLOCATOR_UNIT],
    #[cfg(feature = "gc-storage")]
    encoded_units: &[&super::STORAGE_UNIT],
    #[cfg(not(feature = "gc-storage"))]
    encoded_units: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_number_unit_advertises_each_pinned_export_once() {
        let exports: Vec<_> = NUMBER_UNIT
            .variant
            .function_exports
            .iter()
            .map(|export| export.name)
            .collect();
        let provided: Vec<_> = NUMBER_UNIT
            .provided
            .iter()
            .map(|operation| operation.abi.export)
            .collect();
        assert_eq!(exports, provided);
        const { assert!(NUMBER_UNIT.required.is_empty()) };
        assert_eq!(NUMBER_UNIT.state.owner, Some(NUMBER_UNIT.id));
        const { assert!(!NUMBER_UNIT.state.grows_memory) };
        assert_eq!(PSRS_RUNTIME.units.len(), 2);
        assert_eq!(PSRS_RUNTIME.units[0].id, NUMBER_UNIT.id);
        assert_eq!(PSRS_RUNTIME.units[1].id, ALLOCATOR_UNIT.id);
        const { assert!(ALLOCATOR_UNIT.state.grows_memory) };
        assert_eq!(ALLOCATOR_UNIT.state.owner, Some(ALLOCATOR_UNIT.id));
        assert_eq!(ALLOCATOR_UNIT.provided.len(), 1);
        assert!(ALLOCATOR_UNIT.required.is_empty());
    }
}
