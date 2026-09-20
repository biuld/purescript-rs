pub mod abi;
mod bindings;
pub mod capability;
pub mod cc;
pub mod component;
pub mod mir;
pub mod types;
pub mod wasm;

pub use bindings::{BackendInput, ExternalBinding, ExternalBindings};

pub use capability::TargetCapabilities;

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

/// The default-profile validator, retained for backend unit tests.
#[allow(dead_code)]
pub(crate) fn validator() -> wasmparser::Validator {
    validator_for(TargetCapabilities::default())
}

/// Creates a validator that accepts exactly the proposals declared by a
/// target profile.  This deliberately starts from MVP instead of the
/// dependency's defaults, so a library upgrade cannot silently broaden the
/// artifact contract.
pub(crate) fn validator_for(target: TargetCapabilities) -> wasmparser::Validator {
    wasmparser::Validator::new_with_features(target.wasm_features())
}

#[derive(Clone, Debug)]
pub struct Stages {
    pub cc: cc::Module,
    pub mir: mir::Module,
    pub wasm: wasm::Module,
    pub artifact: Artifact,
}

pub fn compile_with_stages(module: psrs_core::Module) -> Result<Stages, Vec<BackendError>> {
    compile_with_target(module, TargetCapabilities::default())
}

/// Compiles a program using an explicit target capability profile.
pub fn compile_with_target(
    module: psrs_core::Module,
    target: TargetCapabilities,
) -> Result<Stages, Vec<BackendError>> {
    let external_bindings = ExternalBindings::from_core(&module);
    let lowered_cc = cc::lower_module_with_bindings(module, external_bindings)?;
    let cc = lowered_cc.cc;
    let (mir, mut wasi) =
        mir::lower_module_with_bindings(cc.clone(), lowered_cc.externals, target)?;
    let owner = mir.entry.map(|entry| entry.module);
    if !target.component_model
        || !target.wasi_p2
        || !target.wasi_cli
        || !target.wasi_io
        || !target.wasi_clocks
        || !target.wasi_random
    {
        return Err(annotate_errors(
            vec![BackendError::new(
                "P11 target capabilities",
                mir.span,
                "the current artifact pipeline requires Component Model and WASI 0.2 capabilities",
            )],
            owner,
        ));
    }
    let wasm = wasm::lower_module_with_capabilities(&mir, &mut wasi, target)
        .map_err(|errors| annotate_errors(errors, owner))?;
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
    validator_for(target)
        .validate_all(&binary)
        .map_err(|error| {
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
