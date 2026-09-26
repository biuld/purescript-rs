use crate::abi::WasiRegistry;
use crate::capability::TargetCapabilities;
use crate::types::{FunctionId, RecGroup, ValueDecl, ValueId, ValueType};
use crate::{BackendError, annotate_errors, cc};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

pub(crate) mod cfg;
mod instruction;
mod layout;
mod literals;
mod lower;
mod numeric;
pub mod opt;
mod planner;
mod reachable;
mod scalar_helpers;
mod verify;
mod wit;

use literals::StringLiterals;
use lower::lower_function;
use planner::{GcPlanner, RepresentationPlanner};
use scalar_helpers::lower_scalar_helpers;

pub use instruction::{Instruction, ListDirection};
pub use numeric::{NumericOp, UnaryOp};
pub use verify::{verify_module, verify_module_with_capabilities};

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod gc_tests;
#[cfg(test)]
mod indirect_tests;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    /// Defined GC types owned by MIR. The Wasm encoding emits them after the
    /// function types at a fixed base; see
    /// `docs/design/backend/00-ir-boundaries.md`.
    pub types: Vec<RecGroup>,
    /// Static string literals referenced by `ArrayNewData` data indices.
    pub strings: Vec<String>,
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
    /// Stable module-local MIR identity. P10 maps this identity to a final
    /// Wasm function index after imports are ordered.
    pub id: FunctionId,
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
    /// A two-way branch. It carries no structuring hint: the structurer
    /// derives the join from the CFG edges (the nearest common descendant of
    /// `then_block` and `else_block`).
    Branch {
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
        span: TextRange,
    },
    /// Selects a basic block using a closed integer tag. Case values are
    /// required to be unique; the default target makes the dispatch total.
    Switch {
        value: ValueId,
        cases: Vec<(i32, BlockId)>,
        default: BlockId,
        span: TextRange,
    },
    /// A direct tail call. The arguments are evaluated and control transfers to
    /// `function`, whose result becomes this function's result.
    ReturnCall {
        function: SymbolId,
        arguments: Vec<ValueId>,
        span: TextRange,
    },
    /// A tail call through a typed function reference. `function` is a value of
    /// typed function-reference type; a closure tail call projects its code
    /// reference and passes the receiver as the first argument before forming
    /// this terminator.
    ReturnCallRef {
        function: ValueId,
        arguments: Vec<ValueId>,
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
    bindings.validate_cc(&module)?;
    let wasi = WasiRegistry::load_with_capabilities(target).map_err(|message| {
        annotate_errors(
            vec![BackendError::new("P9 MIR lowering", module.span, message)],
            module.entry.map(|entry| entry.module),
        )
    })?;
    lower_module_after_binding_validation(module, bindings, target, wasi)
}

#[cfg(test)]
pub(crate) fn lower_module_with_registry(
    module: cc::Module,
    bindings: crate::ExternalBindings,
    target: TargetCapabilities,
    wasi: WasiRegistry,
) -> Result<(Module, WasiRegistry), Vec<BackendError>> {
    bindings.validate_cc(&module)?;
    lower_module_after_binding_validation(module, bindings, target, wasi)
}

