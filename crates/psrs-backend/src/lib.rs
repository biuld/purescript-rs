pub mod abi;
pub mod cc;
pub mod component;
pub mod mir;
pub mod types;
pub mod wasm;

use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendError {
    pub pass: &'static str,
    pub span: TextRange,
    pub message: String,
}

impl BackendError {
    fn new(pass: &'static str, span: TextRange, message: impl Into<String>) -> Self {
        Self {
            pass,
            span,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub wasm: Vec<u8>,
    pub wat: String,
}

pub fn compile(module: psrs_core::Module) -> Result<Artifact, Vec<BackendError>> {
    Ok(compile_with_stages(module)?.artifact)
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
    let mir = mir::lower_module(cc.clone())?;
    let wasm = wasm::lower_module(&mir)?;
    let core = wasm::encode_module(&wasm)?;
    let (resolve, world) = component::command_world()
        .map_err(|message| vec![BackendError::new("P11 component", mir.span, message)])?;
    let binary = component::componentize(&core, &resolve, world)
        .map_err(|message| vec![BackendError::new("P11 component", mir.span, message)])?;
    wasmparser::Validator::new()
        .validate_all(&binary)
        .map_err(|error| {
            vec![BackendError::new(
                "P11 Wasm validation",
                mir.span,
                format!("generated WebAssembly failed validation: {error}"),
            )]
        })?;
    let text = wasmprinter::print_bytes(&binary).map_err(|error| {
        vec![BackendError::new(
            "P11 WAT printing",
            mir.span,
            format!("generated WebAssembly could not be printed as WAT: {error}"),
        )]
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
