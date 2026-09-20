use crate::abi::WasiRegistry;
use crate::capability::TargetCapabilities;
use crate::types::{RecGroup, ValueDecl, ValueId, ValueType};
use crate::{BackendError, annotate_errors, cc};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

mod instruction;
mod layout;
mod lower;
mod verify;
mod wit;

use layout::PlannedLayout;
use lower::lower_function;

pub use instruction::Instruction;
pub use verify::verify_module;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    /// Defined GC types owned by MIR. The Wasm encoding emits them after the
    /// function types at a fixed base; see `docs/design/D-06`.
    pub types: Vec<RecGroup>,
    /// Runtime ABI imports the module may call. Their canonical signatures come
    /// from the WIT runtime ABI; see `docs/decision/DEC-06`.
    pub imports: Vec<Import>,
    pub functions: Vec<Function>,
    /// The program entry declaration, if selected by the driver.
    pub entry: Option<SymbolId>,
    pub span: TextRange,
}

/// A runtime ABI import, lowered to its canonical ABI signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    /// The canonical ABI symbol the call references. The interface, function,
    /// and return-pointer details live in the ABI registry, not here.
    pub symbol: SymbolId,
    pub parameters: Vec<ValueType>,
    pub result: Option<ValueType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub symbol: SymbolId,
    pub name: String,
    pub parameters: Vec<ValueId>,
    pub values: Vec<ValueDecl>,
    pub entry: BlockId,
    pub blocks: Vec<BasicBlock>,
    pub result: ValueId,
    pub result_type: ValueType,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasicBlock {
    pub id: BlockId,
    pub parameters: Vec<ValueId>,
    pub instructions: Vec<Instruction>,
    pub terminator: Option<Terminator>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Return {
        value: ValueId,
        span: TextRange,
    },
    Jump {
        target: BlockId,
        arguments: Vec<ValueId>,
        span: TextRange,
    },
    Branch {
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
        merge_block: BlockId,
        span: TextRange,
    },
}

/// Lowers a CC module to MIR, returning the module and the ABI registry that
/// resolved its WIT imports. The registry is returned so the Wasm stage can name
/// each import; the MIR module itself stores no WIT or component detail.
pub fn lower_module(module: cc::Module) -> Result<(Module, WasiRegistry), Vec<BackendError>> {
    lower_module_with_capabilities(module, TargetCapabilities::default())
}

/// Lowers CC to MIR using an explicit target capability profile.
pub fn lower_module_with_capabilities(
    module: cc::Module,
    target: TargetCapabilities,
) -> Result<(Module, WasiRegistry), Vec<BackendError>> {
    lower_module_with_bindings(module, crate::ExternalBindings::default(), target)
}

/// Lowers CC to MIR with the external binding side table produced by P8.
pub fn lower_module_with_bindings(
    module: cc::Module,
    bindings: crate::ExternalBindings,
    target: TargetCapabilities,
) -> Result<(Module, WasiRegistry), Vec<BackendError>> {
    let mut wasi = WasiRegistry::load_with_capabilities(target).map_err(|message| {
        annotate_errors(
            vec![BackendError::new("P9 MIR lowering", module.span, message)],
            module.entry.map(|entry| entry.module),
        )
    })?;
    // Resolve every source-declared WIT import to its canonical ABI descriptor.
    let mut wit_imports = HashMap::new();
    for external in &bindings.imports {
        let interface = &external.interface;
        let function = &external.function;
        let import = wasi.import(interface, function).map_err(|message| {
            vec![
                BackendError::new("P9 MIR lowering", module.span, message)
                    .with_module(external.symbol.module),
            ]
        })?;
        if let Some(reason) = &import.unsupported {
            return Err(vec![
                BackendError::new(
                    "P9 MIR lowering",
                    external
                        .signature
                        .as_ref()
                        .map_or(module.span, |signature| signature.span),
                    format!("WIT import `{interface}#{function}` is unsupported: {reason}"),
                )
                .with_module(external.symbol.module),
            ]);
        }
        let Some(signature) = external.signature.as_ref() else {
            return Err(vec![
                BackendError::new(
                    "P9 MIR lowering",
                    module.span,
                    format!("WIT import `{interface}#{function}` has no source signature"),
                )
                .with_module(external.symbol.module),
            ]);
        };
        wasi.validate_signature(&import, signature)
            .map_err(|message| {
                vec![
                    BackendError::new("P9 MIR lowering", signature.span, message)
                        .with_module(external.symbol.module),
                ]
            })?;
        wit_imports.insert(external.symbol, import);
    }
    let planned_layout = PlannedLayout::plan(&module.representations, target).map_err(|error| {
        annotate_errors(
            vec![BackendError::new(
                "P9 MIR lowering",
                module.span,
                format!("invalid representation table: {error:?}"),
            )],
            module.entry.map(|entry| entry.module),
        )
    })?;
    let mut functions = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        let lowered =
            lower_function(function, &wit_imports, &planned_layout).map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| error.with_module(function.symbol.module))
                    .collect::<Vec<_>>()
            })?;
        functions.push(lowered);
    }
    // Keep only the imports a lowered call actually references, so a resolved but
    // unused external does not add a Wasm import.
    let used = referenced_imports(&functions);
    let imports = wasi
        .imports()
        .iter()
        .filter(|import| used.contains(&import.symbol))
        .map(|import| Import {
            symbol: import.symbol,
            parameters: import.parameters.clone(),
            result: import.result,
        })
        .collect();
    let mir = Module {
        name: module.name,
        types: planned_layout.types.clone(),
        imports,
        functions,
        entry: module.entry,
        span: module.span,
    };
    verify_module(&mir)?;
    Ok((mir, wasi))
}

/// The import symbols referenced by any call in the module.
fn referenced_imports(functions: &[Function]) -> HashSet<SymbolId> {
    let mut used = HashSet::new();
    for function in functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                match instruction {
                    Instruction::Call { function, .. } | Instruction::CallVoid { function, .. } => {
                        used.insert(*function);
                    }
                    _ => {}
                }
            }
        }
    }
    used
}