fn lower_module_after_binding_validation(
    module: cc::Module,
    bindings: crate::ExternalBindings,
    target: TargetCapabilities,
    mut wasi: WasiRegistry,
) -> Result<(Module, WasiRegistry), Vec<BackendError>> {
    // Resolve and validate every source-declared WIT binding. A declaration
    // must fail with its ABI diagnostic even when dead code does not call it;
    // the later import projection keeps unused runtime imports out of MIR.
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
        wit_imports.insert(
            external.symbol,
            crate::abi::BoundWasiImport {
                import,
                signature: signature.clone(),
            },
        );
    }
    let layout = GcPlanner { target }.plan_module(&module).map_err(|error| {
        annotate_errors(
            vec![BackendError::new(
                "P9 MIR lowering",
                module.span,
                format!("invalid representation table: {error:?}"),
            )],
            module.entry.map(|entry| entry.module),
        )
    })?;
    let (scalar_helpers, generated_helpers) =
        lower_scalar_helpers(&module, module.functions.len() as u32);
    let mut conversion_helpers = lower::ConversionHelpers::new(&module, &generated_helpers);
    let mut literals = StringLiterals::default();
    let mut functions = Vec::with_capacity(module.functions.len());
    for (id, function) in module.functions.iter().enumerate() {
        let lowered = lower_function(
            function,
            FunctionId(id as u32),
            &wit_imports,
            &scalar_helpers,
            &layout,
            Some(&mut conversion_helpers),
            Some(&mut literals),
            target,
        )
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.with_module(function.symbol.module))
                .collect::<Vec<_>>()
        })?;
        functions.push(lowered);
    }
    functions.extend(generated_helpers);
    let first_helper_id = functions.len() as u32;
    for (offset, helper) in conversion_helpers.into_functions().iter().enumerate() {
        let lowered = lower_function(
            helper,
            FunctionId(first_helper_id + offset as u32),
            &wit_imports,
            &scalar_helpers,
            &layout,
            None,
            Some(&mut literals),
            target,
        )?;
        functions.push(lowered);
    }
    // Keep only the imports a lowered call actually references, so a resolved but
    // unused external does not add a Wasm import.
    let used = referenced_imports(&functions);
    let mut imports: Vec<Import> = wasi
        .imports()
        .iter()
        .filter(|import| used.contains(&import.symbol))
        .map(|import| Import {
            symbol: import.symbol,
            parameters: import.parameters.clone(),
            result: import.result,
        })
        .collect();
    if used.contains(&crate::abi::REALLOC_SYMBOL)
        || wasi
            .imports()
            .iter()
            .any(|import| used.contains(&import.symbol) && import.has_indirect_parameters())
    {
        imports.push(Import {
            symbol: crate::abi::REALLOC_SYMBOL,
            parameters: vec![ValueType::I32; 4],
            result: Some(ValueType::I32),
        });
    }
    // The canonical ABI boundary transcodes between the GC string's UTF-16 and
    // the component's UTF-8. The adapter calls these reserved helpers, which P10
    // synthesizes as ordinary Wasm functions; they are never core imports.
    if used.contains(&crate::abi::STRING_TO_BYTES_SYMBOL)
        || used.contains(&crate::abi::BYTES_TO_STRING_SYMBOL)
    {
        let string_type = layout
            .value_type(&cc::ValueShape::String)
            .map_err(|error| {
                annotate_errors(
                    vec![BackendError::new(
                        "P9 MIR lowering",
                        module.span,
                        format!("invalid string layout: {error:?}"),
                    )],
                    module.entry.map(|entry| entry.module),
                )
            })?;
        if used.contains(&crate::abi::STRING_TO_BYTES_SYMBOL) {
            imports.push(Import {
                symbol: crate::abi::STRING_TO_BYTES_SYMBOL,
                parameters: vec![string_type],
                result: Some(ValueType::I32),
            });
        }
        if used.contains(&crate::abi::BYTES_TO_STRING_SYMBOL) {
            imports.push(Import {
                symbol: crate::abi::BYTES_TO_STRING_SYMBOL,
                parameters: vec![ValueType::I32, ValueType::I32],
                result: Some(string_type),
            });
        }
    }
    let strings = literals.into_strings();
    let mir = Module {
        name: module.name,
        types: layout.types,
        strings,
        imports,
        functions,
        entry: module.entry,
        span: module.span,
    };
    verify_module_with_capabilities(&mir, target)?;
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
                    Instruction::ListCopy {
                        element: crate::abi::ListElement::String,
                        ..
                    } => {
                        used.insert(crate::abi::STRING_TO_BYTES_SYMBOL);
                        used.insert(crate::abi::BYTES_TO_STRING_SYMBOL);
                        used.insert(crate::abi::REALLOC_SYMBOL);
                    }
                    _ => {}
                }
            }
        }
    }
    used
}
