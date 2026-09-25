use super::convert::val_type;
use super::{
    Body, DataIndex, DataSegment, Entry, Export, ExportIndex, ExportKind, FuncType, Function,
    FunctionIndex, Import, Memory, MemoryIndex, Module, Op, TypeIndex,
};
use crate::BackendError;
use crate::abi::{self, names};
use crate::capability::TargetCapabilities;
use crate::mir::{self, Function as MirFunction};
use crate::types::{CompositeType, DataId, MemoryId, ValueId, ValueType};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::HashMap;
use wasm_encoder::{Instruction, ValType};

mod extent;
mod realloc;
mod runtime;
mod structure;

use realloc::build_realloc;
use runtime::collect_strings;
use structure::Structurer;

/// Structures MIR control flow and builds the thin Wasm IR. The ABI registry
/// names each MIR import; MIR itself carries only canonical signatures.
pub fn lower_module(
    module: &mir::Module,
    wasi: &mut abi::WasiRegistry,
) -> Result<Module, Vec<BackendError>> {
    lower_module_with_capabilities(module, wasi, TargetCapabilities::default())
}

/// Lowers MIR using an explicit target capability profile.
pub fn lower_module_with_capabilities(
    module: &mir::Module,
    wasi: &mut abi::WasiRegistry,
    target: TargetCapabilities,
) -> Result<Module, Vec<BackendError>> {
    mir::verify_module_with_capabilities(module, target)?;
    let Some(entry_symbol) = module.entry else {
        return Err(wasm_error(
            module.span,
            "no program entry point was selected for the component artifact",
        ));
    };
    let Some(main_position) = module
        .functions
        .iter()
        .position(|function| function.symbol == entry_symbol)
    else {
        return Err(wasm_error(
            module.span,
            "the program entry declaration is not a lowered function",
        ));
    };
    let entry_function = &module.functions[main_position];
    if !entry_function.parameters.is_empty() || entry_function.result_type != ValueType::I32 {
        return Err(wasm_error(
            entry_function.span,
            "the component artifact requires a zero-argument Int entry declaration",
        ));
    }

    let (string_offsets, mut data, data_end) = collect_strings(module);
    extent::verify_static_access_extents(module, &string_offsets)?;
    // `cabi_realloc` is needed only when an imported function returns a list or
    // string that the host allocates in guest memory.
    let needs_realloc = module
        .imports
        .iter()
        .any(|import| wasi.has_list_result(import.symbol));

    let type_defs = module.types.clone();
    let defined = type_defs
        .iter()
        .map(|group| group.0.len() as u32)
        .sum::<u32>();
    let (mut types, function_types) = collect_function_types(module, defined, Vec::new())?;

    // Core imports: the WASI imports MIR declared, then `exit-with-code` used
    // by the synthesized `run` entry when the target exposes WASI CLI.
    let mut imports = Vec::new();
    let mut import_indices = HashMap::<SymbolId, FunctionIndex>::new();
    for import in &module.imports {
        let type_index = TypeIndex(defined + types.len() as u32);
        types.push(FuncType {
            parameters: import.parameters.iter().map(|ty| val_type(*ty)).collect(),
            results: import
                .result
                .map(|ty| vec![val_type(ty)])
                .unwrap_or_default(),
        });
        let (module_name, field) = wasi.symbol_name(import.symbol).ok_or_else(|| {
            wasm_error(module.span, "a MIR import symbol has no ABI registry entry")
        })?;
        import_indices.insert(import.symbol, FunctionIndex(imports.len() as u32));
        imports.push(Import {
            module: module_name.to_string(),
            name: field.to_string(),
            type_index,
        });
    }
    // The synthesized `run` entry exits with `main`'s code through WASI. The
    // registry is the one P9 built; interning `exit` here does not add an import
    // unless a lowered call references it.
    let exit_index = if target.wasi_cli {
        let exit = wasi
            .import(names::EXIT, names::EXIT_WITH_CODE)
            .map_err(|message| {
                vec![BackendError::new(
                    "P10 Wasm structuring",
                    module.span,
                    message,
                )]
            })?;
        let exit_type_index = TypeIndex(defined + types.len() as u32);
        types.push(FuncType {
            parameters: exit.parameters.iter().map(|ty| val_type(*ty)).collect(),
            results: Vec::new(),
        });
        let index = FunctionIndex(imports.len() as u32);
        imports.push(Import {
            module: exit.module,
            name: exit.name,
            type_index: exit_type_index,
        });
        Some(index)
    } else {
        None
    };

    let import_count = imports.len() as u32;
    let entry_type = TypeIndex(defined + types.len() as u32);
    types.push(FuncType {
        parameters: Vec::new(),
        results: vec![ValType::I32],
    });

    let entry_index = FunctionIndex(import_count + module.functions.len() as u32);

    let mut function_indices = module
        .functions
        .iter()
        .map(|function| (function.symbol, FunctionIndex(import_count + function.id.0)))
        .collect::<HashMap<_, _>>();
    for (symbol, index) in &import_indices {
        function_indices.insert(*symbol, *index);
    }

    let mut functions = Vec::with_capacity(module.functions.len());
    for (index, source) in module.functions.iter().enumerate() {
        let lowered = lower_function(
            source,
            function_types[index],
            &function_indices,
            &string_offsets,
        )
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.with_module(source.symbol.module))
                .collect::<Vec<_>>()
        })?;
        functions.push(lowered);
    }

    let main_index = FunctionIndex(import_count + entry_function.id.0);

    // An imported function that returns a list/string has the host allocate the
    // buffer in guest memory, so the module must export `cabi_realloc`. The
    // allocator is a bump allocator whose free pointer lives in a data segment
    // after the string data. It prefixes each allocation with its length, so a
    // returned `(pointer, length)` can be a length-prefixed string value.
    let mut exports = vec![
        Export {
            name: abi::RUN_CORE_EXPORT.into(),
            kind: ExportKind::Function,
            index: ExportIndex::Function(entry_index),
        },
        Export {
            name: "memory".into(),
            kind: ExportKind::Memory,
            index: ExportIndex::Memory(MemoryIndex(0)),
        },
    ];
    let mut minimum = 1;
    let mut realloc = None;
    if needs_realloc {
        let heap_pointer = data_end.next_multiple_of(4);
        let heap_start = (heap_pointer + 4).next_multiple_of(16);
        let realloc_type = TypeIndex(defined + types.len() as u32);
        types.push(FuncType {
            parameters: vec![ValType::I32; 4],
            results: vec![ValType::I32],
        });
        let index = FunctionIndex(entry_index.0 + 1);
        exports.push(Export {
            name: "cabi_realloc".into(),
            kind: ExportKind::Function,
            index: ExportIndex::Function(index),
        });
        data.push(DataSegment {
            id: DataId(data.len() as u32),
            index: DataIndex(data.len() as u32),
            offset: heap_pointer,
            bytes: heap_start.to_le_bytes().to_vec(),
        });
        minimum = (heap_start as u64).div_ceil(0x10000) + 1;
        realloc = Some(build_realloc(realloc_type, heap_pointer, module.span));
    }

    let wasm = Module {
        name: module.name.clone(),
        imports,
        types,
        type_defs,
        functions,
        memories: vec![Memory {
            id: MemoryId(0),
            index: MemoryIndex(0),
            minimum,
            maximum: None,
        }],
        data,
        exports,
        entry: Some(Entry {
            type_index: entry_type,
            body: {
                let mut body = vec![Op::Leaf(Instruction::Call(main_index.0))];
                if let Some(exit_index) = exit_index {
                    body.push(Op::Leaf(Instruction::Call(exit_index.0)));
                    body.push(Op::Leaf(Instruction::I32Const(0)));
                }
                body
            },
        }),
        realloc,
        span: module.span,
    };
    super::verify::verify_module(&wasm)?;
    Ok(wasm)
}

