//! Target capability profiles shared by lowering and validation.
//!
//! A runtime may implement more proposals than a compiler target uses.  Keep
//! those two ideas separate: this profile describes what an artifact is
//! allowed to require, while the lowerings decide which capabilities a
//! particular module actually needs.

/// Capabilities of a WebAssembly/WASI compilation target.
///
/// The default profile is the project's stable Wasmtime/WASI 0.2 target.  The
/// fields are deliberately explicit so adding a proposal is a reviewed target
/// change rather than an accidental consequence of a runtime upgrade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetCapabilities {
    pub mutable_globals: bool,
    pub sign_extension: bool,
    pub nontrapping_float_to_int: bool,
    pub multi_value: bool,
    pub bulk_memory: bool,
    pub reference_types: bool,
    pub function_references: bool,
    pub gc: bool,
    pub simd: bool,
    pub relaxed_simd: bool,
    pub tail_call: bool,
    pub multi_memory: bool,
    pub memory64: bool,
    pub exceptions: bool,
    pub extended_const: bool,
    pub wide_arithmetic: bool,
    pub threads: bool,
    pub component_model: bool,
    pub component_async: bool,
    pub component_map: bool,
    pub component_implements: bool,
    pub wasi_p1: bool,
    pub wasi_p2: bool,
    pub wasi_p3: bool,
    pub wasi_cli: bool,
    pub wasi_io: bool,
    pub wasi_clocks: bool,
    pub wasi_random: bool,
    pub wasi_filesystem: bool,
    pub wasi_sockets: bool,
    pub wasi_http: bool,
    pub wasi_tls: bool,
}

impl TargetCapabilities {
    /// The stable target used by `compile`.
    ///
    /// GC and typed function references are required by the current runtime
    /// representation.  Other Tier 1 Wasmtime proposals remain opt-in until
    /// a lowering and an execution test actually use them.
    pub const fn wasmtime_wasi_0_2() -> Self {
        Self {
            mutable_globals: true,
            sign_extension: true,
            nontrapping_float_to_int: true,
            multi_value: true,
            bulk_memory: true,
            reference_types: true,
            function_references: true,
            gc: true,
            simd: false,
            relaxed_simd: false,
            tail_call: false,
            multi_memory: false,
            memory64: false,
            exceptions: false,
            extended_const: true,
            wide_arithmetic: false,
            threads: false,
            component_model: true,
            component_async: false,
            component_map: false,
            component_implements: false,
            wasi_p1: false,
            wasi_p2: true,
            wasi_p3: false,
            wasi_cli: true,
            wasi_io: true,
            wasi_clocks: true,
            wasi_random: true,
            wasi_filesystem: false,
            wasi_sockets: false,
            wasi_http: false,
            wasi_tls: false,
        }
    }

    /// A core-MVP profile useful for testing feature-independent lowerings.
    pub const fn wasm_mvp() -> Self {
        Self {
            mutable_globals: false,
            sign_extension: false,
            nontrapping_float_to_int: false,
            multi_value: false,
            bulk_memory: false,
            reference_types: false,
            function_references: false,
            gc: false,
            simd: false,
            relaxed_simd: false,
            tail_call: false,
            multi_memory: false,
            memory64: false,
            exceptions: false,
            extended_const: false,
            wide_arithmetic: false,
            threads: false,
            component_model: false,
            component_async: false,
            component_map: false,
            component_implements: false,
            wasi_p1: false,
            wasi_p2: false,
            wasi_p3: false,
            wasi_cli: false,
            wasi_io: false,
            wasi_clocks: false,
            wasi_random: false,
            wasi_filesystem: false,
            wasi_sockets: false,
            wasi_http: false,
            wasi_tls: false,
        }
    }

    /// Converts the profile to the validator feature set.
    pub(crate) fn wasm_features(self) -> wasmparser::WasmFeatures {
        use wasmparser::WasmFeatures;

        let mut features = WasmFeatures::MVP;
        for (feature, enabled) in [
            (WasmFeatures::MUTABLE_GLOBAL, self.mutable_globals),
            (WasmFeatures::SIGN_EXTENSION, self.sign_extension),
            (
                WasmFeatures::SATURATING_FLOAT_TO_INT,
                self.nontrapping_float_to_int,
            ),
            (WasmFeatures::MULTI_VALUE, self.multi_value),
            (WasmFeatures::BULK_MEMORY, self.bulk_memory),
            (WasmFeatures::REFERENCE_TYPES, self.reference_types),
            (WasmFeatures::FUNCTION_REFERENCES, self.function_references),
            (WasmFeatures::GC, self.gc),
            (WasmFeatures::SIMD, self.simd),
            (WasmFeatures::RELAXED_SIMD, self.relaxed_simd),
            (WasmFeatures::TAIL_CALL, self.tail_call),
            (WasmFeatures::MULTI_MEMORY, self.multi_memory),
            (WasmFeatures::MEMORY64, self.memory64),
            (WasmFeatures::EXCEPTIONS, self.exceptions),
            (WasmFeatures::EXTENDED_CONST, self.extended_const),
            (WasmFeatures::WIDE_ARITHMETIC, self.wide_arithmetic),
            (WasmFeatures::THREADS, self.threads),
            (WasmFeatures::COMPONENT_MODEL, self.component_model),
            (WasmFeatures::CM_ASYNC, self.component_async),
            (WasmFeatures::CM_MAP, self.component_map),
            // The current wasmparser release does not expose a separate
            // component-implements feature flag.
        ] {
            features.set(feature, enabled);
        }
        features
    }
}

impl Default for TargetCapabilities {
    fn default() -> Self {
        Self::wasmtime_wasi_0_2()
    }
}

#[cfg(test)]
mod tests {
    use super::TargetCapabilities;
    use wasmparser::WasmFeatures;

    #[test]
    fn stable_profile_enables_only_the_current_runtime_baseline() {
        let target = TargetCapabilities::default();
        let features = target.wasm_features();
        assert!(features.contains(WasmFeatures::GC));
        assert!(features.contains(WasmFeatures::FUNCTION_REFERENCES));
        assert!(features.contains(WasmFeatures::COMPONENT_MODEL));
        assert!(!features.contains(WasmFeatures::SIMD));
        assert!(!features.contains(WasmFeatures::TAIL_CALL));
        assert!(!features.contains(WasmFeatures::MEMORY64));
        assert!(!features.contains(WasmFeatures::CM_ASYNC));
    }

    #[test]
    fn mvp_profile_does_not_enable_proposals() {
        let features = TargetCapabilities::wasm_mvp().wasm_features();
        assert!(features.contains(WasmFeatures::MVP));
        assert!(!features.contains(WasmFeatures::GC));
        assert!(!features.contains(WasmFeatures::COMPONENT_MODEL));
    }
}
