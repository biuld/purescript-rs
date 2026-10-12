pub mod abi;
mod bindings;
mod boundary;
pub mod capability;
pub mod cc;
mod control_provenance;
mod linking;
pub mod mir;
mod pipeline;
mod target_intrinsics;
mod target_runtime;
pub mod trace;
pub mod types;
pub mod wasm;

pub use bindings::{BackendInput, ExternalBinding, ExternalBindings, RuntimeBinding};

pub use capability::TargetCapabilities;
pub use trace::*;

use psrs_hir::ModuleId;
use psrs_span::TextRange;

/// Classifies a backend failure so a caller can distinguish a compiler defect
/// from a valid program the backend does not support (MIR-12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendErrorKind {
    /// The backend produced IR that violates its own invariants. This is always
    /// a compiler bug, never a property of the source program.
    InvalidCompilerIr,
    /// Valid source input that the current backend cannot lower.
    UnsupportedSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendError {
    pub pass: &'static str,
    pub span: TextRange,
    pub message: String,
    /// The source module that owns the failing declaration, when the lowering
    /// pass can identify one. Module IDs are preserved from the frontend so a
    /// multi-module driver can report backend failures against the right file.
    pub module: Option<ModuleId>,
    /// Whether the failure is invalid compiler IR or unsupported source.
    pub kind: BackendErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendWarning {
    pub pass: &'static str,
    pub span: TextRange,
    pub message: String,
    /// The source module that owns the warning, when the lowering pass can
    /// identify one.
    pub module: Option<ModuleId>,
}

impl BackendWarning {
    pub(crate) fn new(pass: &'static str, span: TextRange, message: impl Into<String>) -> Self {
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

impl BackendError {
    fn new(pass: &'static str, span: TextRange, message: impl Into<String>) -> Self {
        Self {
            pass,
            span,
            message: message.into(),
            module: None,
            kind: BackendErrorKind::UnsupportedSource,
        }
    }

    /// Builds a failure that reports invalid compiler IR rather than an
    /// unsupported source program.
    pub(crate) fn invalid_ir(
        pass: &'static str,
        span: TextRange,
        message: impl Into<String>,
    ) -> Self {
        Self {
            pass,
            span,
            message: message.into(),
            module: None,
            kind: BackendErrorKind::InvalidCompilerIr,
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
    pub warnings: Vec<BackendWarning>,
}

pub fn compile(module: psrs_core::Module) -> Result<Artifact, Vec<BackendError>> {
    Ok(compile_with_stages(module)?.artifact)
}

/// Emits an artifact from a P8 input using the default target profile. Source
/// typing and binding conformance belong to P8; this entry rechecks CC and all
/// subsequent representation contracts, including modified inspection inputs.
pub fn compile_cc(input: BackendInput) -> Result<Artifact, Vec<BackendError>> {
    pipeline::compile_cc(input)
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
    /// Verified Typed Core after P7 and before P8.
    pub core: psrs_core::Module,
    pub cc: cc::Module,
    pub mir: mir::Module,
    pub wasm: wasm::Module,
    pub artifact: Artifact,
}

/// IR values captured by the diagnostic compile path. Each field is populated
/// only after that representation has been produced successfully by its pass.
#[derive(Clone, Debug, Default)]
pub struct PartialStages {
    /// Optimized Core after P7.
    pub core: Option<psrs_core::Module>,
    /// CC after closure conversion and its verifier both succeed.
    pub cc: Option<cc::Module>,
    /// Latest MIR produced by lowering or optimization.
    pub mir: Option<mir::Module>,
    /// The pass that most recently produced `mir`.
    pub mir_stage: Option<&'static str>,
}

/// Backend diagnostics together with the last successful IR values.
#[derive(Clone, Debug)]
pub struct CompileFailure {
    pub errors: Vec<BackendError>,
    pub partial: Box<PartialStages>,
}

/// Normal backend result together with the pass/artifact events observed while
/// compiling it. Partial IR values are retained only when requested.
#[derive(Clone, Debug)]
pub struct TracedCompile {
    pub result: Result<Stages, CompileFailure>,
    pub trace: CompileTrace,
}

pub fn compile_with_stages(module: psrs_core::Module) -> Result<Stages, Vec<BackendError>> {
    compile_with_target(module, TargetCapabilities::default())
}

/// Compiles a program using an explicit target capability profile.
pub fn compile_with_target(
    module: psrs_core::Module,
    target: TargetCapabilities,
) -> Result<Stages, Vec<BackendError>> {
    compile_with_context(module, target)
}

pub fn compile_with_context(
    module: psrs_core::Module,
    target: TargetCapabilities,
) -> Result<Stages, Vec<BackendError>> {
    pipeline::compile_with_context_inner(module, target, None, None)
}

/// Compiles with actual top-level pass/artifact tracing. Setting
/// `capture_partial` retains IR snapshots for diagnostics; the trace itself is
/// lightweight and does not clone or format IR values.
pub fn compile_with_context_traced(
    module: psrs_core::Module,
    target: TargetCapabilities,
    capture_partial: bool,
) -> TracedCompile {
    let (result, trace) = pipeline::compile_with_context_traced(module, target, capture_partial);
    TracedCompile { result, trace }
}

/// Compiles using the normal backend pipeline and returns only representations
/// whose producing pass completed. In particular, a CC verifier failure leaves
/// the verified Core available but does not publish the unverified CC candidate.
pub fn compile_with_context_capturing(
    module: psrs_core::Module,
    target: TargetCapabilities,
) -> Result<Stages, CompileFailure> {
    let mut partial = PartialStages::default();
    pipeline::compile_with_context_inner(module, target, Some(&mut partial), None).map_err(
        |errors| CompileFailure {
            errors,
            partial: Box::new(partial),
        },
    )
}
