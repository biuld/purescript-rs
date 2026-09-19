pub mod abi;
pub mod cc;
pub mod component;
pub mod mir;
pub mod types;
pub mod wasm;

use psrs_hir::ModuleId;
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendError {
    pub pass: &'static str,
    pub span: TextRange,
    pub message: String,
    /// The source module that owns the failing declaration, when the lowering
    /// pass can identify one. Module IDs are preserved from the frontend so a
    /// multi-module driver can report backend failures against the right file.
    pub module: Option<ModuleId>,
}

impl BackendError {
    fn new(pass: &'static str, span: TextRange, message: impl Into<String>) -> Self {
        Self {
            pass,
            span,
            message: message.into(),
            module: None,
        }
    }

    pub(crate) fn with_module(mut self, module: ModuleId) -> Self {
        if self.module.is_none() {
            self.module = Some(module);
        }
        self
    }
}

pub(crate) fn annotate_errors(
    errors: Vec<BackendError>,
    module: Option<ModuleId>,
) -> Vec<BackendError> {
    errors
        .into_iter()
        .map(|error| match module {
            Some(module) => error.with_module(module),
            None => error,
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub wasm: Vec<u8>,
    pub wat: String,
}

pub fn compile(module: psrs_core::Module) -> Result<Artifact, Vec<BackendError>> {
    Ok(compile_with_stages(module)?.artifact)
}

/// The validator configured for the feature profile in
/// `docs/design/D-05-backend-capability.md`. It starts from the `wasmparser`
/// defaults (which currently match the pinned `wasmtime` baseline) and then
/// pins the features the backend depends on and the opt-in preview proposals
/// the profile excludes, so a change in defaults cannot silently disable GC or
/// the component model.
pub(crate) fn validator() -> wasmparser::Validator {
    use wasmparser::WasmFeatures;
    let mut features = WasmFeatures::default();
    for feature in [
        WasmFeatures::REFERENCE_TYPES,
        WasmFeatures::FUNCTION_REFERENCES,
        WasmFeatures::GC,
        WasmFeatures::GC_TYPES,
        WasmFeatures::MULTI_VALUE,
        WasmFeatures::TAIL_CALL,
        WasmFeatures::EXCEPTIONS,
        WasmFeatures::MULTI_MEMORY,
        WasmFeatures::MEMORY64,
        WasmFeatures::SIMD,
        WasmFeatures::RELAXED_SIMD,
        WasmFeatures::THREADS,
        WasmFeatures::BULK_MEMORY,
        WasmFeatures::EXTENDED_CONST,
        WasmFeatures::COMPONENT_MODEL,
    ] {
        features.set(feature, true);
    }
    for feature in [
        WasmFeatures::SHARED_EVERYTHING_THREADS,
        WasmFeatures::MEMORY_CONTROL,
        WasmFeatures::CUSTOM_PAGE_SIZES,
        WasmFeatures::STACK_SWITCHING,
        WasmFeatures::WIDE_ARITHMETIC,
        WasmFeatures::LEGACY_EXCEPTIONS,
        WasmFeatures::CUSTOM_DESCRIPTORS,
        WasmFeatures::COMPACT_IMPORTS,
    ] {
        features.set(feature, false);
    }
    wasmparser::Validator::new_with_features(features)
}

#[derive(Clone, Debug)]
pub struct Stages {
    pub cc: cc::Module,
    pub mir: mir::Module,
    pub wasm: wasm::Module,
    pub artifact: Artifact,
}

pub fn compile_with_stages(module: psrs_core::Module) -> Result<Stages, Vec<BackendError>> {
    let cc = cc::lower_module(module)?;
    let (mir, mut wasi) = mir::lower_module(cc.clone())?;
    let owner = mir.entry.map(|entry| entry.module);
    let wasm =
        wasm::lower_module(&mir, &mut wasi).map_err(|errors| annotate_errors(errors, owner))?;
    let core = wasm::encode_module(&wasm).map_err(|errors| annotate_errors(errors, owner))?;
    let (resolve, world) = component::command_world().map_err(|message| {
        annotate_errors(
            vec![BackendError::new("P11 component", mir.span, message)],
            owner,
        )
    })?;
    let binary = component::componentize(&core, &resolve, world).map_err(|message| {
        annotate_errors(
            vec![BackendError::new("P11 component", mir.span, message)],
            owner,
        )
    })?;
    validator().validate_all(&binary).map_err(|error| {
        annotate_errors(
            vec![BackendError::new(
                "P11 Wasm validation",
                mir.span,
                format!("generated WebAssembly failed validation: {error}"),
            )],
            owner,
        )
    })?;
    let text = wasmprinter::print_bytes(&binary).map_err(|error| {
        annotate_errors(
            vec![BackendError::new(
                "P11 WAT printing",
                mir.span,
                format!("generated WebAssembly could not be printed as WAT: {error}"),
            )],
            owner,
        )
    })?;
    Ok(Stages {
        cc,
        mir,
        wasm,
        artifact: Artifact {
            wasm: binary,
            wat: text,
        },
    })
}