fn collect_function_types(
    module: &mir::Module,
    defined: u32,
    initial_types: Vec<FuncType>,
) -> Result<(Vec<FuncType>, Vec<TypeIndex>), Vec<BackendError>> {
    let mut types = initial_types;
    let mut indices = HashMap::<(Vec<ValType>, Vec<ValType>), TypeIndex>::new();
    for (index, definition) in module
        .types
        .iter()
        .flat_map(|group| group.0.iter())
        .enumerate()
    {
        let CompositeType::Func {
            parameters,
            results,
        } = &definition.composite
        else {
            continue;
        };
        indices.insert(
            (
                parameters.iter().copied().map(val_type).collect(),
                results.iter().copied().map(val_type).collect(),
            ),
            TypeIndex(index as u32),
        );
    }
    let mut function_types = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        if function.parameters.len() > function.values.len() {
            return Err(wasm_error(
                function.span,
                "MIR function has more parameters than values",
            ));
        }
        let mut parameters = Vec::with_capacity(function.parameters.len());
        for parameter in &function.parameters {
            let ty = value_type(function, *parameter).ok_or_else(|| {
                wasm_error(function.span, "MIR function parameter has no value type")
            })?;
            parameters.push(val_type(ty));
        }
        let key = (parameters, vec![val_type(function.result_type)]);
        let type_index = match indices.get(&key) {
            Some(index) => *index,
            None => {
                let index = TypeIndex(defined + types.len() as u32);
                types.push(FuncType {
                    parameters: key.0.clone(),
                    results: key.1.clone(),
                });
                indices.insert(key, index);
                index
            }
        };
        function_types.push(type_index);
    }
    Ok((types, function_types))
}

