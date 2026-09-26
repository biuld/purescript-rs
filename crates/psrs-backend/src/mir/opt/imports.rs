//! Removes prevalidated imports no longer referenced by optimized code.

use super::cfg;
use crate::mir::{Instruction, Module};
use psrs_hir::SymbolId;
use std::collections::HashSet;

pub(super) fn project_reachable(module: &mut Module) {
    let mut referenced = HashSet::<SymbolId>::new();
    for function in &module.functions {
        let reachable = cfg::reachable_blocks(function.entry, &function.blocks);
        for block in &function.blocks {
            if !reachable.contains(&block.id) {
                continue;
            }
            for instruction in &block.instructions {
                match instruction {
                    Instruction::Call { function, .. }
                    | Instruction::CallVoid { function, .. }
                    | Instruction::RefFunc { function, .. }
                    | Instruction::ClosureNew { function, .. } => {
                        referenced.insert(*function);
                    }
                    // A canonical string-list copy names its codec and allocator
                    // helpers directly in Wasm lowering, not through a MIR call.
                    Instruction::ListCopy {
                        element: crate::abi::ListElement::String,
                        ..
                    } => {
                        referenced.insert(crate::abi::STRING_TO_BYTES_SYMBOL);
                        referenced.insert(crate::abi::BYTES_TO_STRING_SYMBOL);
                        referenced.insert(crate::abi::REALLOC_SYMBOL);
                    }
                    _ => {}
                }
            }
        }
    }
    module
        .imports
        .retain(|import| referenced.contains(&import.symbol));
}

#[cfg(test)]
mod tests {
    use super::project_reachable;
    use crate::mir::{BasicBlock, Function, Import, Instruction, Module, Terminator};
    use crate::types::{
        DefinedTypeId, FunctionId, HeapType, RefType, ValueDecl, ValueId, ValueType,
    };
    use psrs_hir::{ModuleId, SymbolId};
    use psrs_span::TextRange;

    #[test]
    fn retains_imports_referenced_by_closure_construction() {
        let entry = SymbolId::new(ModuleId(0), 0);
        let imported = SymbolId::new(ModuleId(1), 0);
        let span = TextRange::new(0, 1);
        let mut module = Module {
            name: "ImportProjectionTest".into(),
            types: Vec::new(),
            strings: Vec::new(),
            imports: vec![Import {
                symbol: imported,
                parameters: Vec::new(),
                result: None,
            }],
            functions: vec![Function {
                id: FunctionId(0),
                symbol: entry,
                name: "main".into(),
                parameters: Vec::new(),
                values: vec![ValueDecl {
                    id: ValueId(0),
                    ty: ValueType::Ref(RefType {
                        nullable: false,
                        heap: HeapType::Index(DefinedTypeId(0)),
                    }),
                }],
                entry: crate::mir::BlockId(0),
                blocks: vec![BasicBlock {
                    id: crate::mir::BlockId(0),
                    parameters: Vec::new(),
                    instructions: vec![Instruction::ClosureNew {
                        destination: ValueId(0),
                        function: imported,
                        type_index: DefinedTypeId(0),
                        closure_type: DefinedTypeId(0),
                        capture_array_type: DefinedTypeId(0),
                        boxed_integer_type: None,
                        boxed_f64_type: None,
                        captures: Vec::new(),
                        span,
                    }],
                    terminator: Some(Terminator::Return {
                        value: ValueId(0),
                        span,
                    }),
                }],
                result: ValueId(0),
                result_type: ValueType::Ref(RefType {
                    nullable: false,
                    heap: HeapType::Index(DefinedTypeId(0)),
                }),
                span,
            }],
            entry: Some(entry),
            span,
        };

        project_reachable(&mut module);

        assert_eq!(module.imports.len(), 1);
        assert_eq!(module.imports[0].symbol, imported);
    }
}
