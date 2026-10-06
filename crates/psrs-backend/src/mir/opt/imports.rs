//! Projects executable functions and imports from the command entry closure.

use super::cfg;
use crate::mir::{Instruction, Module};
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};

pub(super) fn project_reachable(module: &mut Module) {
    let mut edges = HashMap::new();
    for function in &module.functions {
        let mut referenced = HashSet::<SymbolId>::new();
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
                    Instruction::ListCopy { element, .. }
                        if crate::mir::element_has_bytes(element) =>
                    {
                        referenced.insert(crate::abi::STRING_TO_BYTES_SYMBOL);
                        referenced.insert(crate::abi::BYTES_TO_STRING_SYMBOL);
                        referenced.insert(crate::abi::REALLOC_SYMBOL);
                    }
                    _ => {}
                }
            }
            if let Some(crate::mir::Terminator::ReturnCall { function, .. }) = &block.terminator {
                referenced.insert(*function);
            }
        }
        edges.insert(function.symbol, referenced);
    }
    // Modules without an entry expose all their definitions. Command modules
    // expose only the entry and functions it can call or capture as values.
    if let Some(entry) = module.entry {
        let mut pending = vec![entry];
        let mut live = HashSet::new();
        while let Some(symbol) = pending.pop() {
            if live.insert(symbol)
                && let Some(references) = edges.get(&symbol)
            {
                pending.extend(references.iter().copied());
            }
        }
        module
            .functions
            .retain(|function| live.contains(&function.symbol));
        // FunctionId is a module position; call and capture identities remain
        // SymbolIds and are unaffected by compacting the function vector.
        for (position, function) in module.functions.iter_mut().enumerate() {
            function.id = crate::types::FunctionId(position as u32);
        }
    }
    let referenced = module
        .functions
        .iter()
        .flat_map(|function| &edges[&function.symbol])
        .copied()
        .collect::<HashSet<_>>();
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

    fn fixture() -> Module {
        let entry = SymbolId::new(ModuleId(0), 0);
        let imported = SymbolId::new(ModuleId(1), 0);
        let span = TextRange::new(0, 1);
        Module {
            name: "ImportProjectionTest".into(),
            types: Vec::new(),
            strings: Vec::new(),
            layout: None,
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
        }
    }

    #[test]
    fn retains_imports_referenced_by_closure_construction() {
        let mut module = fixture();
        project_reachable(&mut module);
        assert_eq!(module.imports.len(), 1);
        assert_eq!(module.imports[0].symbol, SymbolId::new(ModuleId(1), 0));
    }

    #[test]
    fn only_command_entry_reachability_can_discard_definitions() {
        let mut module = fixture();
        let dead_import = SymbolId::new(ModuleId(2), 0);
        let mut dead = module.functions[0].clone();
        dead.symbol = SymbolId::new(ModuleId(0), 1);
        dead.id = FunctionId(0);
        module.functions[0].id = FunctionId(1);
        let Instruction::ClosureNew { function, .. } = &mut dead.blocks[0].instructions[0] else {
            panic!("fixture constructs a closure");
        };
        *function = dead_import;
        module.functions.insert(0, dead);
        module.imports.push(Import {
            symbol: dead_import,
            parameters: Vec::new(),
            result: None,
        });
        let mut library = module.clone();
        library.entry = None;
        project_reachable(&mut library);
        assert_eq!(library.functions.len(), 2);
        assert_eq!(library.imports.len(), 2);
        project_reachable(&mut module);
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].id, FunctionId(0));
        assert_eq!(module.imports.len(), 1);
        assert_eq!(module.imports[0].symbol, SymbolId::new(ModuleId(1), 0));
    }
}