fn lower_function(
    source: &MirFunction,
    type_index: TypeIndex,
    function_indices: &HashMap<SymbolId, FunctionIndex>,
    string_offsets: &HashMap<String, u32>,
) -> Result<Function, Vec<BackendError>> {
    let locals = local_indices(source)?;
    let parameters = source
        .parameters
        .iter()
        .map(|parameter| {
            value_type(source, *parameter)
                .map(val_type)
                .ok_or_else(|| wasm_error(source.span, "MIR function parameter has no value type"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let local_types = source
        .values
        .iter()
        .skip(source.parameters.len())
        .map(|value| val_type(value.ty))
        .collect::<Vec<_>>();
    let structurer = Structurer {
        function: source,
        blocks: source
            .blocks
            .iter()
            .map(|block| (block.id, block))
            .collect(),
        locals,
        function_indices,
        string_offsets,
    };
    let mut body = Body::new();
    let uses_dispatcher = structurer.emit_control_flow(&mut body)?;
    body.push(Op::Leaf(Instruction::LocalGet(local(
        &structurer.locals,
        source.result,
        source.span,
    )?)));
    let mut locals = local_types;
    if uses_dispatcher {
        locals.push(ValType::I32);
    }
    Ok(Function {
        symbol: source.symbol,
        name: source.name.clone(),
        type_index,
        parameters,
        locals,
        body,
        span: source.span,
    })
}

fn local_indices(function: &MirFunction) -> Result<HashMap<ValueId, u32>, Vec<BackendError>> {
    let locals = function
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| (value.id, index as u32))
        .collect::<HashMap<_, _>>();
    for (index, parameter) in function.parameters.iter().enumerate() {
        if locals.get(parameter).copied() != Some(index as u32) {
            return Err(wasm_error(
                function.span,
                "MIR parameters must occupy the first Wasm local indices",
            ));
        }
    }
    Ok(locals)
}

pub(super) fn local(
    locals: &HashMap<ValueId, u32>,
    value: ValueId,
    span: TextRange,
) -> Result<u32, Vec<BackendError>> {
    locals
        .get(&value)
        .copied()
        .ok_or_else(|| wasm_error(span, "MIR value has no Wasm local index"))
}

fn value_type(function: &MirFunction, value: ValueId) -> Option<ValueType> {
    function
        .values
        .iter()
        .find(|declaration| declaration.id == value)
        .map(|declaration| declaration.ty)
}

pub(super) fn wasm_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P10 Wasm structuring", span, message)]
}
