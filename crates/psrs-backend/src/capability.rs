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
    /// Deferred: the pinned component toolchain cannot lift 64-bit memories and
    /// the WASI host path is incomplete, so enabling it would break the WASI
    /// artifact. See `docs/design/backend/wasm/capability-profile.md`.
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
    fn stable_profile_matches_the_documented_wasi_0_2_target() {
        let expected = TargetCapabilities {
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
        };

        assert_eq!(TargetCapabilities::wasmtime_wasi_0_2(), expected);
        assert_eq!(TargetCapabilities::default(), expected);
    }

    #[test]
    fn wasm_feature_flags_follow_independent_profile_fields() {
        let disabled = disabled_target();
        let baseline = disabled.wasm_features();
        assert_eq!(baseline, WasmFeatures::MVP);

        macro_rules! assert_independent {
            ($field:ident, $feature:expr) => {{
                let mut target = disabled;
                target.$field = true;
                assert_eq!(
                    target.wasm_features().bits() ^ baseline.bits(),
                    $feature.bits(),
                    "{} must control only its matching wasmparser feature",
                    stringify!($field)
                );
            }};
        }

        assert_independent!(mutable_globals, WasmFeatures::MUTABLE_GLOBAL);
        assert_independent!(sign_extension, WasmFeatures::SIGN_EXTENSION);
        assert_independent!(
            nontrapping_float_to_int,
            WasmFeatures::SATURATING_FLOAT_TO_INT
        );
        assert_independent!(multi_value, WasmFeatures::MULTI_VALUE);
        assert_independent!(bulk_memory, WasmFeatures::BULK_MEMORY);
        assert_independent!(reference_types, WasmFeatures::REFERENCE_TYPES);
        assert_independent!(function_references, WasmFeatures::FUNCTION_REFERENCES);
        assert_independent!(gc, WasmFeatures::GC);
        assert_independent!(simd, WasmFeatures::SIMD);
        assert_independent!(relaxed_simd, WasmFeatures::RELAXED_SIMD);
        assert_independent!(tail_call, WasmFeatures::TAIL_CALL);
        assert_independent!(multi_memory, WasmFeatures::MULTI_MEMORY);
        assert_independent!(memory64, WasmFeatures::MEMORY64);
        assert_independent!(exceptions, WasmFeatures::EXCEPTIONS);
        assert_independent!(extended_const, WasmFeatures::EXTENDED_CONST);
        assert_independent!(wide_arithmetic, WasmFeatures::WIDE_ARITHMETIC);
        assert_independent!(threads, WasmFeatures::THREADS);
        assert_independent!(component_model, WasmFeatures::COMPONENT_MODEL);
        assert_independent!(component_async, WasmFeatures::CM_ASYNC);
        assert_independent!(component_map, WasmFeatures::CM_MAP);

        macro_rules! has_no_wasmparser_feature {
            ($field:ident) => {{
                let mut target = disabled;
                target.$field = true;
                assert_eq!(target.wasm_features(), baseline);
            }};
        }

        has_no_wasmparser_feature!(component_implements);
        has_no_wasmparser_feature!(wasi_p1);
        has_no_wasmparser_feature!(wasi_p2);
        has_no_wasmparser_feature!(wasi_p3);
        has_no_wasmparser_feature!(wasi_cli);
        has_no_wasmparser_feature!(wasi_io);
        has_no_wasmparser_feature!(wasi_clocks);
        has_no_wasmparser_feature!(wasi_random);
        has_no_wasmparser_feature!(wasi_filesystem);
        has_no_wasmparser_feature!(wasi_sockets);
        has_no_wasmparser_feature!(wasi_http);
        has_no_wasmparser_feature!(wasi_tls);
    }

    #[test]
    fn target_validator_rejects_simd_when_its_gate_is_disabled() {
        use wasm_encoder::{
            CodeSection, Function, FunctionSection, Instruction, Module, TypeSection,
        };

        let mut types = TypeSection::new();
        types.ty().function([], [wasm_encoder::ValType::V128]);
        let mut functions = FunctionSection::new();
        functions.function(0);
        let mut body = Function::new([]);
        body.instruction(&Instruction::V128Const(0));
        body.instruction(&Instruction::End);
        let mut code = CodeSection::new();
        code.function(&body);
        let mut module = Module::new();
        module.section(&types);
        module.section(&functions);
        module.section(&code);
        let bytes = module.finish();

        let target = TargetCapabilities {
            simd: false,
            ..TargetCapabilities::default()
        };
        assert!(crate::validator_for(target).validate_all(&bytes).is_err());

        let target = TargetCapabilities {
            simd: true,
            ..target
        };
        crate::validator_for(target)
            .validate_all(&bytes)
            .expect("enabling SIMD should admit the SIMD module");
    }

    fn disabled_target() -> TargetCapabilities {
        TargetCapabilities {
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
}
