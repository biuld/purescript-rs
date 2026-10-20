use psrs_hir::SymbolId;
use psrs_span::TextRange;
use wasm_encoder::{HeapType, Instruction, ValType};

use crate::types::{DataId, MemoryId};

mod convert;
mod encode;
mod lower;
mod type_contract;
mod verify;

#[cfg(test)]
mod tests;

pub use encode::encode_module;
pub(crate) use lower::lower_module_with_plan;
pub use lower::{lower_module, lower_module_with_capabilities};
pub(crate) use type_contract::raw_signature;

/// Final index domains assigned by P10. These are deliberately distinct from
/// MIR's module-local IDs and from one another; conversion to raw `u32` is
/// confined to the encoder boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeIndex(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionIndex(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MemoryIndex(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DataIndex(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlobalIndex(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TagIndex(pub u32);

/// A module-level global variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Global {
    pub index: GlobalIndex,
    pub mutable: bool,
    pub ty: ValType,
    pub init: GlobalInit,
}

/// A global's constant initializer. Only the forms the backend emits are
/// modeled; the encoder maps each to a `wasm_encoder::ConstExpr`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalInit {
    /// `ref.null` of the given heap type.
    RefNull(HeapType),
    /// `i32.const`.
    I32(u32),
}

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
    pub type_index: TypeIndex,
}

/// A linear memory in the module skeleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory {
    pub id: MemoryId,
    pub index: MemoryIndex,
    pub minimum: u64,
    pub maximum: Option<u64>,
}

/// The kind of an export in the module skeleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Function,
    Memory,
    Global,
}

/// An exported item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub index: ExportIndex,
}

/// The index domain selected by an export's kind. A function index can never
/// be accidentally passed where a memory index is expected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportIndex {
    Function(FunctionIndex),
    Memory(MemoryIndex),
    Global(GlobalIndex),
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
    /// A branch label whose depth is relative to the enclosing structured
    /// control stack.
    Block {
        body: Body,
        result: Option<ValType>,
        span: TextRange,
    },
    /// A loop label whose depth is relative to the enclosing structured
    /// control stack.
    Loop {
        body: Body,
        result: Option<ValType>,
        span: TextRange,
    },
    /// A `try_table` region: the protected body executes inside the region's
    /// own internal label, and the catch clauses arm its unwinding behavior.
    /// A clause's `label` is depth-relative to the active-label stack at the
    /// `try_table`'s emission point, not counting the region's internal label.
    Try {
        body: Body,
        clauses: Vec<CatchClause>,
        span: TextRange,
    },
}

/// A `try_table` catch clause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatchClause {
    /// `catch $tag $label`: an exception of the named tag is delivered to the
    /// label as its branch arguments, so the label's result type must equal
    /// the tag's payload.
    Catch { tag: TagIndex, label: u32 },
    /// `catch_all $label`: any exception is delivered to the label, which
    /// must therefore take no arguments.
    CatchAll { label: u32 },
    /// Delivers the original exception as one non-null exception reference.
    CatchAllRef { label: u32 },
}

/// A module-local exception tag. The tag attribute is always 0 (exception),
/// and this design neither imports nor exports a tag; a tag's function type
/// carries the payload as its parameters and produces no results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagDef {
    pub type_index: TypeIndex,
    pub span: TextRange,
}

/// A sequence of structured Wasm operations.
pub type Body = Vec<Op>;

/// A Wasm function with a structured body.
#[derive(Clone, Debug)]
pub struct Function {
    pub symbol: SymbolId,
    pub name: String,
    pub type_index: TypeIndex,
    pub parameters: Vec<ValType>,
    pub locals: Vec<ValType>,
    pub body: Body,
    pub span: TextRange,
}

/// How a data segment is made available: copied into linear memory at a fixed
/// offset, or held passively for `array.new_data` to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataMode {
    Active { offset: u32 },
    Passive,
}

/// An initialized data segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataSegment {
    pub id: DataId,
    pub index: DataIndex,
    pub mode: DataMode,
    pub bytes: Vec<u8>,
}

/// The synthesized command entry that calls `main` and exits.
#[derive(Clone, Debug)]
pub struct Entry {
    pub type_index: TypeIndex,
    /// The entry's declared locals. Empty except for the budget's watermark
    /// save local when the target enables `exceptions`.
    pub locals: Vec<ValType>,
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
    pub tags: Vec<TagDef>,
    pub globals: Vec<Global>,
    pub data: Vec<DataSegment>,
    pub exports: Vec<Export>,
    pub entry: Option<Entry>,
    /// A synthesized `cabi_realloc` export, present when canonical ABI lowering
    /// needs guest linear-memory allocation.
    pub realloc: Option<Function>,
    /// Synthesized string-boundary codec functions. They are local functions,
    /// referenced by reserved MIR symbols, and encoded after `realloc`.
    pub helpers: Vec<Function>,
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
