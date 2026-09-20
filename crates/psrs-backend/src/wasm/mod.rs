use psrs_hir::SymbolId;
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};

mod convert;
mod encode;
mod lower;
mod verify;

#[cfg(test)]
mod tests;

pub use encode::encode_module;
pub use lower::{lower_module, lower_module_with_capabilities};

/// A WebAssembly function signature in the thin Wasm IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncType {
    pub parameters: Vec<ValType>,
    pub results: Vec<ValType>,
}

/// A function imported from the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub type_index: u32,
}

/// A linear memory in the module skeleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory {
    pub minimum: u64,
    pub maximum: Option<u64>,
}

/// The kind of an export in the module skeleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Function,
    Memory,
}

/// An exported item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub index: u32,
}

/// A structured function body.
///
/// Only control-flow structure is modeled here. Leaf opcodes are delegated to
/// `wasm_encoder::Instruction`, so this IR never mirrors the full Wasm
/// instruction set.
#[derive(Clone, Debug)]
pub enum Op {
    /// A flat Wasm instruction, including any value pushed for a following
    /// structured `if` condition.
    Leaf(Instruction<'static>),
    /// A value-producing `if`/`else`/`end` region.
    If {
        then_body: Body,
        else_body: Body,
        result: Option<ValType>,
        span: TextRange,
    },
}

/// A sequence of structured Wasm operations.
pub type Body = Vec<Op>;

/// A Wasm function with a structured body.
#[derive(Clone, Debug)]
pub struct Function {
    pub symbol: SymbolId,
    pub name: String,
    pub type_index: u32,
    pub parameters: Vec<ValType>,
    pub locals: Vec<ValType>,
    pub body: Body,
    pub span: TextRange,
}

/// An initialized data segment in linear memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataSegment {
    pub offset: u32,
    pub bytes: Vec<u8>,
}

/// The synthesized command entry that calls `main` and exits.
#[derive(Clone, Debug)]
pub struct Entry {
    pub type_index: u32,
    pub body: Body,
}

/// A thin, structured WebAssembly module: the target skeleton plus function
/// bodies whose leaf opcodes come from `wasm_encoder`.
///
/// Defined GC types occupy the start of the type index space; function types
/// follow at `defined_type_count()`, so a reference to defined type `i` is type
/// index `i` and a function type at position `j` is index `n + j`.
#[derive(Clone, Debug)]
pub struct Module {
    pub name: String,
    pub imports: Vec<Import>,
    pub types: Vec<FuncType>,
    pub type_defs: Vec<crate::types::RecGroup>,
    pub functions: Vec<Function>,
    pub memories: Vec<Memory>,
    pub data: Vec<DataSegment>,
    pub exports: Vec<Export>,
    pub entry: Option<Entry>,
    /// A synthesized `cabi_realloc` export, present when the module imports a
    /// function that returns a `list`/`string`.
    pub realloc: Option<Function>,
    pub span: TextRange,
}

impl Module {
    /// The number of defined (GC) types. Defined types occupy indices `0..n`;
    /// function types follow at `n`.
    pub fn defined_type_count(&self) -> u32 {
        self.type_defs
            .iter()
            .map(|group| group.0.len() as u32)
            .sum()
    }
}
