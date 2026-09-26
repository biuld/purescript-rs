use super::optimize;
use crate::TargetCapabilities;
use crate::mir::{
    BasicBlock, BlockId, Function, Import, Instruction, Module, NumericOp, Terminator,
};
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn value(id: u32, ty: ValueType) -> ValueDecl {
    ValueDecl {
        id: ValueId(id),
        ty,
    }
}

fn module(functions: Vec<Function>, imports: Vec<Import>, entry: SymbolId) -> Module {
    Module {
        name: "OptimizationTest".into(),
        types: Vec::new(),
        strings: Vec::new(),
        imports,
        functions,
        entry: Some(entry),
        span: span(),
    }
}

mod constants;
mod control_flow;
mod copy;
mod inline;
