use crate::abi::WasiRegistry;
use crate::types::{RecGroup, ValueDecl, ValueId, ValueType};
use crate::{BackendError, cc};
use psrs_hir::{ExternalSymbol, SymbolId};
use psrs_span::TextRange;

mod instruction;
mod lower;
mod verify;

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
    pub externals: Vec<ExternalSymbol>,
    /// Defined GC types owned by MIR. The Wasm encoding emits them after the
    /// function types at a fixed base; see `docs/design/D-06`.
    pub types: Vec<RecGroup>,
    /// Runtime ABI imports the module may call. Their canonical signatures come
    /// from the WIT runtime ABI; see `docs/decision/DEC-06`.
    pub imports: Vec<Import>,
    pub functions: Vec<Function>,
    pub span: TextRange,
}

/// A runtime ABI import, lowered to its canonical ABI signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub symbol: SymbolId,
    pub module: String,
    pub name: String,
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

pub fn lower_module(module: cc::Module) -> Result<Module, Vec<BackendError>> {
    let mut wasi = WasiRegistry::load()
        .map_err(|message| vec![BackendError::new("P9 MIR lowering", module.span, message)])?;
    let mut functions = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        functions.push(lower_function(function, &mut wasi)?);
    }
    let imports = wasi
        .imports()
        .iter()
        .map(|import| Import {
            symbol: import.symbol,
            module: import.module.clone(),
            name: import.name.clone(),
            parameters: import.parameters.clone(),
            result: import.result,
        })
        .collect();
    let mir = Module {
        name: module.name,
        externals: module.externals,
        types: Vec::new(),
        imports,
        functions,
        span: module.span,
    };
    verify_module(&mir)?;
    Ok(mir)
}
